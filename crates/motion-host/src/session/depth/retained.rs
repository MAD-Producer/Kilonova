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
}
impl Entry {
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
