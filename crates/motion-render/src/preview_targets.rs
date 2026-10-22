use crate::{RenderError, RenderTarget, Renderer, ResourceBudgetScope, ResourceRejection};
use std::ops::Deref;

/// At most two targets. Only a successfully submitted candidate becomes front.
pub struct PreviewFrameTargets {
    front: RenderTarget,
    candidate: Option<RenderTarget>,
    valid: bool,
}
impl Deref for PreviewFrameTargets {
    type Target = RenderTarget;
    fn deref(&self) -> &RenderTarget {
        &self.front
    }
}
impl PreviewFrameTargets {
    pub fn new(renderer: &Renderer, width: u32, height: u32) -> Result<Self, RenderError> {
        let bytes = target_bytes(renderer, width, height)?;
        check_budget(renderer, bytes)?;
        Ok(Self {
            front: renderer.render_target_in(width, height, crate::TextureCategory::PreviewTargets)?,
            candidate: None,
            valid: false,
        })
    }
    pub fn valid(&self) -> bool {
        self.valid
    }
    pub fn prepared(&self) -> Option<&RenderTarget> {
        self.candidate.as_ref()
    }
    pub fn invalidate(&mut self) {
        self.valid = false;
        self.candidate = None;
    }
    pub fn discard_candidate(&mut self) {
        self.candidate = None;
    }
    pub fn collect_texture_resources(&self, ledger: &mut crate::TextureResourceLedger) {
        self.front.collect_texture_resources(
            ledger,
            "preview/front",
            crate::TextureState::PreviewFront,
        );
        if let Some(candidate) = &self.candidate {
            candidate.collect_texture_resources(
                ledger,
                "preview/candidate",
                crate::TextureState::PreviewCandidate,
            );
        }
    }
    pub fn texture_bytes(&self) -> u64 {
        self.front.texture_bytes()
            + self
                .candidate
                .as_ref()
                .map_or(0, RenderTarget::texture_bytes)
    }
    pub fn prepare(
        &mut self,
        renderer: &Renderer,
        width: u32,
        height: u32,
    ) -> Result<&RenderTarget, RenderError> {
        let bytes = target_bytes(renderer, width, height)?;
        // Check even when reusing an existing candidate: a runtime policy may
        // have been lowered since it was allocated. Reject before mutation or
        // allocation, leaving the last successfully presented image intact.
        check_budget(renderer, self.front.texture_bytes() + bytes)?;
        if self
            .candidate
            .as_ref()
            .is_none_or(|t| (t.width, t.height) != (width, height))
        {
            self.candidate = None;
            self.candidate = Some(renderer.render_target_in(width, height, crate::TextureCategory::PreviewTargets)?);
        }
        Ok(self.candidate.as_ref().unwrap())
    }
    /// Call after submitting the encoder that wrote the prepared candidate.
    pub fn commit_submitted(&mut self) {
        let mut candidate = self.candidate.take().expect("prepared preview candidate");
        std::mem::swap(&mut self.front, &mut candidate);
        self.candidate = Some(candidate);
        self.valid = true;
    }
}

fn check_budget(renderer: &Renderer, required_bytes: u64) -> Result<(), RenderError> {
    let budget = renderer.memory_policy().preview_target_budget_bytes;
    if required_bytes > budget {
        return Err(RenderError::budget(
            ResourceBudgetScope::PreviewTarget,
            required_bytes,
            budget,
            "retained and candidate preview targets exceed configured budget",
        ));
    }
    Ok(())
}

fn target_bytes(renderer: &Renderer, width: u32, height: u32) -> Result<u64, RenderError> {
    if width == 0 || height == 0 {
        return Err(RenderError::Invalid("invalid preview target size".into()));
    }
    let limit = renderer.device.limits().max_texture_dimension_2d;
    if width > limit || height > limit {
        return Err(RenderError::Resource {
            rejection: ResourceRejection::TextureDimensions {
                width,
                height,
                max_dimension: limit,
            },
            message: "preview target exceeds device dimensions".into(),
        });
    }
    // Offscreen color targets have one mip, one sample and one layer. Query
    // the actual format rather than applying an RGBA8 estimate to float output.
    let format = renderer.target_format;
    let bytes = u64::from(
        format
            .block_copy_size(None)
            .filter(|_| format.block_dimensions() == (1, 1))
            .ok_or_else(|| {
                RenderError::Invalid("unsupported preview target color format".into())
            })?,
    );
    // Preserve the existing capture/plan pixel limit, independently of format.
    let pixels = u64::from(width) * u64::from(height);
    if pixels > 16 * 1024 * 1024 {
        return Err(RenderError::budget(
            ResourceBudgetScope::PreviewTarget,
            pixels * bytes,
            16 * 1024 * 1024 * bytes,
            "preview target exceeds 16 megapixels",
        ));
    }
    Ok(pixels * bytes)
}
