//! Aperture resolve over the actual common-camera depth attachment. Every
//! intermediate image and camera block belongs to one encode, including MSAA.
use super::*;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct Key(wgpu::TextureFormat, u32);
pub(super) struct Pipeline {
    render: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    inverse: [[f32; 4]; 4],
    eye: [f32; 4],
    forward: [f32; 4],
    viewport: [f32; 4],
    aperture: [f32; 4],
}
pub(super) struct Prepared {
    pub color: RenderTarget,
    render: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    _uniform: crate::AdmittedBuffer,
    pub bytes: u64,
}

pub(super) fn validate(frame: &ScenePassFrame) -> Result<()> {
    if let Some(dof) = frame.depth_of_field {
        dof.validate().map_err(|cause| {
            crate::RenderError::Invalid(format!("scene phase=depth_of_field: {cause}"))
        })?;
        if dof.aperture_diameter > 0.
            && frame.camera.projection != motion_model::ProjectionKind::Perspective
        {
            return Err(unsupported(
                0,
                "depth of field requires a perspective camera",
            ));
        }
    }
    Ok(())
}

pub(super) fn prepare(
    renderer: &Renderer,
    target: &RenderTarget,
    frame: &ScenePassFrame,
    depth: &Targets,
    pipelines: &mut HashMap<Key, Pipeline>,
) -> Result<Option<Prepared>> {
    let Some(dof) = frame
        .depth_of_field
        .filter(|dof| dof.aperture_diameter > 0.)
    else {
        return Ok(None);
    };
    let viewport = frame
        .viewport
        .unwrap_or([0., 0., target.width as f32, target.height as f32]);
    let inverse = frame.camera.view_projection.inverse();
    let forward = (frame.camera.target - frame.camera.eye)
        .try_normalize()
        .ok_or_else(|| invalid("depth_of_field", 0, "camera eye and target must differ"))?;
    let zoom = frame.camera.view_projection.row(1).truncate().length() * viewport[3] * 0.5;
    if !inverse.is_finite() || !zoom.is_finite() || zoom <= 0. {
        return Err(invalid(
            "depth_of_field",
            0,
            "finite inverse projection and pixel zoom required",
        ));
    }
    let key = Key(target.texture().format(), frame.samples);
    if let std::collections::hash_map::Entry::Vacant(entry) = pipelines.entry(key) {
        entry.insert(Pipeline::new(renderer, key)?);
    }
    let pipeline = &pipelines[&key];
    let color = renderer.render_target_with_format(
        target.width,
        target.height,
        target.texture().format(),
        TextureCategory::LayerCompositingScratch,
    )?;
    let parameters = Uniform {
        inverse: inverse.to_cols_array_2d(),
        eye: frame.camera.eye.extend(0.).to_array(),
        forward: forward.extend(0.).to_array(),
        viewport,
        aperture: [
            dof.focus_distance,
            dof.aperture_diameter,
            dof.max_radius_pixels,
            zoom,
        ],
    };
    let uniform = renderer
        .allocation_context()
        .buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("immutable scene aperture camera"),
            contents: bytemuck::bytes_of(&parameters),
            usage: wgpu::BufferUsages::UNIFORM,
        })?;
    let group = renderer
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("owned native color and physical depth"),
            layout: &pipeline.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&depth.depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
    let bytes = crate::resource_ledger::logical_bytes(color.texture())
        .ok_or_else(|| invalid("depth_of_field", 0, "cannot account aperture color"))?
        + uniform.size();
    Ok(Some(Prepared {
        color,
        render: pipeline.render.clone(),
        group,
        _uniform: uniform,
        bytes,
    }))
}
impl Prepared {
    pub fn uniform_bytes(&self) -> u64 {
        self._uniform.size()
    }
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, target: &RenderTarget) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bounded physical-depth aperture resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.render);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}
impl Pipeline {
    fn new(renderer: &Renderer, Key(format, samples): Key) -> Result<Self> {
        let oom = renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let (declaration, load) = if samples == 4 {
            ("@group(0) @binding(1) var scene_depth: texture_depth_multisampled_2d;",
             "var depth = 1.; for (var i = 0; i < 4; i++) { depth = min(depth, textureLoad(scene_depth, pixel, i)); }")
        } else {
            (
                "@group(0) @binding(1) var scene_depth: texture_depth_2d;",
                "let depth = textureLoad(scene_depth, pixel, 0);",
            )
        };
        let source = include_str!("depth_of_field.wgsl")
            .replace("// DEPTH_DECLARATION", declaration)
            .replace("// DEPTH_LOAD", load);
        let shader = renderer
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("native world-depth aperture"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let layout = renderer
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("unfiltered working color and actual hardware depth"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: samples == 4,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(
                                std::mem::size_of::<Uniform>() as u64
                            ),
                        },
                        count: None,
                    },
                ],
            });
        let pipeline_layout =
            renderer
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("native aperture layout"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                });
        let render = renderer
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("49-tap bounded scene aperture"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
        let cause = pollster::block_on(validation.pop());
        let oom_cause = pollster::block_on(oom.pop());
        if let Some(cause) = cause.or(oom_cause) {
            return Err(cause.into());
        }
        Ok(Self { render, layout })
    }
}
