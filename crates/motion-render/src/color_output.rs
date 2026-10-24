//! Explicit SDR output boundary. Raw image capture never applies this transform.
//! Rendering stays in its working precision; preview encoding requires no CPU
//! readback, per-frame allocations or filterable Float32 texture support.
use crate::resource_ledger::OwnedTexture;
use crate::{AllocationEncoder, allocation_buffer::AllocationBuffer};
use crate::{
    ImageCaptureTarget, RenderError, RenderStats, RenderTarget, Renderer, Scene,
    TextureResourceLedger,
};
use motion_effects::image_contract::{ChannelPrecision, ImageContract, ImageMeaning};
use motion_effects::{AlphaMode, WorkingSpace};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToneMap {
    /// Clip negative channels to zero and highlights to one.
    Clip,
    /// Map nonnegative linear channels with x/(1+x); negative channels clip.
    Reinhard,
}

/// Exposure is in stops, applied to straight linear RGB before tone mapping.
/// The explicit range bounds the conversion parameter, not working image data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputTransform {
    pub tone_map: ToneMap,
    pub exposure_stops: f32,
}
impl OutputTransform {
    pub const SDR_CLIP: Self = Self {
        tone_map: ToneMap::Clip,
        exposure_stops: 0.,
    };
    fn validate(self) -> Result<(), RenderError> {
        if !self.exposure_stops.is_finite() || !(-24. ..=24.).contains(&self.exposure_stops) {
            return Err(RenderError::Invalid(format!(
                "color output exposure {} must be finite and between -24 and 24 stops",
                self.exposure_stops
            )));
        }
        Ok(())
    }
}

/// Output bytes are sRGB encoded. sRGB attachments perform encoding in hardware;
/// plain UNORM attachments receive an explicit shader encoding instead. Alpha
/// is clamped to [0,1]; premultiplication is defined in linear color space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorOutputDescriptor {
    pub format: wgpu::TextureFormat,
    pub alpha: AlphaMode,
    pub transform: OutputTransform,
}

fn validate_output_format(format: wgpu::TextureFormat) -> Result<(), RenderError> {
    if !matches!(
        format,
        wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        return Err(RenderError::Invalid(
            "SDR color output requires an 8-bit RGBA or BGRA attachment".into(),
        ));
    }
    Ok(())
}

/// Immutable transform and retained input binding. Update the source texture's
/// contents in place each frame. Recreate this object after replacing the source
/// allocation or changing the descriptor. The input must belong to this device.
pub struct ColorOutput {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    source: OwnedTexture,
    descriptor: ColorOutputDescriptor,
    _uniform: AllocationBuffer,
    owner: Arc<()>,
}

impl ColorOutput {
    /// Coverage and numeric image data require a separate visualization policy.
    pub fn for_image(
        renderer: &Renderer,
        source: &ImageCaptureTarget,
        descriptor: ColorOutputDescriptor,
    ) -> Result<Self, RenderError> {
        renderer.check_health()?;
        source.check_owner(renderer)?;
        Self::new(
            renderer,
            source.texture(),
            source.contract(),
            descriptor,
            false,
        )
    }

    /// Native compositor targets contain linear premultiplied color, including
    /// when their storage format performs automatic sRGB sampling transfers.
    pub fn for_render_target(
        renderer: &Renderer,
        source: &RenderTarget,
        descriptor: ColorOutputDescriptor,
    ) -> Result<Self, RenderError> {
        Self::for_render_target_mode(renderer, source, descriptor, false)
    }
    fn for_render_target_mode(
        renderer: &Renderer,
        source: &RenderTarget,
        descriptor: ColorOutputDescriptor,
        opaque_black: bool,
    ) -> Result<Self, RenderError> {
        renderer.check_health()?;
        source.check_owner(renderer)?;
        let contract = ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: AlphaMode::Premultiplied,
            },
            crate::image_formats::color_precision(source.texture().format()).ok_or_else(|| {
                RenderError::Invalid("output source has unsupported color storage".into())
            })?,
        );
        Self::new(
            renderer,
            source.texture(),
            contract,
            descriptor,
            opaque_black,
        )
    }

    fn new(
        renderer: &Renderer,
        source: &OwnedTexture,
        contract: ImageContract,
        descriptor: ColorOutputDescriptor,
        opaque_black: bool,
    ) -> Result<Self, RenderError> {
        renderer.check_health()?;
        descriptor.transform.validate()?;
        contract
            .preferred_storage()
            .map_err(|cause| RenderError::Invalid(format!("color output input: {cause}")))?;
        let ImageMeaning::Color { space, alpha } = contract.meaning else {
            return Err(RenderError::Invalid("color output requires color input; numeric data and coverage are not display color".into()));
        };
        validate_output_format(descriptor.format)?;
        let device = &renderer.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SDR color output input"),
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
                        min_binding_size: wgpu::BufferSize::new(32),
                    },
                    count: None,
                },
            ],
        });
        let words = [
            u32::from(space == WorkingSpace::Srgb),
            u32::from(alpha == AlphaMode::Premultiplied),
            u32::from(descriptor.alpha == AlphaMode::Premultiplied),
            u32::from(descriptor.transform.tone_map == ToneMap::Reinhard),
            descriptor.transform.exposure_stops.exp2().to_bits(),
            u32::from(!descriptor.format.is_srgb()),
            u32::from(opaque_black),
            0,
        ];
        let uniform = AllocationBuffer::new_init(
            device,
            renderer.allocation_budget(),
            &wgpu::util::BufferInitDescriptor {
                label: Some("immutable SDR output transform"),
                contents: bytemuck::cast_slice(&words),
                usage: wgpu::BufferUsages::UNIFORM,
            },
        )?;
        let view = source.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDR output source"),
            layout: &layout,
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("explicit SDR conversion"),
            source: wgpu::ShaderSource::Wgsl(include_str!("color_output.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("explicit SDR conversion"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: descriptor.format,
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
        renderer.check_health()?;
        Ok(Self {
            pipeline,
            group,
            source: source.clone(),
            descriptor,
            _uniform: uniform,
            owner: renderer.allocation_owner.clone(),
        })
    }

    pub fn descriptor(&self) -> ColorOutputDescriptor {
        self.descriptor
    }

    fn check_owner(&self, renderer: &Renderer) -> Result<(), RenderError> {
        renderer.check_health()?;
        if Arc::ptr_eq(&self.owner, &renderer.allocation_owner) {
            Ok(())
        } else {
            Err(RenderError::Invalid(
                "output conversion belongs to another renderer".into(),
            ))
        }
    }
    /// Encode into an owned destination. AllocationEncoder retains source,
    /// uniform and destination reservations until submission completion.
    pub fn encode(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        destination: &RenderTarget,
    ) -> Result<(), RenderError> {
        self.check_owner(renderer)?;
        destination.check_owner(renderer)?;
        self.encode_checked(encoder, &destination.view)
    }
    pub fn encode_image(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        destination: &ImageCaptureTarget,
    ) -> Result<(), RenderError> {
        self.check_owner(renderer)?;
        destination.check_owner(renderer)?;
        self.encode_checked(encoder, destination.view())
    }
    fn encode_checked(
        &self,
        encoder: &mut AllocationEncoder,
        destination: &wgpu::TextureView,
    ) -> Result<(), RenderError> {
        encoder.check_owner(&self.owner)?;
        if destination.texture().format() != self.descriptor.format {
            return Err(RenderError::Invalid(
                "output destination format differs from descriptor".into(),
            ));
        }
        if destination.texture() == &*self.source {
            return Err(RenderError::Invalid(
                "output source and destination must be distinct".into(),
            ));
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SDR output conversion"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: destination,
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

    /// This binding retains its input allocation after the original target is
    /// dropped. Merge with other owners to count its identity only once.
    pub fn texture_resource_ledger(&self) -> TextureResourceLedger {
        let mut ledger = TextureResourceLedger::default();
        ledger.retained(
            &self.source,
            crate::TextureCategory::CaptureTargets,
            "output/source".into(),
        );
        ledger
    }
}

/// Reusable working-precision and SDR targets for screenshots or video frames.
/// The staging capacity is separate from application-owned texture capacity.
pub struct SdrCaptureTarget {
    source: RenderTarget,
    output: ImageCaptureTarget,
    conversion: ColorOutput,
}
impl SdrCaptureTarget {
    /// Native working attachment for a producer other than the scene compositor.
    /// Its pipeline must target `working_format()` and submit before readback.
    pub fn working_view(&self) -> &crate::AdmittedTextureView {
        &self.source.view
    }
    pub fn working_format(&self) -> wgpu::TextureFormat {
        self.source.texture().format()
    }
    pub fn width(&self) -> u32 {
        self.source.width
    }
    pub fn height(&self) -> u32 {
        self.source.height
    }
    pub fn transform(&self) -> OutputTransform {
        self.conversion.descriptor().transform
    }
    pub fn texture_resource_ledger(&self) -> TextureResourceLedger {
        let mut ledger = self.conversion.texture_resource_ledger();
        ledger.merge(self.output.texture_resource_ledger());
        ledger
    }
    pub fn texture_bytes(&self) -> u64 {
        self.source.texture_bytes() + self.output.texture_bytes()
    }
    pub fn readback_bytes(&self) -> u64 {
        self.output.readback_bytes()
    }
}

impl Renderer {
    /// Separate SDR attachment for preview. It does not change scene precision
    /// and has no staging/readback allocation. Retain and reuse it across frames.
    pub fn color_output_target(
        &self,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<RenderTarget, RenderError> {
        self.check_health()?;
        validate_output_format(format)?;
        self.render_target_with_format(
            width,
            height,
            format,
            crate::TextureCategory::PreviewTargets,
        )
    }
    pub fn sdr_capture_target(
        &self,
        width: u32,
        height: u32,
        transform: OutputTransform,
    ) -> Result<SdrCaptureTarget, RenderError> {
        self.sdr_output_target(width, height, transform, AlphaMode::Straight)
    }
    /// Opaque encoded RGB over linear black. Tone mapping precedes linear
    /// premultiplication; alpha is then one to prevent another encoder matte.
    pub fn sdr_video_target(
        &self,
        width: u32,
        height: u32,
        transform: OutputTransform,
    ) -> Result<SdrCaptureTarget, RenderError> {
        self.sdr_output_target(width, height, transform, AlphaMode::Premultiplied)
    }
    fn sdr_output_target(
        &self,
        width: u32,
        height: u32,
        transform: OutputTransform,
        alpha: AlphaMode,
    ) -> Result<SdrCaptureTarget, RenderError> {
        self.check_health()?;
        transform.validate()?;
        let source = self.render_target(width, height)?;
        let descriptor = ColorOutputDescriptor {
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            alpha,
            transform,
        };
        let conversion = ColorOutput::for_render_target_mode(
            self,
            &source,
            descriptor,
            alpha == AlphaMode::Premultiplied,
        )?;
        let output = self.image_capture_target(
            width,
            height,
            ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha,
                },
                ChannelPrecision::Unorm8,
            ),
        )?;
        Ok(SdrCaptureTarget {
            source,
            output,
            conversion,
        })
    }
    fn check_sdr_target(&self, target: &SdrCaptureTarget) -> Result<(), RenderError> {
        self.check_health()?;
        target.source.check_owner(self)?;
        target.output.check_owner(self)?;
        target.conversion.check_owner(self)?;
        if self.target_format != target.working_format() {
            return Err(RenderError::Invalid(
                "SDR capture working target format differs from renderer".into(),
            ));
        }
        Ok(())
    }
    /// Convert already submitted native content and read its explicit SDR output.
    /// The retained target may be reused by exposure and custom graph producers.
    pub fn read_sdr_target(
        &self,
        target: &SdrCaptureTarget,
    ) -> Result<crate::CapturedImage, RenderError> {
        self.check_sdr_target(target)?;
        let mut encoder = self.allocation_command_encoder_for(
            crate::gpu_commands::CommandStage::CompositionAndPresentation,
            &wgpu::CommandEncoderDescriptor {
                label: Some("native content to SDR readback"),
            },
        )?;
        target
            .conversion
            .encode_image(self, &mut encoder, &target.output)?;
        self.submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(target.texture_resource_ledger())
        })?;
        self.check_health()?;
        self.read_image_target(&target.output)
    }
    pub fn capture_exposure_sdr(
        &self,
        exposure: &crate::ExposureTarget,
        target: &SdrCaptureTarget,
    ) -> Result<crate::CapturedImage, RenderError> {
        self.check_sdr_target(target)?;
        self.resolve_exposure(exposure, &target.source)?;
        self.read_sdr_target(target)
    }
    /// Render at working precision, then convert and quantize to the target's
    /// explicit SDR/alpha policy. Capture targets use straight alpha for PNG;
    /// video targets provide encoded RGB over linear black. Raw APIs are unchanged.
    pub fn capture_sdr(
        &mut self,
        scene: &Scene,
        target: &SdrCaptureTarget,
    ) -> Result<(crate::CapturedImage, RenderStats), RenderError> {
        self.check_sdr_target(target)?;
        self.preflight_effects(scene, target.width(), target.height())?;
        let began = std::time::Instant::now();
        let mut encoder = self.allocation_command_encoder_for(
            crate::gpu_commands::CommandStage::CompositionAndPresentation,
            &wgpu::CommandEncoderDescriptor {
                label: Some("working precision to SDR capture"),
            },
        )?;
        let mut stats = self.encode(
            scene,
            &target.source.view,
            target.width(),
            target.height(),
            &mut encoder,
            None,
        )?;
        target
            .conversion
            .encode_image(self, &mut encoder, &target.output)?;
        stats.draw_calls += 1;
        self.submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(target.texture_resource_ledger());
        })?;
        stats.cpu_prepare_us = began.elapsed().as_micros() as u64;
        self.check_health()?;
        Ok((self.read_image_target(&target.output)?, stats))
    }
}
