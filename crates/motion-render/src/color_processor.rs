//! Direct immutable OCIO image binding. No per-frame allocations, image
//! buffers, sampling transfer, filtering or CPU readback in encode.
use crate::{
    allocation_buffer::AllocationBuffer, resource_ledger::OwnedTexture, AllocationEncoder,
    ImageCaptureTarget, RenderError, Renderer, TextureResourceLedger,
};
use motion_effects::{
    color_processor::{ColorProcessorPhase, CompiledColorImageProcessor},
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode,
};
use std::{error::Error, fmt, sync::Arc};

mod managed;
pub use managed::{
    CapturedColorImage, ColorImageContract, ColorImageRenderSourceError, ColorImageTarget,
    ColorImageUploadError, ColorProcessorOutput, ColorRangeMismatch, ColorSpaceMismatch,
};

/// Retains primary admission and all native causes without displaying complete
/// shader source in the outer diagnostic. Callers traverse source/native APIs.
#[derive(Debug)]
pub struct NativeColorProcessorError {
    native: wgpu::Error,
    related: Vec<wgpu::Error>,
    poll: Option<wgpu::PollError>,
    admission: Option<Box<RenderError>>,
    key: [u8; 32],
    phase: ColorProcessorPhase,
}
impl NativeColorProcessorError {
    pub fn native(&self) -> &wgpu::Error {
        &self.native
    }
    pub fn related(&self) -> &[wgpu::Error] {
        &self.related
    }
    pub fn poll_error(&self) -> Option<&wgpu::PollError> {
        self.poll.as_ref()
    }
    pub fn admission_error(&self) -> Option<&RenderError> {
        self.admission.as_deref()
    }
    pub fn key(&self) -> [u8; 32] {
        self.key
    }
    pub fn phase(&self) -> ColorProcessorPhase {
        self.phase
    }
}
impl fmt::Display for NativeColorProcessorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(error) = &self.admission {
            write!(f, "{error}; ")?;
        }
        let kind = match &self.native {
            wgpu::Error::Validation { .. } => "validation",
            wgpu::Error::OutOfMemory { .. } => "out-of-memory",
            wgpu::Error::Internal { .. } => "internal",
        };
        write!(f,"color processor phase=create transform={:?} key={:02x?} native={kind} related={} poll_failed={}",self.phase,&self.key[..8],self.related.len(),self.poll.is_some())
    }
}
impl Error for NativeColorProcessorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.admission {
            Some(error) => Some(&**error),
            None => Some(&self.native),
        }
    }
}

/// Private raw transport. The public wrapper checks complete endpoint identity;
/// ImageContract's legacy Linear tag only selects non-transferring float storage.
struct RawColorProcessorOutput {
    pipeline: wgpu::RenderPipeline,
    tables_group: wgpu::BindGroup,
    source_group: wgpu::BindGroup,
    source: OwnedTexture,
    output: ImageContract,
    key: [u8; 32],
    _tables: Vec<AllocationBuffer>,
    owner: Arc<()>,
}
fn alpha(contract: ImageContract) -> Result<AlphaMode, RenderError> {
    contract.preferred_storage()?;
    let ImageMeaning::Color { alpha, .. } = contract.meaning else {
        return Err(RenderError::Invalid("OCIO image transform requires color; coverage, depth, normals and numeric data must bypass it".into()));
    };
    // Qualification of this direct path is Float32. A half-float destination
    // needs an explicit overflow/quantization contract, never implicit clipping.
    if contract.precision != ChannelPrecision::Float32 {
        return Err(RenderError::Invalid(
            "OCIO image transform requires explicit Float32 source and destination storage".into(),
        ));
    }
    Ok(alpha)
}
fn table_entry(binding: u32, bytes: u64) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(bytes),
        },
        count: None,
    }
}
impl RawColorProcessorOutput {
    pub fn new(
        renderer: &Renderer,
        source: &ImageCaptureTarget,
        processor: &CompiledColorImageProcessor,
        output: ImageContract,
    ) -> Result<Self, RenderError> {
        Self::create(renderer, source, processor, output, processor.wgsl())
    }
    fn create(
        renderer: &Renderer,
        source: &ImageCaptureTarget,
        processor: &CompiledColorImageProcessor,
        output: ImageContract,
        shader: &str,
    ) -> Result<Self, RenderError> {
        renderer.check_health()?;
        source.check_owner(renderer)?;
        if alpha(source.contract())? != processor.input_alpha()
            || alpha(output)? != processor.output_alpha()
        {
            return Err(RenderError::Invalid(
                "OCIO image alpha contract differs from prepared transform".into(),
            ));
        }
        let device = &renderer.device;
        let limits = device.limits();
        if processor.tables().len() > limits.max_storage_buffers_per_shader_stage as usize {
            return Err(RenderError::Invalid(format!(
                "OCIO image table count={} exceeds device fragment buffer limit={}",
                processor.tables().len(),
                limits.max_storage_buffers_per_shader_stage
            )));
        }
        for table in processor.tables() {
            let bytes = table.len() as u64 * 16;
            if bytes > u64::from(limits.max_storage_buffer_binding_size)
                || bytes > limits.max_buffer_size
            {
                return Err(RenderError::Invalid(format!(
                    "OCIO image table bytes={bytes} exceed device binding={} buffer={}",
                    limits.max_storage_buffer_binding_size, limits.max_buffer_size
                )));
            }
        }
        let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = (|| {
            let tables: Vec<_> = processor
                .tables()
                .iter()
                .map(|values| {
                    AllocationBuffer::new_init(
                        device,
                        renderer.allocation_budget(),
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("immutable OCIO numeric image table"),
                            contents: bytemuck::cast_slice(values),
                            usage: wgpu::BufferUsages::STORAGE,
                        },
                    )
                })
                .collect::<Result<_, _>>()?;
            let entries: Vec<_> = processor
                .tables()
                .iter()
                .enumerate()
                .map(|(i, t)| table_entry(i as u32, t.len() as u64 * 16))
                .collect();
            let tables_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("OCIO image table bindings"),
                entries: &entries,
            });
            let source_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("OCIO image source"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });
            let table_entries: Vec<_> = tables
                .iter()
                .enumerate()
                .map(|(i, t)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: t.as_entire_binding(),
                })
                .collect();
            let tables_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("retained OCIO image tables"),
                layout: &tables_layout,
                entries: &table_entries,
            });
            let source_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("retained OCIO source image"),
                layout: &source_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source.view()),
                }],
            });
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("direct OCIO image shader"),
                source: wgpu::ShaderSource::Wgsl(shader.into()),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("direct OCIO image layout"),
                bind_group_layouts: &[Some(&tables_layout), Some(&source_layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("direct OCIO image transform"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("motion_image_vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: crate::image_formats::native_format(output.preferred_storage()?),
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
            Ok::<_, RenderError>((pipeline, tables_group, source_group, tables))
        })();
        let validation = validation.pop();
        let internal = internal.pop();
        let memory = memory.pop();
        let polled = device.poll(wgpu::PollType::Poll);
        let mut errors = pollster::block_on(validation)
            .into_iter()
            .chain(pollster::block_on(internal))
            .chain(pollster::block_on(memory));
        if let Some(native) = errors.next() {
            return Err(NativeColorProcessorError {
                native,
                related: errors.collect(),
                poll: polled.err(),
                admission: result.err().map(Box::new),
                key: processor.key(),
                phase: processor.phase(),
            }
            .into());
        }
        let (pipeline, tables_group, source_group, tables) = result.map_err(|primary| {
            if let Err(secondary) = &polled {
                let text = format!("{primary}; OCIO image creation poll also failed: {secondary}");
                primary.with_diagnostic(text)
            } else {
                primary
            }
        })?;
        polled?;
        renderer.check_health()?;
        Ok(Self {
            pipeline,
            tables_group,
            source_group,
            source: source.texture().clone(),
            output,
            key: processor.key(),
            _tables: tables,
            owner: renderer.allocation_owner.clone(),
        })
    }
    pub fn key(&self) -> [u8; 32] {
        self.key
    }
    /// Exact image extents, one fragment per pixel. No resampling or implicit
    /// display transfer; AllocationEncoder retains admission through completion.
    fn validate_destination(
        &self,
        renderer: &Renderer,
        encoder: &AllocationEncoder,
        destination: &ImageCaptureTarget,
    ) -> Result<(), RenderError> {
        renderer.check_health()?;
        if !Arc::ptr_eq(&self.owner, &renderer.allocation_owner) {
            return Err(RenderError::Invalid(
                "OCIO image binding belongs to another renderer".into(),
            ));
        }
        encoder.check_owner(&self.owner)?;
        destination.check_owner(renderer)?;
        if destination.contract() != self.output {
            return Err(RenderError::Invalid(
                "OCIO destination contract differs from declaration".into(),
            ));
        }
        if destination.texture().size() != self.source.size() {
            return Err(RenderError::Invalid(
                "OCIO source and destination extents differ".into(),
            ));
        }
        if &**destination.texture() == &*self.source {
            return Err(RenderError::Invalid(
                "OCIO source and destination must be distinct".into(),
            ));
        }
        Ok(())
    }
    fn encode_validated(&self, encoder: &mut AllocationEncoder, destination: &ImageCaptureTarget) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("direct OCIO image transform"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: destination.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.tables_group, &[]);
        pass.set_bind_group(1, &self.source_group, &[]);
        pass.draw(0..3, 0..1);
    }
    pub fn texture_resource_ledger(&self) -> TextureResourceLedger {
        let mut ledger = TextureResourceLedger::default();
        ledger.retained(
            &self.source,
            crate::TextureCategory::CaptureTargets,
            "color-processor/source".into(),
        );
        ledger
    }
}
#[cfg(test)]
mod tests;
