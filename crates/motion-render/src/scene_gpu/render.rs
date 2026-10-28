use super::{invalid, log_failure, unsupported, GpuScene, Result, SceneRequest, Vertex};
use crate::{
    AdmittedTexture, AdmittedTextureView, AllocationEncoder, RenderTarget, Renderer,
    TextureCategory,
};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use motion_scene::AlphaMode;
use std::{collections::HashSet, time::Instant};

mod depth_seed;
mod planes;
mod shared;
pub(crate) use planes::ScenePlane;
pub use depth_seed::SceneDepthInput;
pub use shared::{ScenePass, ScenePassFrame, ScenePassInstance};

const MAX_LIGHTS: usize = 4;
const DEFAULT_LIGHTS: &[SceneLight] = &[SceneLight::Directional {
    direction: [0.2, -0.5, -1.],
    color: [1.; 3],
    intensity: 3.,
}];
#[derive(Clone, Copy, Debug)]
pub enum SceneLight {
    /// Direction in which rays travel, in the same right-handed world as the camera.
    Directional {
        direction: [f32; 3],
        color: [f32; 3],
        intensity: f32,
    },
    /// Position in world pixels; inverse-square distance is converted to metres.
    Point {
        position: [f32; 3],
        color: [f32; 3],
        intensity: f32,
        range_metres: Option<f32>,
    },
}
pub struct SceneFrame<'a> {
    pub scene: usize,
    pub animation: Option<motion_scene::AnimationSample>,
    pub camera: motion_model::CameraPose,
    pub pixels_per_metre: f32,
    pub world: Mat4,
    pub lights: &'a [SceneLight],
    /// Constant linear irradiance approximation; this is not image-based lighting.
    pub ambient: [f32; 3],
    /// Linear straight RGB and alpha; clear converts it to premultiplied storage.
    pub background: [f32; 4],
    /// Explicit 1 or 4, with refusal rather than an implicit MSAA fallback.
    pub samples: u32,
    /// Native-pixel camera viewport; None uses the complete target.
    pub viewport: Option<[f32; 4]>,
}
impl SceneFrame<'static> {
    pub fn new(camera: motion_model::CameraPose, scene: usize, pixels_per_metre: f32) -> Self {
        Self {
            scene,
            animation: None,
            camera,
            pixels_per_metre,
            world: Mat4::IDENTITY,
            lights: DEFAULT_LIGHTS,
            ambient: [0.03; 3],
            background: [0.; 4],
            samples: 1,
            viewport: None,
        }
    }
}
impl SceneFrame<'static> {
    pub fn from_imported_camera(
        camera: &motion_scene::CameraView,
        scene: usize,
        pixels_per_metre: f32,
    ) -> Self {
        let mut frame = Self::new(imported_camera_pose(camera), scene, pixels_per_metre);
        frame.viewport = Some(camera.viewport);
        frame
    }
}
fn imported_camera_pose(camera: &motion_scene::CameraView) -> motion_model::CameraPose {
    motion_model::CameraPose {
        eye: camera.eye,
        target: camera.target,
        view_projection: camera.view_projection,
        projection: match camera.kind {
            motion_scene::CameraKind::Perspective => motion_model::ProjectionKind::Perspective,
            motion_scene::CameraKind::Orthographic => motion_model::ProjectionKind::Orthographic,
        },
    }
}
#[derive(Debug, Default)]
pub struct SceneRenderStats {
    pub draws: usize,
    pub triangles: u64,
    pub blended_draws: usize,
    pub parameter_bytes: u64,
    pub resident_bytes: u64,
    /// Immutable per-node displaced vertices, separately admitted per encode.
    pub deformation_bytes: u64,
    /// Immutable calibrated clip-depth/validity texture and viewport constants.
    pub depth_input_bytes: u64,
    /// Immutable actual plane vertices for mixed model/plane depth composition.
    pub plane_vertex_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Light {
    position_kind: [f32; 4],
    color_intensity: [f32; 4],
    parameters: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Draw {
    view_projection: [[f32; 4]; 4],
    world: [[f32; 4]; 4],
    normal_world: [[f32; 4]; 4],
    eye_scale: [f32; 4],
    ambient: [f32; 4],
    flags: [u32; 4],
    lights: [Light; MAX_LIGHTS],
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct PipelineKey {
    format: wgpu::TextureFormat,
    samples: u32,
    blend: bool,
    double: bool,
    mirrored: bool,
}
pub(super) struct Targets {
    size: (u32, u32),
    format: wgpu::TextureFormat,
    samples: u32,
    depth: AdmittedTextureView,
    _depth: AdmittedTexture,
    color: Option<AdmittedTextureView>,
    _color: Option<AdmittedTexture>,
    bytes: u64,
}
impl Targets {
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn collect(&self, ledger: &mut crate::TextureResourceLedger) {
        self._depth
            .collect_texture_resources(ledger, "scene/depth", crate::TextureState::Retained);
        if let Some(color) = &self._color {
            color.collect_texture_resources(ledger, "scene/msaa", crate::TextureState::Retained);
        }
    }
}
struct Item {
    mesh: usize,
    primitive: usize,
    uniform: Draw,
    key: PipelineKey,
    distance: f32,
    vertices: Option<crate::AdmittedBuffer>,
}

pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene per-draw transforms and lights"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Draw>() as u64),
            },
            count: None,
        }],
    })
}
impl GpuScene {
    fn check_owner(&self, renderer: &Renderer) -> Result<()> {
        // SessionRegistry stores Renderer behind a Mutex, requiring Send. This
        // identity is movable metadata; native access remains owner-thread-only.
        // AllocationContext itself retains its compile-time !Send contract.
        if std::thread::current().id() != self.thread {
            return Err(invalid(
                "thread",
                0,
                "scene access requires its owning thread",
            ));
        }
        if !std::sync::Arc::ptr_eq(&self.owner, &renderer.allocation_owner) {
            return Err(invalid("owner", 0, "scene belongs to another renderer"));
        }
        Ok(())
    }
    /// Encode and submit one scene. Healthy frames perform no file IO/logging.
    pub fn draw(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        target: &RenderTarget,
        frame: &SceneFrame<'_>,
    ) -> Result<SceneRenderStats> {
        let began = Instant::now();
        let result = (|| {
            self.check_owner(renderer)?;
            let mut encoder = renderer.allocation_command_encoder_for(
                crate::gpu_commands::CommandStage::Composition,
                &wgpu::CommandEncoderDescriptor {
                    label: Some("static 3D scene"),
                },
            )?;
            let result = self.encode_inner(renderer, request, &mut encoder, target, frame)?;
            renderer.submit_allocated(encoder.finish(), |ledger| {
                ledger.merge(self.texture_resource_ledger());
                target.collect_texture_resources(
                    ledger,
                    "scene/output",
                    crate::TextureState::Retained,
                );
            })?;
            Ok(result)
        })();
        self.observe_frame(request, began, &result);
        result
    }
    /// Caller must submit this encoder through the renderer's allocated path.
    /// Per-call immutable uniforms make two encodes before submission independent.
    pub fn encode(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        encoder: &mut AllocationEncoder,
        target: &RenderTarget,
        frame: &SceneFrame<'_>,
    ) -> Result<SceneRenderStats> {
        let began = Instant::now();
        let result = self.encode_inner(renderer, request, encoder, target, frame);
        self.observe_frame(request, began, &result);
        result
    }
    fn observe_frame(
        &mut self,
        request: SceneRequest,
        began: Instant,
        result: &Result<SceneRenderStats>,
    ) {
        if let Err(cause) = result {
            self.render_failures = self.render_failures.saturating_add(1).min(61);
            if self.render_failures == 1 || self.render_failures == 60 {
                log_failure(request, "scene_gpu_frame", began, cause);
            }
        } else {
            self.render_failures = 0;
            if began.elapsed() >= std::time::Duration::from_millis(100) {
                log::debug!(
                    "done #{} scene_gpu_frame source={} elapsed_ms={}",
                    request.request,
                    request.source,
                    began.elapsed().as_millis()
                );
            }
        }
    }
    fn encode_inner(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        encoder: &mut AllocationEncoder,
        target: &RenderTarget,
        frame: &SceneFrame<'_>,
    ) -> Result<SceneRenderStats> {
        self.check_owner(renderer)?;
        renderer.check_health()?;
        encoder.check_owner(&renderer.allocation_owner)?;
        target.check_owner(renderer)?;
        let texture: &wgpu::Texture = target.texture();
        if target.width != texture.width()
            || target.height != texture.height()
            || target.view.texture() != texture
        {
            return Err(invalid("target", 0, "target metadata/view was changed"));
        }
        validate_viewport(frame.viewport, target)?;
        let items = self.prepare_items(
            renderer,
            request,
            texture.format(),
            frame,
            self.limits.max_draws,
        )?;
        self.ensure_targets(renderer, target, frame.samples)?;
        let targets = self
            .targets
            .as_ref()
            .ok_or_else(|| invalid("targets", 0, "scene targets missing"))?;
        let FrameUniforms {
            buffer,
            group,
            stride,
            size,
        } = self.uniforms(renderer, &items)?;
        let color = targets.color.as_ref().map_or(&*target.view, |v| &**v);
        let mut stats = SceneRenderStats {
            plane_vertex_bytes: 0,
            draws: items.len(),
            triangles: 0,
            blended_draws: 0,
            parameter_bytes: size,
            depth_input_bytes: 0,
            resident_bytes: self.resident_bytes,
            deformation_bytes: items
                .iter()
                .filter_map(|item| item.vertices.as_ref())
                .map(|v| v.size())
                .sum(),
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene depth and material pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: targets.color.as_ref().map(|_| &*target.view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(frame.background[0] * frame.background[3]),
                            g: f64::from(frame.background[1] * frame.background[3]),
                            b: f64::from(frame.background[2] * frame.background[3]),
                            a: f64::from(frame.background[3]),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some([x, y, w, h]) = frame.viewport {
                pass.set_viewport(x, y, w, h, 0., 1.);
            }
            if let Some(group) = &group {
                for (i, item) in items.iter().enumerate() {
                    let p = &self.meshes[item.mesh][item.primitive];
                    let material = &self.materials[p.material];
                    pass.set_pipeline(&self.pipelines[&item.key]);
                    pass.set_bind_group(0, group, &[(i as u64 * stride) as u32]);
                    pass.set_bind_group(1, &material.group, &[]);
                    pass.set_vertex_buffer(
                        0,
                        item.vertices.as_ref().unwrap_or(&p.vertices).slice(..),
                    );
                    pass.set_index_buffer(p.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..p.index_count, 0, 0..1);
                    stats.triangles += u64::from(p.index_count / 3);
                    stats.blended_draws += usize::from(item.key.blend);
                }
            }
        }
        drop(buffer);
        Ok(stats)
    }
    fn prepare_items(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        format: wgpu::TextureFormat,
        frame: &SceneFrame<'_>,
        max_draws: usize,
    ) -> Result<Vec<Item>> {
        let (forward, lights) = validate_frame(frame)?;
        let sampled = self.source.mesh_instances_with_skins_at(
            frame.scene,
            frame.pixels_per_metre,
            frame.animation,
        )?;
        let mut items = Vec::new();
        let mut needed = HashSet::new();
        let mut frame_bytes = self.resident_bytes;
        for instance in sampled.meshes {
            let mesh = instance.mesh;
            for (primitive, p) in self.meshes[mesh].iter().enumerate() {
                let primitive_source = &self.source.meshes()[mesh].primitives[primitive];
                let palette = if primitive_source.joint_influences.is_empty() {
                    None
                } else {
                    instance.skin.and_then(|s| sampled.skins[s].as_deref())
                };
                let world = frame.world
                    * if palette.is_some() {
                        Mat4::from_scale(Vec3::splat(frame.pixels_per_metre))
                    } else {
                        instance.world
                    };
                let determinant = world.determinant();
                // A fully collapsed animated hierarchy has no visible geometry.
                // Other singular transforms retain the explicit shading refusal below.
                if frame.animation.is_some()
                    && world.is_finite()
                    && [world.x_axis, world.y_axis, world.z_axis]
                        .iter()
                        .all(|axis| axis.truncate() == Vec3::ZERO)
                {
                    continue;
                }
                if !world.is_finite() || !determinant.is_finite() || determinant == 0. {
                    return Err(invalid(
                        "transform",
                        mesh,
                        "singular or overflowing world transform cannot shade a mesh",
                    ));
                }
                let normal_world = world.inverse().transpose();
                if !normal_world.is_finite() {
                    return Err(invalid("transform", mesh, "normal transform overflow"));
                }
                if items.len() >= self.limits.max_draws.min(max_draws) {
                    return Err(invalid(
                        "instances",
                        mesh,
                        "scene draw count limit exceeded",
                    ));
                }
                let m = &self.materials[p.material];
                let (vertices, local_center) = if instance.has_active_morph() || palette.is_some() {
                    let cost = (primitive_source.positions.len() as u64)
                        .checked_mul(std::mem::size_of::<Vertex>() as u64)
                        .ok_or_else(|| invalid("morph", mesh, "vertex capacity overflow"))?;
                    let (buffer, center) = (|| {
                        super::charge(&mut frame_bytes, cost, self.limits)?;
                        super::meshes::deformed(
                            &renderer.allocation_context(),
                            primitive_source,
                            &instance.weights,
                            palette,
                        )
                    })()
                    .map_err(|cause| {
                        if palette.is_some() {
                            super::SceneGpuError::SkinDeformation {
                                node: instance.node,
                                mesh,
                                primitive,
                                cause: Box::new(cause),
                            }
                        } else {
                            super::SceneGpuError::Deformation {
                                node: instance.node,
                                mesh,
                                primitive,
                                cause: Box::new(cause),
                            }
                        }
                    })?;
                    (Some(buffer), center)
                } else {
                    (None, p.center)
                };
                let center = world.transform_point3(local_center);
                let distance = (center - frame.camera.eye).dot(forward);
                if !center.is_finite() || !distance.is_finite() {
                    return Err(invalid("sort", mesh, "primitive depth overflow"));
                }
                let key = PipelineKey {
                    format,
                    samples: frame.samples,
                    blend: m.alpha == AlphaMode::Blend,
                    double: m.double_sided,
                    mirrored: determinant < 0.,
                };
                needed.insert(key);
                let uniform = Draw {
                    view_projection: frame.camera.view_projection.to_cols_array_2d(),
                    world: world.to_cols_array_2d(),
                    normal_world: normal_world.to_cols_array_2d(),
                    eye_scale: [
                        frame.camera.eye.x,
                        frame.camera.eye.y,
                        frame.camera.eye.z,
                        frame.pixels_per_metre,
                    ],
                    ambient: [frame.ambient[0], frame.ambient[1], frame.ambient[2], 0.],
                    flags: [
                        u32::from(p.normal),
                        u32::from(p.tangent),
                        frame.lights.len() as u32,
                        u32::from(determinant < 0.),
                    ],
                    lights,
                };
                items.push(Item {
                    mesh,
                    primitive,
                    uniform,
                    key,
                    distance,
                    vertices,
                });
            }
        }
        // Keep opaque/masked first; stable far-to-near primitive ordering for blend.
        // Intersecting transparent triangles require a later OIT policy, never BSP mutation.
        items.sort_by(|a, b| {
            a.key.blend.cmp(&b.key.blend).then_with(|| {
                if a.key.blend {
                    b.distance.total_cmp(&a.distance)
                } else {
                    a.distance.total_cmp(&b.distance)
                }
            })
        });
        for key in needed {
            if !self.pipelines.contains_key(&key) {
                let began = Instant::now();
                log::info!("start #{} scene_gpu_pipeline source={} format={:?} samples={} blend={} double={} mirrored={}",request.request,request.source,key.format,key.samples,key.blend,key.double,key.mirrored);
                let pipeline = self.pipeline(renderer, key)?;
                self.pipelines.insert(key, pipeline);
                log::debug!(
                    "done #{} scene_gpu_pipeline source={} elapsed_ms={}",
                    request.request,
                    request.source,
                    began.elapsed().as_millis()
                );
            }
        }
        Ok(items)
    }
    fn uniforms(&self, renderer: &Renderer, items: &[Item]) -> Result<FrameUniforms> {
        let stride = (std::mem::size_of::<Draw>() as u64).div_ceil(u64::from(
            renderer.device.limits().min_uniform_buffer_offset_alignment,
        )) * u64::from(renderer.device.limits().min_uniform_buffer_offset_alignment);
        let size = stride
            .checked_mul(items.len() as u64)
            .filter(|&n| n <= renderer.device.limits().max_buffer_size && n <= u64::from(u32::MAX))
            .ok_or_else(|| invalid("uniforms", 0, "draw uniform capacity exceeds native limits"))?;
        let (buffer, group) = if items.is_empty() {
            (None, None)
        } else {
            let mut staging = Vec::new();
            staging
                .try_reserve_exact(size as usize)
                .map_err(|_| invalid("uniforms", 0, "draw staging allocation failed"))?;
            staging.resize(size as usize, 0);
            for (i, item) in items.iter().enumerate() {
                let start = i * stride as usize;
                let bytes = bytemuck::bytes_of(&item.uniform);
                staging[start..start + bytes.len()].copy_from_slice(bytes);
            }
            let buffer =
                renderer
                    .allocation_context()
                    .buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("immutable scene frame transforms"),
                        contents: &staging,
                        usage: wgpu::BufferUsages::UNIFORM,
                    })?;
            let group = renderer
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("scene frame transforms"),
                    layout: &self.draw_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(std::mem::size_of::<Draw>() as u64),
                        }),
                    }],
                });
            (Some(buffer), Some(group))
        };
        Ok(FrameUniforms {
            buffer,
            group,
            stride,
            size,
        })
    }
    fn pipeline(&self, renderer: &Renderer, key: PipelineKey) -> Result<wgpu::RenderPipeline> {
        let features = features(renderer, key.format);
        if !features
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            || !features.flags.sample_count_supported(key.samples)
            || (key.samples == 4
                && !features
                    .flags
                    .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE))
        {
            return Err(unsupported(0, "requested target format/MSAA combination"));
        }
        if key.blend
            && !features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
        {
            return Err(unsupported(
                0,
                "requested color format does not support enabled blending",
            ));
        }
        let oom = renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = renderer
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("static glTF and Wavefront materials"),
                source: wgpu::ShaderSource::Wgsl(include_str!("material.wgsl").into()),
            });
        let layout = renderer
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("scene materials and lights"),
                bind_group_layouts: &[Some(&self.draw_layout), Some(&self.material_layout)],
                immediate_size: 0,
            });
        let pipeline=renderer.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some("scene shaded triangles"),layout:Some(&layout),vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout{array_stride:std::mem::size_of::<Vertex>() as u64,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x4,3=>Float32x2,4=>Float32x2,5=>Float32x4]})]},fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState{format:key.format,blend:key.blend.then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),primitive:wgpu::PrimitiveState{front_face:if key.mirrored{wgpu::FrontFace::Cw}else{wgpu::FrontFace::Ccw},cull_mode:if key.double{None}else{Some(wgpu::Face::Back)},..Default::default()},depth_stencil:Some(wgpu::DepthStencilState{format:wgpu::TextureFormat::Depth32Float,depth_write_enabled:Some(!key.blend),depth_compare:Some(wgpu::CompareFunction::LessEqual),stencil:Default::default(),bias:Default::default()}),multisample:wgpu::MultisampleState{count:key.samples,..Default::default()},multiview_mask:None,cache:None});
        let validation_cause = pollster::block_on(validation.pop());
        let oom_cause = pollster::block_on(oom.pop());
        let cause = validation_cause.or(oom_cause);
        if let Some(cause) = cause {
            return Err(cause.into());
        }
        Ok(pipeline)
    }
    fn ensure_targets(
        &mut self,
        renderer: &Renderer,
        target: &RenderTarget,
        samples: u32,
    ) -> Result<()> {
        let format = target.texture().format();
        let size = (target.width, target.height);
        if self
            .targets
            .as_ref()
            .is_some_and(|t| t.size == size && t.format == format && t.samples == samples)
        {
            return Ok(());
        }
        self.targets = Some(Targets::new(renderer, target, samples, false)?);
        Ok(())
    }
}
struct FrameUniforms {
    buffer: Option<crate::AdmittedBuffer>,
    group: Option<wgpu::BindGroup>,
    stride: u64,
    size: u64,
}
impl Targets {
    fn new(
        renderer: &Renderer,
        target: &RenderTarget,
        samples: u32,
        copy_depth: bool,
    ) -> Result<Self> {
        let format = target.texture().format();
        let size = (target.width, target.height);
        if !features(renderer, wgpu::TextureFormat::Depth32Float)
            .flags
            .sample_count_supported(samples)
        {
            return Err(unsupported(0, "requested depth sample count"));
        }
        if copy_depth && !features(renderer, wgpu::TextureFormat::Depth32Float)
            .allowed_usages.contains(wgpu::TextureUsages::TEXTURE_BINDING) {
            return Err(unsupported(0, "shared scene depth sampling"));
        }
        let descriptor = |format, label| wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | if copy_depth && format == wgpu::TextureFormat::Depth32Float {
                    wgpu::TextureUsages::TEXTURE_BINDING
                } else {
                    wgpu::TextureUsages::empty()
                }
                | if copy_depth && samples == 1 && format == wgpu::TextureFormat::Depth32Float {
                    wgpu::TextureUsages::COPY_SRC
                } else {
                    wgpu::TextureUsages::empty()
                },
            view_formats: &[],
        };
        let context = renderer.allocation_context();
        let depth = context.texture_in(
            &descriptor(wgpu::TextureFormat::Depth32Float, "scene depth"),
            TextureCategory::LayerCompositingScratch,
        )?;
        let color = if samples == 4 {
            Some(context.texture_in(
                &descriptor(format, "scene MSAA color"),
                TextureCategory::LayerCompositingScratch,
            )?)
        } else {
            None
        };
        let depth_bytes = crate::resource_ledger::logical_bytes(&depth)
            .ok_or_else(|| invalid("targets", 0, "cannot account scene depth storage"))?;
        let color_bytes = color
            .as_ref()
            .map(|texture| {
                crate::resource_ledger::logical_bytes(texture).ok_or_else(|| {
                    invalid("targets", 0, "cannot account scene multisample storage")
                })
            })
            .transpose()?
            .unwrap_or(0);
        let bytes = depth_bytes
            .checked_add(color_bytes)
            .ok_or_else(|| invalid("targets", 0, "scene target capacity overflow"))?;
        Ok(Targets {
            size,
            format,
            samples,
            depth: depth.view()?,
            _depth: depth,
            color: color.as_ref().map(|c| c.view()).transpose()?,
            _color: color,
            bytes,
        })
    }
}
fn validate_frame(frame: &SceneFrame<'_>) -> Result<(Vec3, [Light; MAX_LIGHTS])> {
    if !matches!(frame.samples, 1 | 4) {
        return Err(unsupported(
            0,
            "scene sample count must be explicitly 1 or 4",
        ));
    }
    if !frame.world.is_finite()
        || !frame.camera.view_projection.is_finite()
        || !frame.camera.eye.is_finite()
        || !frame.camera.target.is_finite()
    {
        return Err(invalid("frame", 0, "non-finite camera/world transform"));
    }
    if frame.world.row(3) != glam::Vec4::W {
        return Err(invalid("frame", 0, "world transform must be affine"));
    }
    let projection_determinant = frame.camera.view_projection.determinant();
    if !projection_determinant.is_finite() || projection_determinant == 0. {
        return Err(invalid("camera", 0, "camera projection must be invertible"));
    }
    if frame
        .ambient
        .iter()
        .chain(&frame.background)
        .any(|v| !v.is_finite() || *v < 0.)
        || frame.background[3] > 1.
    {
        return Err(invalid("frame", 0, "invalid ambient/background components"));
    }
    let forward = (frame.camera.target - frame.camera.eye)
        .try_normalize()
        .ok_or_else(|| invalid("camera", 0, "camera eye and target must define a direction"))?;
    let lights = lights(frame)?;
    Ok((forward, lights))
}
fn validate_viewport(viewport: Option<[f32; 4]>, target: &RenderTarget) -> Result<()> {
    if viewport.is_some_and(|[x, y, w, h]| {
        [x, y, w, h].iter().any(|v| !v.is_finite())
            || x < 0.
            || y < 0.
            || w <= 0.
            || h <= 0.
            || x + w > target.width as f32
            || y + h > target.height as f32
    }) {
        return Err(invalid(
            "viewport",
            0,
            "camera viewport exceeds target or has invalid components",
        ));
    }
    Ok(())
}
fn features(renderer: &Renderer, format: wgpu::TextureFormat) -> wgpu::TextureFormatFeatures {
    if renderer
        .device
        .features()
        .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    {
        renderer.adapter.get_texture_format_features(format)
    } else {
        format.guaranteed_format_features(renderer.device.features())
    }
}
fn lights(frame: &SceneFrame<'_>) -> Result<[Light; MAX_LIGHTS]> {
    if frame.lights.len() > MAX_LIGHTS {
        return Err(unsupported(
            0,
            "at most four explicit lights per scene frame",
        ));
    }
    let mut out = [Light::zeroed(); MAX_LIGHTS];
    for (i, light) in frame.lights.iter().enumerate() {
        let (position, kind, color, intensity, range) = match *light {
            SceneLight::Directional {
                direction,
                color,
                intensity,
            } => (
                Vec3::from_array(direction).try_normalize().ok_or_else(|| {
                    invalid("light", i, "light direction must be finite and nonzero")
                })?,
                0.,
                color,
                intensity,
                0.,
            ),
            SceneLight::Point {
                position,
                color,
                intensity,
                range_metres,
            } => (
                Vec3::from_array(position),
                1.,
                color,
                intensity,
                range_metres.unwrap_or(0.),
            ),
        };
        if !position.is_finite()
            || color
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || !intensity.is_finite()
            || !(0. ..=1e6).contains(&intensity)
            || !range.is_finite()
            || range < 0.
        {
            return Err(invalid("light", i, "invalid light color/intensity/range"));
        }
        out[i] = Light {
            position_kind: [position.x, position.y, position.z, kind],
            color_intensity: [color[0], color[1], color[2], intensity],
            parameters: [range, 0., 0., 0.],
        };
    }
    Ok(out)
}
