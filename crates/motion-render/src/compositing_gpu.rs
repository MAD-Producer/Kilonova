//! GPU resources shared by track-matte extraction and layer transfer.
use crate::{effect_gpu::FxTexture, RenderError};
use std::collections::HashMap;

pub(crate) struct BlendInput {
    pub group: wgpu::BindGroup,
    // Raw views in the cached group may outlive a source or accumulator cache.
    _allocations: [Option<crate::allocation_budget::AllocationLease>; 3],
}

pub(crate) struct CompositingGpu {
    pub normal: wgpu::RenderPipeline,
    pub additive: wgpu::RenderPipeline,
    pub alpha: wgpu::RenderPipeline,
    pub luma: wgpu::RenderPipeline,
    // Retire cached bindings before their admitted backing resources.
    pub extract_group: wgpu::BindGroup,
    pub blend_group: wgpu::BindGroup,
    pub blend_inputs: Vec<BlendInput>,
    pub mattes: HashMap<(u64, bool), FxTexture>,
    pub zero: FxTexture,
    pub source: Option<FxTexture>,
    pub extract_buffer: crate::allocation_buffer::AllocationBuffer,
    pub extract_stride: usize,
    pub blend: wgpu::RenderPipeline,
    pub blend_buffer: crate::allocation_buffer::AllocationBuffer,
    blend_layout: wgpu::BindGroupLayout,
    blend_stride: usize,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    coverage_format: wgpu::TextureFormat,
    coverage_bytes: u64,
    color_bytes: u64,
    extended: bool,
    allocation_uploads: std::sync::Arc<crate::allocation_uploads::AllocationUploads>,
    allocation_budget: crate::AllocationBudget,
}

impl CompositingGpu {
    pub(crate) fn collect_textures(&self, ledger: &mut crate::TextureResourceLedger) {
        use crate::TextureCategory::MatteBlendScratch;
        ledger.retained(&self.zero.texture, MatteBlendScratch, "compositing/zero".into());
        for ((id,luma),t) in &self.mattes {
            ledger.retained(&t.texture, MatteBlendScratch, format!("compositing/matte/{id}/{luma}"));
        }
        if let Some(t) = &self.source {
            ledger.retained(&t.texture, MatteBlendScratch, "compositing/source".into());
        }
    }
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform: &wgpu::BindGroupLayout,
        image: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        format: wgpu::TextureFormat,
        budget: &crate::AllocationBudget,
        allocation_uploads: std::sync::Arc<crate::allocation_uploads::AllocationUploads>,
    ) -> Result<Self, RenderError> {
        let precision = crate::image_formats::color_precision(format)
            .ok_or_else(|| RenderError::Invalid(format!("unsupported compositing format {format:?}")))?;
        let storage = motion_effects::image_contract::coverage_storage(precision);
        let coverage_format = crate::image_formats::native_format(storage);
        let compiled = crate::compositing_plan::shader()?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("layer and matte plane"),
            source: wgpu::ShaderSource::Wgsl(crate::plane_coverage::shader(crate::plane_sampling::MATTE, true).into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("layer matte inputs"),
            bind_group_layouts: &[Some(uniform), Some(image), Some(image), Some(image)],
            immediate_size: 0,
        });
        let make = |entry: &str, target, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 20,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x2],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let extract_stride = (std::mem::size_of::<crate::renderer::DrawUniform>() as usize)
            .div_ceil(device.limits().min_uniform_buffer_offset_alignment as usize)
            * device.limits().min_uniform_buffer_offset_alignment as usize;
        let extract_buffer = crate::allocation_buffer::AllocationBuffer::new(device, budget, &wgpu::BufferDescriptor {
            label: Some("matte source uniforms"),
            size: (extract_stride * motion_model::MAX_LAYERS) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })?;
        let extract_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: uniform,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &extract_buffer,
                    offset: 0,
                    size: std::num::NonZeroU64::new(
                        std::mem::size_of::<crate::renderer::DrawUniform>() as u64,
                    ),
                }),
            }],
        });
        let blend_uniform = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("layer transfer uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: std::num::NonZeroU64::new(
                        motion_effects::shader::UNIFORM_BYTES as u64,
                    ),
                },
                count: None,
            }],
        });
        let mut entries = Vec::new();
        for i in 0..3 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: i * 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: i * 2 + 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        let blend_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("layer transfer inputs"),
            entries: &entries,
        });
        let blend_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("layer transfer"),
            source: wgpu::ShaderSource::Wgsl(compiled.wgsl.clone().into()),
        });
        let blend = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("layer transfer"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&blend_uniform), Some(&blend_layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &blend_shader,
                entry_point: Some("sdk_vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blend_shader,
                entry_point: Some("sdk_fragment"),
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
        let blend_stride = motion_effects::shader::UNIFORM_BYTES
            .div_ceil(device.limits().min_uniform_buffer_offset_alignment as usize)
            * device.limits().min_uniform_buffer_offset_alignment as usize;
        let blend_buffer = crate::allocation_buffer::AllocationBuffer::new(device, budget, &wgpu::BufferDescriptor {
            label: Some("layer transfer values"),
            size: (blend_stride * motion_model::MAX_LAYERS) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })?;
        let blend_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &blend_uniform,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &blend_buffer,
                    offset: 0,
                    size: std::num::NonZeroU64::new(motion_effects::shader::UNIFORM_BYTES as u64),
                }),
            }],
        });
        let zero = crate::effect_gpu::admitted_texture(device, image, sampler,
            1, 1, coverage_format, budget, crate::TextureCategory::MatteBlendScratch).map_err(|error| RenderError::Context {
                message: format!("compositing zero coverage texture size=1x1 phase=allocate: {error}"),
                cause: Box::new(error),
            })?;
        // All fallible reservations precede the first queue write. A refused
        // constructor has no pending upload and releases partial capacity now.
        allocation_uploads.track(zero.texture.allocation())?;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &zero.texture, mip_level: 0,
                origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
            },
            &[0;4][..storage.bytes_per_pixel() as usize],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(storage.bytes_per_pixel() as u32), rows_per_image: Some(1) },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        Ok(Self {
            normal: make(
                "fragment_main",
                format,
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            ),
            additive: make(
                "fragment_main",
                format,
                Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING.alpha,
                }),
            ),
            alpha: make("alpha_matte", coverage_format, None),
            luma: make("luma_matte", coverage_format, None),
            mattes: HashMap::new(),
            zero,
            source: None,
            size: [0; 2],
            format,
            extract_buffer,
            extract_group,
            extract_stride,
            blend,
            blend_buffer,
            blend_group,
            blend_inputs: Vec::new(),
            blend_layout,
            blend_stride,
            allocation_uploads,
            allocation_budget: budget.clone(),
            coverage_format, coverage_bytes: storage.bytes_per_pixel(),
            color_bytes: crate::image_formats::color_storage(precision).bytes_per_pixel(),
            extended: precision.is_float(),
        })
    }
    pub fn bytes(&self) -> u64 {
        self.mattes.values().chain(std::iter::once(&self.zero)).chain(self.source.iter())
            .map(|t| crate::resource_ledger::logical_bytes(&t.texture).unwrap_or(u64::MAX)).fold(0, u64::saturating_add)
    }
    pub fn release(&mut self) {
        self.blend_inputs.clear();
        self.mattes.clear();
        self.source = None;
        self.size = [0; 2];
    }
    pub fn prepare_targets(
        &mut self,
        device: &wgpu::Device,
        image: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        size: [u32; 2],
        needed: &[(u64, bool)],
        other_bytes: u64,
        source_budget: u64,
        blend: bool,
    ) -> Result<bool, RenderError> {
        let pixels = u64::from(size[0]) * u64::from(size[1]);
        let unique: std::collections::HashSet<_> = needed.iter().copied().collect();
        if other_bytes + self.coverage_bytes + pixels * unique.len() as u64 * self.coverage_bytes > source_budget {
            return Err(RenderError::budget(crate::ResourceBudgetScope::SourceTextures,
                other_bytes + self.coverage_bytes + pixels * unique.len() as u64 * self.coverage_bytes, source_budget,
                "track matte textures exceed configured source texture budget"));
        }
        if blend && pixels * self.color_bytes * 3 > source_budget {
            return Err(RenderError::budget(crate::ResourceBudgetScope::LayerCompositing,
                pixels * self.color_bytes * 3, source_budget,
                "layer transfer accumulators exceed configured source texture budget"));
        }
        let resized = self.size != size;
        // Prepare the complete replacement set before changing any live
        // dimensions, sources, matte identities or cached input bindings.
        let mut additions = HashMap::new();
        for key in needed {
            if (!resized && self.mattes.contains_key(key)) || additions.contains_key(key) {
                continue;
            }
            let texture = crate::effect_gpu::admitted_texture(
                device, image, sampler, size[0], size[1], self.coverage_format,
                &self.allocation_budget,
                crate::TextureCategory::MatteBlendScratch,
            ).map_err(|error| RenderError::Context {
                message: format!("compositing matte layer={} luma={} size={}x{} phase=allocate: {error}",
                    key.0, key.1, size[0], size[1]),
                cause: Box::new(error),
            })?;
            additions.insert(*key, texture);
        }
        let source_changed = blend && (resized || self.source.is_none());
        let next_source = if source_changed {
            Some(crate::effect_gpu::admitted_texture(
                device,
                image,
                sampler,
                size[0],
                size[1],
                self.format,
                &self.allocation_budget,
                crate::TextureCategory::MatteBlendScratch,
            ).map_err(|error| RenderError::Context {
                message: format!("compositing blend source size={}x{} phase=allocate: {error}", size[0], size[1]),
                cause: Box::new(error),
            })?)
        } else { None };
        if resized {
            self.release();
        } else if !blend || source_changed {
            self.blend_inputs.clear();
        }
        self.size = size;
        self.mattes.retain(|key, _| needed.contains(key));
        self.mattes.extend(additions);
        if source_changed {
            self.source = next_source;
        } else if !blend {
            self.source = None;
        }
        Ok(source_changed)
    }
    pub fn prepare_blend_inputs(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sampler: &wgpu::Sampler,
        accumulators: &[FxTexture],
        layers: &[crate::DrawLayer],
        changed: bool,
    ) -> Result<(), RenderError> {
        if self.source.is_none() {
            return Ok(());
        }
        if changed || self.blend_inputs.is_empty() {
            self.blend_inputs = (0..2)
                .map(|i| {
                    let views = [
                        &accumulators[i].view,
                        &self.source.as_ref().unwrap().view,
                        &self.source.as_ref().unwrap().view,
                    ];
                    let entries: Vec<_> = views
                        .iter()
                        .enumerate()
                        .flat_map(|(j, view)| {
                            [
                                wgpu::BindGroupEntry {
                                    binding: j as u32 * 2,
                                    resource: wgpu::BindingResource::TextureView(view),
                                },
                                wgpu::BindGroupEntry {
                                    binding: j as u32 * 2 + 1,
                                    resource: wgpu::BindingResource::Sampler(sampler),
                                },
                            ]
                        })
                        .collect();
                    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("layer transfer backdrop"),
                        layout: &self.blend_layout,
                        entries: &entries,
                    });
                    BlendInput {
                        group,
                        _allocations: [accumulators[i].texture.allocation(),
                            self.source.as_ref().and_then(|t| t.texture.allocation()),
                            self.source.as_ref().and_then(|t| t.texture.allocation())],
                    }
                })
                .collect();
        }
        if !layers.is_empty() {
            self.allocation_uploads.track(Some(self.blend_buffer.allocation()))?;
        }
        for (i, l) in layers.iter().enumerate() {
            let [w, h] = self.size;
            let region = [0., 0., w as f32, h as f32];
            let mut u = crate::effect_plan::EffectUniform {
                size: [w as f32, h as f32, w as f32, h as f32],
                region,
                input_region: region,
                source_region: region,
                clock: [0.; 4],
                mode: [0.; 4],
                output_mode: [0., 0., 1., 1.],
                params: [[0.; 4]; 32],
            };
            u.params[0] = [
                l.blend.mode.code() as f32,
                f32::from(l.blend.space == motion_model::compositing::BlendSpace::Srgb),
                f32::from(self.extended),
                0.,
            ];
            queue.write_buffer(
                &self.blend_buffer,
                (i * self.blend_stride) as u64,
                bytemuck::bytes_of(&u),
            );
        }
        Ok(())
    }
    pub fn blend_offset(&self, layer: usize) -> u32 {
        (layer * self.blend_stride) as u32
    }
}
