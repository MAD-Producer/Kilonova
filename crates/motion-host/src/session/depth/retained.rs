//! Current-document result ownership, separate from the single worker ticket.
//! No source IO or inference occurs here. Every entry keeps its admission guard.
use super::{Binding, CalibrationSample};
use crate::depth::{Error, Estimate, Identity};
use motion_model::depth::PreparedDepthInput;
use std::sync::Arc;

pub(super) struct Entry {
    pub binding: Binding,
    pub estimate: Arc<Estimate>,
    pub input: PreparedDepthInput,
    pub image: Option<motion_render::DepthImage>,
    pub video_end_us: Option<u64>,
}
impl Entry {
    pub fn video_interval(&self) -> Option<(u64, u64)> {
        if !matches!(self.binding.key, Identity::Video(_)) { return None; }
        let pts = self.estimate.resource.descriptor().source.pts;
        let end = self.video_end_us?;
        (pts.timescale == 1_000_000 && pts.ticks >= 0 && (pts.ticks as u64) < end)
            .then_some((pts.ticks as u64, end))
    }
    fn bytes(&self) -> usize {
        let resource = &self.estimate.resource;
        resource.values().len() * 4
            + resource.validity().map_or(0, |p| p.len())
            + resource.confidence().map_or(0, |p| p.len() * 4)
    }
    fn image_bytes(&self) -> usize { self.image.as_ref().map_or(0, |image| image.bytes()) }
    pub fn sample(&self) -> CalibrationSample {
        CalibrationSample {
            request: self.binding.request,
            estimate: self.estimate.clone(),
            input: self.input,
            image: self.image.clone(),
            video_end_us: self.video_end_us,
            source: self.binding.source.clone(),
            key: self.binding.key.clone(),
            frame: self.binding.frame,
            composition: self.binding.composition.clone(),
        }
    }
}

#[derive(Default)]
pub(super) struct Results {
    pub entries: Vec<Entry>,
    pub bytes: usize,
    pub image_bytes: usize,
    /// Saturating lifetime admission failures of this session depth owner.
    /// Clearing or invalidating an admission does not reset this count.
    pub rejections: u64,
}
impl Results {
    /// A temporal cache evicts the oldest completed video sample first. Static
    /// admissions and current render samples are pinned. Preflight before evicting so an oversized incoming
    /// result cannot destroy working admissions. This is completion-order FIFO,
    /// not a claim of unlimited playback retention or inference throughput.
    pub fn admit_video(&mut self, entry: Entry, byte_limit: usize, sample_limit: usize,
        pinned: impl Fn(&Entry) -> bool) -> crate::depth::Result<()> {
        let mut scalar = entry.bytes();
        let mut images = entry.image_bytes();
        let mut samples = 1;
        for old in &self.entries {
            if (old.video_interval().is_none() || pinned(old)) && !old.binding.key.same_request(&entry.binding.key) {
                scalar += old.bytes(); images += old.image_bytes(); samples += 1;
            }
        }
        if scalar + images > byte_limit || samples > sample_limit {
            self.rejections = self.rejections.saturating_add(1);
            return Err(Error::RetentionCapacity {
                request: entry.binding.request, asset: entry.estimate.resource.descriptor().source.asset,
                required_bytes: scalar + images, byte_limit, required_samples: samples, sample_limit,
            });
        }
        self.remove(&entry.binding.key);
        while self.bytes + self.image_bytes + entry.bytes() + entry.image_bytes() > byte_limit
            || self.entries.len() + 1 > sample_limit {
            let Some(index) = self.entries.iter().position(|old| old.video_interval().is_some() && !pinned(old)) else {
                // Preflight guarantees capacity after the remaining video entries.
                return Err(Error::Invalid("video depth retention accounting diverged"));
            };
            let old = self.entries.remove(index);
            self.bytes -= old.bytes(); self.image_bytes -= old.image_bytes();
        }
        self.admit(entry, byte_limit, sample_limit)
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&Entry) -> bool) {
        self.entries.retain(|entry| {
            if keep(entry) { return true; }
            self.bytes -= entry.bytes();
            self.image_bytes -= entry.image_bytes();
            false
        });
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.image_bytes = 0;
    }
    pub fn remove(&mut self, key: &Identity) {
        self.entries.retain(|entry| {
            if entry.binding.key.same_request(key) {
                self.bytes -= entry.bytes();
                self.image_bytes -= entry.image_bytes();
                false
            } else {
                true
            }
        });
    }
    /// The public request API selects one inference configuration per asset.
    /// A new model/crop request supersedes that asset's old result as well;
    /// another asset must not be evicted or used as a fallback.
    pub fn remove_source(&mut self, asset: u64) {
        self.entries.retain(|entry| {
            if entry.estimate.resource.descriptor().source.asset == asset {
                self.bytes -= entry.bytes();
                self.image_bytes -= entry.image_bytes();
                false
            } else {
                true
            }
        });
    }
    pub fn admit(
        &mut self,
        entry: Entry,
        byte_limit: usize,
        sample_limit: usize,
    ) -> crate::depth::Result<()> {
        let previous = self
            .entries
            .iter()
            .find(|old| old.binding.key.same_request(&entry.binding.key));
        let required_samples = self.entries.len() + usize::from(previous.is_none());
        // Both scalar planes and original immutable image allocations count.
        let scalar_bytes = self.bytes - previous.map_or(0, Entry::bytes) + entry.bytes();
        let image_bytes = self.image_bytes - previous.map_or(0, Entry::image_bytes) + entry.image_bytes();
        let required_bytes = scalar_bytes + image_bytes;
        if required_bytes > byte_limit || required_samples > sample_limit {
            self.rejections = self.rejections.saturating_add(1);
            return Err(Error::RetentionCapacity {
                request: entry.binding.request,
                asset: entry.estimate.resource.descriptor().source.asset,
                required_bytes,
                byte_limit,
                required_samples,
                sample_limit,
            });
        }
        self.remove(&entry.binding.key);
        self.bytes = scalar_bytes;
        self.image_bytes = image_bytes;
        self.entries.push(entry);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
