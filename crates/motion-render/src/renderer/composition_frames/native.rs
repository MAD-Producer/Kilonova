//! Admission before dispatch and format-preserving installation after completion.
use super::*;
use motion_effects::{
    image_contract::{ImageContract, ImageMeaning, ImageUsage},
    AlphaMode, WorkingSpace,
};

/// This is an actual admitted destination on the foreground renderer. It is
/// never a background RenderTarget and cannot be installed into another cache.
pub struct NativeCompositionCandidate {
    key: CompositionFrameKey,
    target: RenderTarget,
    epoch: Arc<()>,
    reservation: Arc<()>,
}
impl NativeCompositionCandidate {
    pub fn key(&self) -> CompositionFrameKey {
        self.key
    }
    pub fn texture_bytes(&self) -> u64 {
        self.target.texture_bytes()
    }
}

impl CompositionFrameCache {
    /// There is at most one active destination per cache. An outstanding canceled
    /// destination remains charged to the shared budget until its real owner
    /// drops it, even after invalidation removes it from this cache's counters.
    pub fn prepare_native(
        &mut self,
        renderer: &Renderer,
        key: CompositionFrameKey,
    ) -> Result<Option<NativeCompositionCandidate>> {
        if key.context != self.context {
            self.info.stale = self.info.stale.saturating_add(1);
            return Ok(None);
        }
        self.check_owner(renderer)?;
        renderer.check_health()?;
        let dimension_limit = renderer.device.limits().max_texture_dimension_2d;
        if key.width > dimension_limit || key.height > dimension_limit {
            return Err(RenderError::Invalid(
                "native cache destination exceeds device dimensions".into(),
            ));
        }
        let precision = crate::image_formats::color_precision(key.format)
            .ok_or_else(|| RenderError::Invalid("invalid native cache precision".into()))?;
        let contract = native_contract(precision);
        let storage = renderer.image_capabilities.negotiate(
            contract,
            ImageUsage {
                sampled: true,
                copy_src: true,
                copy_dst: true,
                ..Default::default()
            },
        )?;
        if crate::image_formats::native_format(storage) != key.format {
            return Err(RenderError::Invalid(
                "native cache key differs from the declared working storage".into(),
            ));
        }
        let bytes = storage
            .capacity(key.width, key.height)
            .ok_or_else(|| RenderError::Invalid("native cache capacity overflow".into()))?;
        if self.pending_frames() != 0 || !self.make_room(bytes, 1) {
            self.info.refused = self.info.refused.saturating_add(1);
            return Ok(None);
        }
        let target = allocate(renderer, key.width, key.height, key.format).map_err(|cause| {
            self.info.refused = self.info.refused.saturating_add(1);
            cause.with_diagnostic("composition frame cache phase=reserve-native".into())
        })?;
        let reservation = Arc::new(());
        self.pending = Some((Arc::downgrade(&reservation), bytes));
        self.owner = Some(renderer.allocation_owner.clone());
        Ok(Some(NativeCompositionCandidate {
            key,
            target,
            epoch: self.epoch.clone(),
            reservation,
        }))
    }

    /// The coordinator must first match its scheduler ticket. This additionally
    /// checks cache identity/generation, exact native layout and semantic contract
    /// before queue IO. Stale completion is a normal false result. Invalid bytes
    /// are an error; neither case replaces an existing valid cached frame.
    pub fn install_native(
        &mut self,
        renderer: &Renderer,
        candidate: NativeCompositionCandidate,
        image: &crate::CapturedImage,
    ) -> Result<bool> {
        if candidate.key.context != self.context
            || !Arc::ptr_eq(&candidate.epoch, &self.epoch)
            || !self.pending.as_ref().is_some_and(|(reservation, _)| {
                std::sync::Weak::ptr_eq(reservation, &Arc::downgrade(&candidate.reservation))
            })
        {
            self.info.stale = self.info.stale.saturating_add(1);
            return Ok(false);
        }
        self.check_owner(renderer)?;
        candidate.target.check_owner(renderer)?;
        renderer.check_health()?;
        let key = candidate.key;
        let precision = crate::image_formats::color_precision(key.format)
            .ok_or_else(|| RenderError::Invalid("invalid native cache precision".into()))?;
        if image.contract != native_contract(precision)
            || image.contract.preferred_storage()? != image.storage
            || (
                image.width,
                image.height,
                crate::image_formats::native_format(image.storage),
            ) != (key.width, key.height, key.format)
            || image.storage.capacity(image.width, image.height) != Some(image.pixels.len() as u64)
        {
            return Err(RenderError::Invalid(
                "native cache completion contract, storage or byte layout differs from reservation"
                    .into(),
            ));
        }
        if self.entries.iter().any(|entry| entry.key == key) {
            self.pending = None;
            return Ok(true);
        }
        // Prepare a valid submission before queue IO. Track the destination
        // before write; a later submission/health failure must retain its lease.
        let encoder = renderer.allocation_command_encoder_for(
            crate::gpu_commands::CommandStage::Composition,
            &wgpu::CommandEncoderDescriptor {
                label: Some("Motion Studio native composition upload"),
            },
        )?;
        renderer
            .allocation_uploads
            .track(candidate.target.texture.allocation())?;
        renderer.queue.write_texture(
            candidate.target.texture().as_image_copy(),
            &image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(
                    key.width
                        * key.format.block_copy_size(None).ok_or_else(|| {
                            RenderError::Invalid("native cache row size unavailable".into())
                        })?,
                ),
                rows_per_image: Some(key.height),
            },
            wgpu::Extent3d {
                width: key.width,
                height: key.height,
                depth_or_array_layers: 1,
            },
        );
        renderer.submit_allocated(encoder.finish(), |ledger| {
            candidate.target.collect_texture_resources(
                ledger,
                "composition/native_upload",
                crate::TextureState::Retained,
            );
        })?;
        renderer.check_health()?;
        self.pending = None;
        self.entries.push_back(Arc::new(CachedCompositionFrame {
            key,
            target: candidate.target,
        }));
        self.info.installed = self.info.installed.saturating_add(1);
        Ok(true)
    }

    pub(super) fn pending_frames(&self) -> usize {
        usize::from(
            self.pending
                .as_ref()
                .is_some_and(|(token, _)| token.strong_count() > 0),
        )
    }
    pub(super) fn pending_bytes(&self) -> u64 {
        self.pending
            .as_ref()
            .filter(|(token, _)| token.strong_count() > 0)
            .map_or(0, |(_, bytes)| *bytes)
    }
    pub(super) fn cancel_pending(&mut self) {
        self.pending = None;
        self.epoch = Arc::new(());
    }
    pub(super) fn make_room(&mut self, bytes: u64, frames: usize) -> bool {
        let pending_bytes = self.pending_bytes();
        let pending_frames = self.pending_frames();
        if self.info.max_frames < pending_frames + frames
            || bytes > self.info.quota_bytes.saturating_sub(pending_bytes)
        {
            return false;
        }
        let available = self.info.quota_bytes - pending_bytes - bytes;
        while self.entries.len() + pending_frames + frames > self.info.max_frames
            || self.bytes() > available
        {
            self.evict();
        }
        true
    }
}

fn native_contract(precision: motion_effects::image_contract::ChannelPrecision) -> ImageContract {
    ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        precision,
    )
}

#[cfg(test)]
mod tests;
