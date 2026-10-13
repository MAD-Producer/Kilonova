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
        Ok(Self {
            front: renderer.render_target(width, height)?,
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
        self.front.collect_texture_resources(ledger, "preview/front", crate::TextureState::PreviewFront);
        if let Some(candidate) = &self.candidate {
            candidate.collect_texture_resources(ledger, "preview/candidate", crate::TextureState::PreviewCandidate);
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
        let bytes = u64::from(width) * u64::from(height) * 4;
        if bytes > 64 * 1024 * 1024 {
            return Err(RenderError::budget(
                ResourceBudgetScope::PreviewTarget,
                bytes,
                64 * 1024 * 1024,
                "preview target exceeds 16 megapixels",
            ));
        }
        if self
            .candidate
            .as_ref()
            .is_none_or(|t| (t.width, t.height) != (width, height))
        {
            self.candidate = None;
            self.candidate = Some(renderer.render_target(width, height)?);
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
