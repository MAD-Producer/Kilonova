//! Immutable, budget-admitted floating-point LUT mapping. This is a numerical
//! transform, not an OCIO configuration resolver or an implicit display transfer.
use crate::allocation_buffer::AllocationBuffer;
use crate::resource_ledger::OwnedTexture;
use crate::{AllocationEncoder, ImageCaptureTarget, RenderError, Renderer, TextureResourceLedger};
use motion_effects::color_lut::{ColorLut, LutInterpolation, LutKind};
use motion_effects::image_contract::{ChannelPrecision, ImageContract, ImageMeaning};
use motion_effects::AlphaMode;
use std::{error::Error, fmt, sync::Arc};

/// Native creation causes are retained rather than converted to validation text.
#[derive(Debug)]
pub struct NativeLutError {
    native: wgpu::Error,
    related: Vec<wgpu::Error>,
    poll: Option<wgpu::PollError>,
    admission: Option<Box<RenderError>>,
}
impl NativeLutError {
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
}
impl fmt::Display for NativeLutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(error) = &self.admission {
            write!(f, "{error}; LUT GPU creation also failed: ")?;
        } else {
            write!(f, "color LUT phase=create: ")?;
        }
        write!(f, "{}", self.native)?;
        for error in &self.related {
            write!(f, "; GPU creation also failed: {error}")?;
        }
        if let Some(error) = &self.poll {
            write!(f, "; GPU poll also failed: {error}")?;
        }
        Ok(())
    }
}
impl Error for NativeLutError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.admission {
            Some(error) => Some(&**error),
            None => Some(&self.native),
        }
    }
}

/// Reuse this binding while updating the original source image in place.
/// Producers must provide finite color components and alpha in [0,1], as for
/// other native image producers. Encoding does not synchronously read pixels
/// back to validate that precondition; CPU import validation is a separate path.
/// Source/output space tags are explicit declarations: this LUT alone does not
/// infer primaries, transfer functions, configuration roles or ACES versions.
pub struct LutOutput {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    source: OwnedTexture,
    output: ImageContract,
    key: [u8; 32],
    _uniform: AllocationBuffer,
    _table: AllocationBuffer,
    owner: Arc<()>,
}

fn color_alpha(contract: ImageContract) -> Result<AlphaMode, RenderError> {
    contract.preferred_storage()?;
    let ImageMeaning::Color { alpha, .. } = contract.meaning else {
        return Err(RenderError::Invalid(
            "color LUT requires color; coverage, depth, normals and numeric data must bypass it"
                .into(),
        ));
    };
    if !matches!(
        contract.precision,
        ChannelPrecision::Float16 | ChannelPrecision::Float32
    ) {
        return Err(RenderError::Invalid(
            "color LUT requires floating-point storage".into(),
        ));
    }
    Ok(alpha)
}

impl LutOutput {
    pub fn new(
        renderer: &Renderer,
        source: &ImageCaptureTarget,
        lut: &ColorLut,
        output: ImageContract,
    ) -> Result<Self, RenderError> {
        Self::create(
            renderer,
            source,
            lut,
            output,
            include_str!("color_lut.wgsl"),
        )
    }

    fn create(
        renderer: &Renderer,
        source: &ImageCaptureTarget,
        lut: &ColorLut,
        output: ImageContract,
        shader: &str,
    ) -> Result<Self, RenderError> {
        renderer.check_health()?;
        source.check_owner(renderer)?;
        let source_alpha = color_alpha(source.contract())?;
        let output_alpha = color_alpha(output)?;
        let format = crate::image_formats::native_format(output.preferred_storage()?);
        if output.precision == ChannelPrecision::Float16
            && lut.values().iter().flatten().any(|v| v.abs() > 65_504.)
        {
            return Err(RenderError::Invalid(
                "color LUT output entries exceed finite Float16 storage range".into(),
            ));
        }
        let device = &renderer.device;
        let table_bytes = lut.values().len() as u64 * 16;
        let limits = device.limits();
        if table_bytes > u64::from(limits.max_storage_buffer_binding_size)
            || table_bytes > limits.max_buffer_size
        {
            return Err(RenderError::Invalid(format!(
                "color LUT table bytes={table_bytes} exceed device storage binding capacity={} buffer capacity={}",
                limits.max_storage_buffer_binding_size, limits.max_buffer_size
            )));
        }
        let d = lut.descriptor();
        let table: Vec<[f32; 4]> = lut
            .values()
            .iter()
            .map(|v| [v[0], v[1], v[2], 0.])
            .collect();
        let words = [
            d.size,
            u32::from(d.kind == LutKind::ThreeD),
            match d.interpolation {
                LutInterpolation::Nearest => 0,
                LutInterpolation::Linear => 1,
                LutInterpolation::Tetrahedral => 2,
            },
            u32::from(source_alpha == AlphaMode::Premultiplied),
            d.domain_min[0].to_bits(),
            d.domain_min[1].to_bits(),
            d.domain_min[2].to_bits(),
            0,
            d.domain_max[0].to_bits(),
            d.domain_max[1].to_bits(),
            d.domain_max[2].to_bits(),
            0,
            u32::from(output_alpha == AlphaMode::Premultiplied),
            0,
            0,
            0,
        ];
        let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = (|| {
            let uniform = AllocationBuffer::new_init(
                device,
                renderer.allocation_budget(),
                &wgpu::util::BufferInitDescriptor {
                    label: Some("immutable color LUT domain and interpolation"),
                    contents: bytemuck::cast_slice(&words),
                    usage: wgpu::BufferUsages::UNIFORM,
                },
            )?;
            let table = AllocationBuffer::new_init(
                device,
                renderer.allocation_budget(),
                &wgpu::util::BufferInitDescriptor {
                    label: Some("immutable color LUT samples"),
                    contents: bytemuck::cast_slice(&table),
                    usage: wgpu::BufferUsages::STORAGE,
                },
            )?;
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("floating color LUT bindings"),
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
                            min_binding_size: wgpu::BufferSize::new(64),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(table_bytes),
                        },
                        count: None,
                    },
                ],
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("retained color LUT source and samples"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source.view()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: table.as_entire_binding(),
                    },
                ],
            });
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("floating color LUT mapping"),
                source: wgpu::ShaderSource::Wgsl(shader.into()),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("floating color LUT pipeline"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("floating color LUT mapping"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vertex_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fragment_main"),
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
            Ok::<_, RenderError>((pipeline, group, uniform, table))
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
            return Err(NativeLutError {
                native,
                related: errors.collect(),
                poll: polled.err(),
                admission: result.err().map(Box::new),
            }
            .into());
        }
        let (pipeline, group, uniform, table) = result.map_err(|primary| {
            if let Err(secondary) = &polled {
                let detail = format!("{primary}; color LUT creation poll also failed: {secondary}");
                primary.with_diagnostic(detail)
            } else {
                primary
            }
        })?;
        polled?;
        renderer.check_health()?;
        Ok(Self {
            pipeline,
            group,
            source: source.texture().clone(),
            output,
            key: lut.key(),
            _uniform: uniform,
            _table: table,
            owner: renderer.allocation_owner.clone(),
        })
    }

    pub fn key(&self) -> [u8; 32] {
        self.key
    }
    pub fn output_contract(&self) -> ImageContract {
        self.output
    }

    /// Equal extents only. No filter, implicit transfer, tone map or quantization
    /// to 8-bit occurs. AllocationEncoder retains all admitted resources through
    /// command submission completion even if this binding is subsequently dropped.
    pub fn encode(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        destination: &ImageCaptureTarget,
    ) -> Result<(), RenderError> {
        renderer.check_health()?;
        if !Arc::ptr_eq(&self.owner, &renderer.allocation_owner) {
            return Err(RenderError::Invalid(
                "color LUT belongs to another renderer".into(),
            ));
        }
        encoder.check_owner(&self.owner)?;
        destination.check_owner(renderer)?;
        if destination.contract() != self.output {
            return Err(RenderError::Invalid(
                "color LUT destination contract differs from declaration".into(),
            ));
        }
        if destination.texture().size() != self.source.size() {
            return Err(RenderError::Invalid(
                "color LUT source and destination extents differ".into(),
            ));
        }
        if &**destination.texture() == &*self.source {
            return Err(RenderError::Invalid(
                "color LUT source and destination must be distinct".into(),
            ));
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("floating color LUT mapping"),
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
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
        Ok(())
    }

    pub fn texture_resource_ledger(&self) -> TextureResourceLedger {
        let mut ledger = TextureResourceLedger::default();
        ledger.retained(
            &self.source,
            crate::TextureCategory::CaptureTargets,
            "color-lut/source".into(),
        );
        ledger
    }
}

#[cfg(test)]
mod tests;
