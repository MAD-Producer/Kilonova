//! User-requested working-format changes on the same device. Prepare first;
//! failed admission or unsupported modes preserve every live renderer field.
use super::*;
use motion_effects::image_contract::{ChannelPrecision, ImageUsage};

pub struct WorkingPrecisionChange {
    owner: Arc<()>,
    previous: ChannelPrecision,
    precision: ChannelPrecision,
    format: wgpu::TextureFormat,
    pipelines: (
        wgpu::RenderPipeline,
        wgpu::RenderPipeline,
        wgpu::RenderPipeline,
    ),
    layer_gpu: crate::layer_gpu::LayerGpu,
    mask_gpu: crate::mask_gpu::MaskGpu,
}
impl Renderer {
    pub fn working_precision(&self) -> ChannelPrecision {
        self.effect_gpu.builder.precision()
    }
    /// No GPU submission or queue write occurs during preparation. All admitted
    /// candidate resources coexist with the old working set until commit/drop.
    pub fn prepare_working_precision(
        &self,
        precision: ChannelPrecision,
    ) -> Result<Option<WorkingPrecisionChange>> {
        self.check_health()?;
        self.allocation_budget.check()?;
        if precision == self.working_precision() {
            return Ok(None);
        }
        let missing = crate::image_formats::working_features(precision) - self.device.features();
        if !missing.is_empty() {
            return Err(RenderError::Invalid(format!(
                "working precision {precision:?} phase=enabled-device: missing {missing:?}; no precision fallback was applied"
            )));
        }
        for storage in [
            crate::image_formats::color_storage(precision),
            crate::image_formats::raster_storage(precision),
            motion_effects::image_contract::coverage_storage(precision),
        ] {
            self.image_capabilities
                .require(storage, ImageUsage::EFFECT_TARGET)?;
        }
        self.image_capabilities
            .require_msaa4(crate::image_formats::raster_storage(precision))?;
        if self.video_gpu.as_ref().is_some_and(|gpu| gpu.has_pending()) {
            return Err(RenderError::Invalid(
                "finish pending video conversions before changing working precision".into(),
            ));
        }
        let format =
            crate::image_formats::native_format(crate::image_formats::color_storage(precision));
        let layer_gpu = crate::layer_gpu::LayerGpu::new(
            &self.device,
            &self.uniform_layout,
            &self.image_layout,
            format,
            &self.allocation_budget,
        )?;
        let mask_gpu = crate::mask_gpu::MaskGpu::with_precision(
            &self.device,
            &self.image_layout,
            self.allocation_budget.clone(),
            self.allocation_uploads.clone(),
            precision,
        )?;
        let pipelines = pipelines(
            &self.device,
            &self.uniform_layout,
            &self.image_layout,
            format,
        );
        self.check_health()?;
        Ok(Some(WorkingPrecisionChange {
            owner: self.allocation_owner.clone(),
            previous: self.working_precision(),
            precision,
            format,
            pipelines,
            layer_gpu,
            mask_gpu,
        }))
    }
    /// The owner and expected current mode prevent foreign or stale prepared
    /// changes. External preview/exposure targets must be invalidated by the host.
    pub fn commit_working_precision(&mut self, change: WorkingPrecisionChange) -> Result<()> {
        self.check_health()?;
        if !Arc::ptr_eq(&self.allocation_owner, &change.owner) {
            return Err(RenderError::Invalid(
                "working precision change belongs to another renderer".into(),
            ));
        }
        if self.working_precision() != change.previous {
            return Err(RenderError::Invalid(
                "working precision change was prepared for another mode".into(),
            ));
        }
        self.allocation_budget.check()?;
        // These image-cache entries contain working-format results. Their size
        // and object ID alone are insufficient after a precision change. Keep
        // static inputs and decoded video, whose storage is independent.
        let obsolete: Vec<_> = self
            .images
            .keys()
            .copied()
            .filter(|key| match key {
                TextureKey::Vector(_) | TextureKey::Mask(_) | TextureKey::EffectInput(_) => true,
                TextureKey::Video(id) => self.composition_images.contains(id),
                _ => false,
            })
            .collect();
        for key in obsolete {
            if let Some(image) = self.images.remove(&key) {
                self.texture_bytes = self.texture_bytes.saturating_sub(image.bytes);
            }
        }
        self.composition_images.clear();
        let (pipeline, additive_pipeline, masked_pipeline) = change.pipelines;
        self.pipeline = pipeline;
        self.additive_pipeline = additive_pipeline;
        self.masked_pipeline = masked_pipeline;
        self.layer_gpu = change.layer_gpu;
        self.mask_gpu = change.mask_gpu;
        self.target_format = change.format;
        self.effect_gpu.builder.set_precision(change.precision);
        self.effect_gpu.state.release_scratch();
        self.compositing_gpu = None;
        self.exposure_gpu = None;
        self.video_blend = None;
        Ok(())
    }
}
pub(super) fn pipelines(
    device: &wgpu::Device,
    uniform_layout: &wgpu::BindGroupLayout,
    image_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> (
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Motion Studio own planar compositor"),
        source: wgpu::ShaderSource::Wgsl(
            crate::plane_coverage::shader(crate::plane_sampling::BASIC, true).into(),
        ),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Motion Studio planar pipeline"),
        bind_group_layouts: &[Some(uniform_layout), Some(image_layout)],
        immediate_size: 0,
    });
    let create_pipeline = |blend| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Motion Studio planar compositor"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GeometryVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x2],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let pipeline = create_pipeline(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
    let additive_pipeline = create_pipeline(wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING.alpha,
    });
    let mask_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("masked source composite"),
        source: wgpu::ShaderSource::Wgsl(
            crate::plane_coverage::shader(crate::plane_sampling::MASK, true).into(),
        ),
    });
    let masked_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("masked layer without effects"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(uniform_layout), Some(image_layout), Some(image_layout)],
                immediate_size: 0,
            }),
        ),
        vertex: wgpu::VertexState {
            module: &mask_shader,
            entry_point: Some("vertex_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GeometryVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x2],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &mask_shader,
            entry_point: Some("fragment_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
    });
    (pipeline, additive_pipeline, masked_pipeline)
}
