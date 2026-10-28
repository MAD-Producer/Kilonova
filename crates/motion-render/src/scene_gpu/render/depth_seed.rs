//! Immutable, admitted, explicit camera-Z depth seeding for the shared pass.
use super::{features, invalid, Result, ScenePassFrame};
use crate::{AdmittedBuffer, AdmittedTexture, AdmittedTextureView, RenderTarget, Renderer};
use motion_model::depth::{DepthSource, ProjectedDepth};
use std::collections::HashMap;

/// Full image UV maps to the pass viewport. Arbitrary transformed image planes
/// require a separate reprojection contract; this input does not infer one.
pub struct SceneDepthInput<'a> {
    pub field: &'a ProjectedDepth,
    /// Independently supplied current source/revision/PTS, not a cached identity.
    pub source: &'a DepthSource,
    /// Common scene world units; independent object import scales may differ.
    pub pixels_per_metre: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct Key(wgpu::TextureFormat, u32);
pub(super) struct Pipeline {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}
pub(super) struct Seed {
    pub pipeline: wgpu::RenderPipeline,
    pub group: wgpu::BindGroup,
    pub texture: AdmittedTexture,
    _view: AdmittedTextureView,
    _uniform: AdmittedBuffer,
    pub bytes: u64,
}

pub(super) fn prepare(
    renderer: &Renderer,
    pipelines: &mut HashMap<Key, Pipeline>,
    target: &RenderTarget,
    frame: &ScenePassFrame,
    input: &SceneDepthInput<'_>,
) -> Result<Seed> {
    prepare_inner(renderer, pipelines, target, frame, input).map_err(|cause| {
        super::super::SceneGpuError::DepthInput {
            expected: input.source.clone(),
            actual: input.field.source().source.clone(),
            cause: Box::new(cause),
        }
    })
}

fn prepare_inner(
    renderer: &Renderer,
    pipelines: &mut HashMap<Key, Pipeline>,
    target: &RenderTarget,
    frame: &ScenePassFrame,
    input: &SceneDepthInput<'_>,
) -> Result<Seed> {
    let field = input.field;
    if !input.source.same_sample(&field.source().source) {
        return Err(invalid(
            "depth_source",
            0,
            "stale depth source/revision/PTS",
        ));
    }
    if !field.projection().matches_camera(frame.camera) {
        return Err(invalid(
            "depth_camera",
            0,
            "depth camera differs from shared camera",
        ));
    }
    if !input.pixels_per_metre.is_finite()
        || input.pixels_per_metre != field.projection().pixels_per_meter()
    {
        return Err(invalid(
            "depth_units",
            0,
            "depth and scene world units differ",
        ));
    }
    let [width, height] = field.size();
    let limit = renderer.device.limits().max_texture_dimension_2d;
    if width > limit || height > limit {
        return Err(invalid(
            "depth_size",
            0,
            "depth texture exceeds device limit",
        ));
    }
    let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
    if !features(renderer, wgpu::TextureFormat::Rg32Float)
        .allowed_usages
        .contains(usage)
    {
        return Err(super::super::unsupported(
            0,
            "RG32Float depth input texture",
        ));
    }
    let key = Key(target.texture().format(), frame.samples);
    if let std::collections::hash_map::Entry::Vacant(entry) = pipelines.entry(key) {
        entry.insert(Pipeline::new(renderer, key)?);
    }
    let pipeline = &pipelines[&key];
    let context = renderer.allocation_context();
    // All allocations/views/bindings precede queue upload and render recording.
    // Each encode owns different immutable pixels and viewport constants.
    let texture = context.texture_in(
        &wgpu::TextureDescriptor {
            label: Some("immutable calibrated shared depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Float,
            usage,
            view_formats: &[],
        },
        crate::TextureCategory::LayerCompositingScratch,
    )?;
    let viewport = frame
        .viewport
        .unwrap_or([0., 0., target.width as f32, target.height as f32]);
    let uniform = context.buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("immutable calibrated depth viewport"),
        contents: bytemuck::cast_slice(&viewport),
        usage: wgpu::BufferUsages::UNIFORM,
    })?;
    let view = texture.view()?;
    let group = renderer
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("calibrated depth and exact viewport"),
            layout: &pipeline.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
    let bytes = u64::from(width) * u64::from(height) * 8 + uniform.size();
    Ok(Seed {
        pipeline: pipeline.pipeline.clone(),
        group,
        texture,
        _view: view,
        _uniform: uniform,
        bytes,
    })
}

/// Called only after every source, uniform and replacement attachment succeeds.
/// Rejections before this point must not strand a pending queue-upload lease.
pub(super) fn upload(renderer: &Renderer, seed: &Seed, input: &SceneDepthInput<'_>) -> Result<()> {
    let [width, height] = input.field.size();
    renderer
        .allocation_context()
        .write_texture(
            &seed.texture,
            bytemuck::cast_slice(input.field.samples()),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 8),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        )
        .map_err(|cause| super::super::SceneGpuError::DepthInput {
            expected: input.source.clone(),
            actual: input.field.source().source.clone(),
            cause: Box::new(cause.into()),
        })
}

impl Pipeline {
    fn new(renderer: &Renderer, Key(format, samples): Key) -> Result<Self> {
        let oom = renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = renderer
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("explicit calibrated shared depth"),
                source: wgpu::ShaderSource::Wgsl(include_str!("depth_seed.wgsl").into()),
            });
        let layout = renderer
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("calibrated scalar depth input"),
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
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(16),
                        },
                        count: None,
                    },
                ],
            });
        let pipeline_layout =
            renderer
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("calibrated shared depth seed"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                });
        let pipeline = renderer
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("depth-only seed before shared geometry"),
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
                        write_mask: wgpu::ColorWrites::empty(),
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            });
        let validation_cause = pollster::block_on(validation.pop());
        let oom_cause = pollster::block_on(oom.pop());
        if let Some(cause) = validation_cause.or(oom_cause) {
            return Err(cause.into());
        }
        Ok(Self { layout, pipeline })
    }
}
