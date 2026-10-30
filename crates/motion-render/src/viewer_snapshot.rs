use crate::{ColorOutput, ColorOutputDescriptor, OutputTransform, RenderError, RenderTarget, Renderer};
use motion_effects::{AlphaMode, image_contract::ChannelPrecision};

/// A preserved working image and, when needed, its matching SDR display image.
/// The caller publishes a replacement only after `capture` succeeds.
pub struct ViewerSnapshot {
    native: RenderTarget,
    display: Option<RenderTarget>,
}
impl ViewerSnapshot {
    pub fn capture(renderer: &Renderer, source: &RenderTarget) -> Result<Self, RenderError> {
        let native = renderer.snapshot_target(source)?;
        let precision = crate::image_formats::color_precision(native.view.texture().format())
            .ok_or_else(|| RenderError::Invalid("viewer snapshot has unsupported working format".into()))?;
        let display = if precision == ChannelPrecision::Unorm8 {
            None
        } else {
            let target = renderer.color_output_target(native.width, native.height, wgpu::TextureFormat::Rgba8UnormSrgb)?;
            let conversion = ColorOutput::for_render_target(renderer, &native, ColorOutputDescriptor {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                alpha: AlphaMode::Premultiplied,
                transform: OutputTransform::SDR_CLIP,
            })?;
            let mut encoder = renderer.allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Motion Studio viewer snapshot display conversion"),
            })?;
            conversion.encode(renderer, &mut encoder, &target)?;
            renderer.submit_allocated(encoder.finish(), |ledger| {
                native.collect_texture_resources(ledger, "viewer/snapshot-working", crate::TextureState::Retained);
                target.collect_texture_resources(ledger, "viewer/snapshot-display", crate::TextureState::Retained);
            })?;
            renderer.check_health()?;
            Some(target)
        };
        Ok(Self { native, display })
    }
    pub fn native(&self) -> &RenderTarget { &self.native }
    pub fn display(&self) -> &RenderTarget { self.display.as_ref().unwrap_or(&self.native) }
    pub fn collect_texture_resources(&self, ledger: &mut crate::TextureResourceLedger) {
        self.native.collect_texture_resources(ledger, "viewer/snapshot-working", crate::TextureState::Retained);
        if let Some(display) = &self.display {
            display.collect_texture_resources(ledger, "viewer/snapshot-display", crate::TextureState::Retained);
        }
    }
}
