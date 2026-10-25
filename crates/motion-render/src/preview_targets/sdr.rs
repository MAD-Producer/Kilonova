//! Displayable retained fronts, with admitted native working candidates.
//! The original raw PreviewFrameTargets contract remains unchanged.
use super::*;
use crate::{AllocationEncoder, ColorOutput, ColorOutputDescriptor, OutputTransform};
use motion_effects::{AlphaMode, image_contract::ChannelPrecision};

struct Output {
    target: RenderTarget,
    conversion: ColorOutput,
}
impl Output {
    fn new(renderer: &Renderer, source: &RenderTarget) -> Result<Option<Self>, RenderError> {
        if crate::image_formats::color_precision(source.texture().format())
            == Some(ChannelPrecision::Unorm8)
        {
            return Ok(None);
        }
        let target = renderer.color_output_target(
            source.width,
            source.height,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )?;
        let conversion = ColorOutput::for_render_target(
            renderer,
            source,
            ColorOutputDescriptor {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                alpha: AlphaMode::Premultiplied,
                transform: OutputTransform::SDR_CLIP,
            },
        )?;
        Ok(Some(Self { target, conversion }))
    }
    fn bytes(&self) -> u64 {
        self.target.texture_bytes()
    }
}

/// Two working slots, each with an optional retained SDR attachment and binding.
/// UNORM8 keeps the original direct presentation path. The host must encode output
/// after writing the prepared working target and commit only after submission.
pub struct SdrPreviewFrameTargets {
    working: PreviewFrameTargets,
    front_output: Option<Output>,
    candidate_output: Option<Output>,
    output_encoded: bool,
}
impl Deref for SdrPreviewFrameTargets {
    type Target = RenderTarget;
    fn deref(&self) -> &RenderTarget {
        self.front_output
            .as_ref()
            .map_or(&self.working.front, |output| &output.target)
    }
}
impl SdrPreviewFrameTargets {
    pub fn new(renderer: &Renderer, width: u32, height: u32) -> Result<Self, RenderError> {
        check_budget(renderer, slot_bytes(renderer, width, height)?)?;
        let working = PreviewFrameTargets::new(renderer, width, height)?;
        let front_output = Output::new(renderer, &working.front)?;
        Ok(Self {
            working,
            front_output,
            candidate_output: None,
            output_encoded: false,
        })
    }
    pub fn valid(&self) -> bool {
        self.working.valid()
    }
    pub fn prepared(&self) -> Option<&RenderTarget> {
        self.working.prepared()
    }
    /// Available only when the native candidate has its required display target.
    pub fn prepared_output(&self) -> Option<&RenderTarget> {
        let source = self.working.prepared()?;
        if crate::image_formats::color_precision(source.texture().format())
            == Some(ChannelPrecision::Unorm8)
        {
            Some(source)
        } else {
            self.candidate_output.as_ref().map(|output| &output.target)
        }
    }
    pub fn invalidate(&mut self) {
        self.discard_candidate();
        self.working.invalidate();
    }
    pub fn discard_candidate(&mut self) {
        self.candidate_output = None;
        self.working.discard_candidate();
        self.output_encoded = false;
    }
    pub fn texture_bytes(&self) -> u64 {
        self.working.texture_bytes()
            + self.front_output.as_ref().map_or(0, Output::bytes)
            + self.candidate_output.as_ref().map_or(0, Output::bytes)
    }
    pub fn collect_texture_resources(&self, ledger: &mut crate::TextureResourceLedger) {
        self.working.collect_texture_resources(ledger);
        for (output, owner, state) in [
            (
                &self.front_output,
                "preview/sdr_front",
                crate::TextureState::PreviewFront,
            ),
            (
                &self.candidate_output,
                "preview/sdr_candidate",
                crate::TextureState::PreviewCandidate,
            ),
        ] {
            if let Some(output) = output {
                output
                    .target
                    .collect_texture_resources(ledger, owner, state);
            }
        }
    }
    pub fn prepare(
        &mut self,
        renderer: &Renderer,
        width: u32,
        height: u32,
    ) -> Result<&RenderTarget, RenderError> {
        renderer.check_health()?;
        self.working.front.check_owner(renderer)?;
        let front_bytes = self.working.front.texture_bytes()
            + self.front_output.as_ref().map_or(0, Output::bytes);
        check_budget(renderer, front_bytes + slot_bytes(renderer, width, height)?)?;
        let replace = self.working.prepared().is_none_or(|target| {
            (target.width, target.height) != (width, height)
                || target.texture().format() != renderer.target_format
        });
        if replace {
            self.candidate_output = None;
        }
        self.output_encoded = false;
        let source = self.working.prepare(renderer, width, height)?;
        if self.candidate_output.is_none() {
            self.candidate_output = Output::new(renderer, source)?;
        }
        Ok(source)
    }
    /// Returns the additional draw count. Healthy reuse performs no allocation,
    /// compilation, queue upload or CPU readback. Even the direct path must call
    /// this method so forgetting conversion cannot publish a floating candidate.
    pub fn encode_output(
        &mut self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
    ) -> Result<u32, RenderError> {
        renderer.check_health()?;
        let source = self.working.prepared().ok_or_else(|| {
            RenderError::Invalid("prepare preview before output conversion".into())
        })?;
        source.check_owner(renderer)?;
        encoder.check_owner(&renderer.allocation_owner)?;
        let draws = if let Some(output) = &self.candidate_output {
            output
                .conversion
                .encode(renderer, encoder, &output.target)?;
            1
        } else if crate::image_formats::color_precision(source.texture().format())
            == Some(ChannelPrecision::Unorm8)
        {
            0
        } else {
            return Err(RenderError::Invalid(
                "high-precision preview has no SDR output target".into(),
            ));
        };
        self.output_encoded = true;
        Ok(draws)
    }
    /// No front changes until both working and display candidate were encoded.
    /// The caller remains responsible for successful allocated submission.
    pub fn commit_submitted(&mut self) -> Result<(), RenderError> {
        if !self.output_encoded || self.prepared_output().is_none() {
            return Err(RenderError::Invalid(
                "encode preview output before committing its submission".into(),
            ));
        }
        self.working.commit_submitted();
        std::mem::swap(&mut self.front_output, &mut self.candidate_output);
        self.output_encoded = false;
        Ok(())
    }
}
fn slot_bytes(renderer: &Renderer, width: u32, height: u32) -> Result<u64, RenderError> {
    let native = target_bytes(renderer, width, height)?;
    Ok(native
        + if renderer.working_precision() == ChannelPrecision::Unorm8 {
            0
        } else {
            u64::from(width) * u64::from(height) * 4
        })
}
