//! Multiple owned mesh sources in one admitted color/depth pass. This native
//! primitive does not flatten meshes into planes or reinterpret relative AI depth.
use super::*;
use std::{sync::Arc, thread::ThreadId};

/// A source table index may be repeated with independent world transforms and
/// animation samples. Material/mesh storage is shared, frame uniforms are not.
pub struct ScenePassInstance<'a> {
    pub source: usize,
    pub request: SceneRequest,
    pub scene: usize,
    pub animation: Option<motion_scene::AnimationSample>,
    pub pixels_per_metre: f32,
    pub world: Mat4,
    pub lights: &'a [SceneLight],
    pub ambient: [f32; 3],
}
impl ScenePassInstance<'static> {
    pub fn new(source: usize, request: SceneRequest, scene: usize, pixels_per_metre: f32) -> Self {
        Self {
            source,
            request,
            scene,
            animation: None,
            pixels_per_metre,
            world: Mat4::IDENTITY,
            lights: DEFAULT_LIGHTS,
            ambient: [0.03; 3],
        }
    }
}

/// Camera, clear and sample policy belong to the entire pass. Instances cannot
/// silently introduce another camera, clear the depth, or change MSAA midway.
pub struct ScenePassFrame {
    pub camera: motion_model::CameraPose,
    pub background: [f32; 4],
    pub samples: u32,
    pub viewport: Option<[f32; 4]>,
}
impl ScenePassFrame {
    pub fn new(camera: motion_model::CameraPose) -> Self {
        Self {
            camera,
            background: [0.; 4],
            samples: 1,
            viewport: None,
        }
    }
    pub fn from_imported_camera(camera: &motion_scene::CameraView) -> Self {
        Self {
            camera: imported_camera_pose(camera),
            background: [0.; 4],
            samples: 1,
            viewport: Some(camera.viewport),
        }
    }
}

/// Owner-thread-only native access, with depth and optional MSAA storage charged
/// to the renderer's shared admission budget. `max_draws` bounds both instances
/// and the combined primitive queue, including repeated source instances.
pub struct ScenePass {
    owner: Arc<()>,
    thread: ThreadId,
    targets: Option<Targets>,
    depth_input: Option<AdmittedTexture>,
    depth_pipelines: std::collections::HashMap<depth_seed::Key, depth_seed::Pipeline>,
    plane_pipelines: planes::Pipelines,
    max_draws: usize,
    // Consecutive failed pass calls; reset by any successful encode/draw.
    failures: u8,
}
struct Prepared {
    source: usize,
    items: Vec<Item>,
    uniforms: FrameUniforms,
}
enum OrderedDraw<'a> {
    Mesh {
        instance: usize,
        item: usize,
        group: &'a wgpu::BindGroup,
    },
    Plane {
        draw: &'a planes::Draw,
        vertices: &'a crate::AdmittedBuffer,
    },
}
impl ScenePass {
    pub fn new(renderer: &Renderer, max_draws: usize) -> Result<Self> {
        if max_draws == 0 || max_draws > u32::MAX as usize {
            return Err(invalid(
                "shared_limits",
                0,
                "draw allowance must be positive and fit u32",
            ));
        }
        Ok(Self {
            owner: renderer.allocation_owner.clone(),
            thread: std::thread::current().id(),
            targets: None,
            depth_input: None,
            depth_pipelines: Default::default(),
            plane_pipelines: Default::default(),
            max_draws,
            failures: 0,
        })
    }
    pub fn texture_resource_ledger(&self) -> crate::TextureResourceLedger {
        let mut ledger = crate::TextureResourceLedger::default();
        if let Some(targets) = &self.targets {
            targets.collect(&mut ledger);
        }
        if let Some(input) = &self.depth_input {
            input.collect_texture_resources(
                &mut ledger,
                "scene/shared-depth-input",
                crate::TextureState::Retained,
            );
        }
        ledger
    }
    /// Cached native depth/MSAA attachments; an optional explicit depth input
    /// is a separate resource reported by texture_resource_ledger.
    pub(crate) fn attachment_bytes(&self) -> u64 {
        self.targets.as_ref().map_or(0, Targets::bytes)
    }
    /// Actual hardware depth in the common camera's projected [0,1] space.
    /// Single-sample storage permits COPY_SRC; multisample storage has no implicit
    /// depth resolve. Retain an admitted clone/view, never an untracked raw alias.
    /// Both sample counts permit texture sampling of the original depth samples.
    pub fn depth(&self) -> Option<&AdmittedTexture> {
        self.targets.as_ref().map(|targets| &targets._depth)
    }
    pub fn draw(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        target: &RenderTarget,
        sources: &mut [&mut GpuScene],
        instances: &[ScenePassInstance<'_>],
        frame: &ScenePassFrame,
    ) -> Result<SceneRenderStats> {
        self.draw_inner(renderer, request, target, sources, instances, frame, None)
    }
    /// Explicit calibrated full-image depth in the common camera viewport.
    pub fn draw_with_depth(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        target: &RenderTarget,
        sources: &mut [&mut GpuScene],
        instances: &[ScenePassInstance<'_>],
        frame: &ScenePassFrame,
        depth: &depth_seed::SceneDepthInput<'_>,
    ) -> Result<SceneRenderStats> {
        self.draw_inner(
            renderer,
            request,
            target,
            sources,
            instances,
            frame,
            Some(depth),
        )
    }
    fn draw_inner(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        target: &RenderTarget,
        sources: &mut [&mut GpuScene],
        instances: &[ScenePassInstance<'_>],
        frame: &ScenePassFrame,
        depth: Option<&depth_seed::SceneDepthInput<'_>>,
    ) -> Result<SceneRenderStats> {
        let began = Instant::now();
        let result = (|| {
            self.check_owner(renderer)?;
            let mut encoder = renderer.allocation_command_encoder_for(
                crate::gpu_commands::CommandStage::Composition,
                &wgpu::CommandEncoderDescriptor {
                    label: Some("shared scene depth and materials"),
                },
            )?;
            let stats = self.encode_inner(
                renderer,
                request,
                &mut encoder,
                target,
                sources,
                instances,
                frame,
                depth,
                &[],
            )?;
            renderer.submit_allocated(encoder.finish(), |ledger| {
                ledger.merge(self.texture_resource_ledger());
                for source in sources.iter() {
                    ledger.merge(source.texture_resource_ledger());
                }
                target.collect_texture_resources(
                    ledger,
                    "scene/shared-output",
                    crate::TextureState::Retained,
                );
            })?;
            Ok(stats)
        })();
        self.observe(request, began, &result);
        result
    }
    /// Submit through the allocated path. All inputs and uniforms are prepared
    /// before recording the first draw; a rejected frame leaves target pixels and
    /// the previous attachment cache intact. Encoded reservations survive drops.
    pub fn encode(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        encoder: &mut AllocationEncoder,
        target: &RenderTarget,
        sources: &mut [&mut GpuScene],
        instances: &[ScenePassInstance<'_>],
        frame: &ScenePassFrame,
    ) -> Result<SceneRenderStats> {
        self.encode_depth_inner(
            renderer, request, encoder, target, sources, instances, frame, None,
        )
    }
    pub fn encode_with_depth(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        encoder: &mut AllocationEncoder,
        target: &RenderTarget,
        sources: &mut [&mut GpuScene],
        instances: &[ScenePassInstance<'_>],
        frame: &ScenePassFrame,
        depth: &depth_seed::SceneDepthInput<'_>,
    ) -> Result<SceneRenderStats> {
        self.encode_depth_inner(
            renderer,
            request,
            encoder,
            target,
            sources,
            instances,
            frame,
            Some(depth),
        )
    }
    fn encode_depth_inner(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        encoder: &mut AllocationEncoder,
        target: &RenderTarget,
        sources: &mut [&mut GpuScene],
        instances: &[ScenePassInstance<'_>],
        frame: &ScenePassFrame,
        depth: Option<&depth_seed::SceneDepthInput<'_>>,
    ) -> Result<SceneRenderStats> {
        let began = Instant::now();
        let result = encoder
            .check_owner(&renderer.allocation_owner)
            .map_err(super::super::SceneGpuError::from)
            .and_then(|()| {
                self.encode_inner(
                    renderer,
                    request,
                    encoder,
                    target,
                    sources,
                    instances,
                    frame,
                    depth,
                    &[],
                )
            });
        self.observe(request, began, &result);
        result
    }
    /// Internal bridge for Renderer::encode's existing encoder contract. The
    /// enclosing allocation encoder remains alive while all native resources
    /// are admitted; no intermediate submission or mutable raster upload occurs.
    pub(crate) fn encode_composition(
        &mut self,
        renderer: &Renderer,
        request: SceneRequest,
        encoder: &mut wgpu::CommandEncoder,
        target: &RenderTarget,
        sources: &mut [&mut GpuScene],
        instances: &[ScenePassInstance<'_>],
        frame: &ScenePassFrame,
        planes: &[ScenePlane],
    ) -> Result<SceneRenderStats> {
        let began = Instant::now();
        let result = self.encode_inner(
            renderer, request, encoder, target, sources, instances, frame, None, planes,
        );
        self.observe(request, began, &result);
        result
    }
    fn check_owner(&self, renderer: &Renderer) -> Result<()> {
        if std::thread::current().id() != self.thread {
            return Err(invalid(
                "shared_thread",
                0,
                "scene pass requires its owning thread",
            ));
        }
        if !Arc::ptr_eq(&self.owner, &renderer.allocation_owner) {
            return Err(invalid(
                "shared_owner",
                0,
                "scene pass belongs to another renderer",
            ));
        }
        Ok(())
    }
    fn observe(
        &mut self,
        request: SceneRequest,
        began: Instant,
        result: &Result<SceneRenderStats>,
    ) {
        if let Err(cause) = result {
            self.failures = self.failures.saturating_add(1).min(61);
            if self.failures == 1 || self.failures == 60 {
                log_failure(request, "scene_shared_pass", began, cause);
            }
        } else {
            self.failures = 0;
            if began.elapsed() >= std::time::Duration::from_millis(100) {
                log::debug!(
                    "done #{} scene_shared_pass source={} elapsed_ms={}",
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
        encoder: &mut wgpu::CommandEncoder,
        target: &RenderTarget,
        sources: &mut [&mut GpuScene],
        instances: &[ScenePassInstance<'_>],
        frame: &ScenePassFrame,
        depth: Option<&depth_seed::SceneDepthInput<'_>>,
        planes: &[ScenePlane],
    ) -> Result<SceneRenderStats> {
        self.check_owner(renderer)?;
        renderer.check_health()?;
        target.check_owner(renderer)?;
        let texture: &wgpu::Texture = target.texture();
        if target.width != texture.width()
            || target.height != texture.height()
            || target.view.texture() != texture
        {
            return Err(invalid(
                "shared_target",
                0,
                "target metadata/view was changed",
            ));
        }
        let mut common = SceneFrame::new(frame.camera, 0, 1.);
        common.samples = frame.samples;
        common.background = frame.background;
        validate_frame(&common)?;
        validate_viewport(frame.viewport, target)?;
        if instances
            .len()
            .checked_add(planes.len())
            .is_none_or(|count| count > self.max_draws)
        {
            return Err(invalid(
                "shared_instances",
                0,
                "combined instance limit exceeded",
            ));
        }
        // Validate source/camera/units and admit the immutable field before mesh
        // uniforms, queue writes to existing outputs, or target recording.
        let seed = depth
            .map(|input| {
                depth_seed::prepare(renderer, &mut self.depth_pipelines, target, frame, input)
            })
            .transpose()?;
        for (index, instance) in instances.iter().enumerate() {
            let source = sources.get(instance.source).ok_or_else(|| {
                invalid(
                    "shared_source",
                    instance.source,
                    "source table index is absent",
                )
            })?;
            source
                .check_owner(renderer)
                .map_err(|cause| instance_error(index, instance, cause))?;
        }
        let planes = planes::prepare(
            renderer,
            request,
            texture.format(),
            frame,
            planes,
            &mut self.plane_pipelines,
        )?;
        let mut prepared = Vec::new();
        prepared
            .try_reserve_exact(instances.len())
            .map_err(|_| invalid("shared_instances", 0, "instance staging allocation failed"))?;
        let mut stats = SceneRenderStats::default();
        if let Some(planes) = &planes {
            stats.draws = planes.count;
            stats.triangles = planes.count as u64 * 2;
            stats.plane_vertex_bytes = planes.vertices.size();
        }
        stats.depth_input_bytes = seed.as_ref().map_or(0, |s| s.bytes);
        let mut used_sources = HashSet::new();
        for (index, instance) in instances.iter().enumerate() {
            let source = &mut sources[instance.source];
            let local = SceneFrame {
                scene: instance.scene,
                animation: instance.animation,
                camera: frame.camera,
                pixels_per_metre: instance.pixels_per_metre,
                world: instance.world,
                lights: instance.lights,
                ambient: instance.ambient,
                background: frame.background,
                samples: frame.samples,
                viewport: frame.viewport,
            };
            let items = source
                .prepare_items(
                    renderer,
                    instance.request,
                    texture.format(),
                    &local,
                    self.max_draws - stats.draws,
                )
                .map_err(|cause| instance_error(index, instance, cause))?;
            let uniforms = source
                .uniforms(renderer, &items)
                .map_err(|cause| instance_error(index, instance, cause))?;
            stats.draws += items.len();
            stats.deformation_bytes = items
                .iter()
                .filter_map(|item| item.vertices.as_ref())
                .try_fold(stats.deformation_bytes, |n, v| n.checked_add(v.size()))
                .ok_or_else(|| {
                    invalid("shared_morph", index, "combined vertex capacity overflow")
                })?;
            stats.parameter_bytes = stats
                .parameter_bytes
                .checked_add(uniforms.size)
                .ok_or_else(|| {
                    invalid("shared_uniforms", index, "combined uniform size overflow")
                })?;
            if used_sources.insert(instance.source) {
                stats.resident_bytes = stats
                    .resident_bytes
                    .checked_add(source.resident_bytes)
                    .ok_or_else(|| invalid("shared_sources", index, "source capacity overflow"))?;
            }
            prepared.push(Prepared {
                source: instance.source,
                items,
                uniforms,
            });
        }
        let mut order = Vec::new();
        order
            .try_reserve_exact(stats.draws)
            .map_err(|_| invalid("shared_sort", 0, "draw queue allocation failed"))?;
        for (instance, prepared) in prepared.iter().enumerate() {
            if !prepared.items.is_empty() {
                let group = prepared.uniforms.group.as_ref().ok_or_else(|| {
                    invalid(
                        "shared_uniforms",
                        instance,
                        "nonempty draw queue lacks uniforms",
                    )
                })?;
                order.extend(prepared.items.iter().enumerate().map(|(index, item)| {
                    (
                        item.key.blend,
                        item.distance,
                        OrderedDraw::Mesh {
                            instance,
                            item: index,
                            group,
                        },
                    )
                }));
            }
        }
        if let Some(planes) = &planes {
            order.extend(planes.draws.iter().map(|draw| {
                (
                    draw.blend,
                    draw.distance,
                    OrderedDraw::Plane {
                        draw,
                        vertices: &planes.vertices,
                    },
                )
            }));
        }
        // Global ordering, including different meshes/sources and repeated instances.
        // Intersecting transparent triangles still require an explicit OIT policy.
        order.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| {
                    if a.0 {
                        b.1.total_cmp(&a.1)
                    } else {
                        a.1.total_cmp(&b.1)
                    }
                })
                .then_with(|| {
                    // Preserve the prior opaque coplanar tie: planes were encoded
                    // first and the mesh won an equal LessEqual depth comparison.
                    if a.0 {
                        std::cmp::Ordering::Equal
                    } else {
                        matches!(a.2, OrderedDraw::Mesh { .. })
                            .cmp(&matches!(b.2, OrderedDraw::Mesh { .. }))
                    }
                })
        });
        let replacement = if self.targets.as_ref().is_some_and(|t| {
            t.size == (target.width, target.height)
                && t.format == texture.format()
                && t.samples == frame.samples
        }) {
            None
        } else {
            Some(Targets::new(renderer, target, frame.samples, true)?)
        };
        let targets = replacement
            .as_ref()
            .or(self.targets.as_ref())
            .ok_or_else(|| invalid("shared_targets", 0, "shared attachments missing"))?;
        if let Some((seed, input)) = seed.as_ref().zip(depth) {
            depth_seed::upload(renderer, seed, input)?;
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("globally ordered shared scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: targets.color.as_ref().map_or(&*target.view, |v| &**v),
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
            if let Some(seed) = &seed {
                pass.set_pipeline(&seed.pipeline);
                pass.set_bind_group(0, &seed.group, &[]);
                pass.draw(0..3, 0..1);
            }
            for (_, _, draw) in order {
                let (instance_index, item_index, group) = match draw {
                    OrderedDraw::Plane { draw, vertices } => {
                        pass.set_pipeline(&draw.pipeline);
                        if let Some(group) = &draw.group {
                            pass.set_bind_group(0, group, &[]);
                        }
                        pass.set_vertex_buffer(0, vertices.slice(draw.vertices.clone()));
                        pass.draw(0..6, 0..1);
                        stats.blended_draws += usize::from(draw.blend);
                        continue;
                    }
                    OrderedDraw::Mesh {
                        instance,
                        item,
                        group,
                    } => (instance, item, group),
                };
                let prepared = &prepared[instance_index];
                let item = &prepared.items[item_index];
                let source = &sources[prepared.source];
                let primitive = &source.meshes[item.mesh][item.primitive];
                pass.set_pipeline(&source.pipelines[&item.key]);
                pass.set_bind_group(
                    0,
                    group,
                    &[(item_index as u64 * prepared.uniforms.stride) as u32],
                );
                pass.set_bind_group(1, &source.materials[primitive.material].group, &[]);
                pass.set_vertex_buffer(
                    0,
                    item.vertices
                        .as_ref()
                        .unwrap_or(&primitive.vertices)
                        .slice(..),
                );
                pass.set_index_buffer(primitive.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..primitive.index_count, 0, 0..1);
                stats.triangles += u64::from(primitive.index_count / 3);
                stats.blended_draws += usize::from(item.key.blend);
            }
        }
        if let Some(targets) = replacement {
            self.targets = Some(targets);
        }
        self.depth_input = seed.map(|seed| seed.texture);
        Ok(stats)
    }
}
fn instance_error(
    index: usize,
    instance: &ScenePassInstance<'_>,
    cause: super::super::SceneGpuError,
) -> super::super::SceneGpuError {
    super::super::SceneGpuError::PassInstance {
        instance: index,
        request: instance.request.request,
        source_id: instance.request.source,
        cause: Box::new(cause),
    }
}
