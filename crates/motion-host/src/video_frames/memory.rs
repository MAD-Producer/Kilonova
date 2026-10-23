//! Explicit CPU ownership read; never used by ordinary frame polling.
use super::*;
use crate::resource_memory::{PixelCacheCollector, PixelMemorySnapshot};
use std::sync::TryLockError;
const MAX_OBSERVED_STREAMS: usize = 1024;
#[cfg(test)]
use crate::video_frame::VideoPixels;

impl VideoFrames {
    /// Pixel payloads and Vec capacities retained by active stream caches.
    /// Busy streams remain unknown. A poisoned cache preserves an actual error.
    /// Snapshot-local Arc clones prevent eviction/address reuse during deduplication;
    /// no frame or pointer identity survives this call.
    pub fn cache_memory_ledger(&self) -> Result<PixelMemorySnapshot> {
        let mut collector = PixelCacheCollector::default();
        let info = self.collect_cache_memory(&mut collector, "active_video_stream")?;
        let mut result = collector.finish("active_stream_cached_pixel_vectors");
        for (key, value) in info {
            result.details[key] = value;
        }
        Ok(result)
    }

    /// Share the caller's identity scope across independent stream collections.
    /// Never block a decoder or consume its result/pending request.
    pub(crate) fn collect_cache_memory(
        &self,
        collector: &mut PixelCacheCollector,
        owner: &'static str,
    ) -> Result<serde_json::Map<String, Value>> {
        let mut streams: Vec<_> = self.streams.values().take(MAX_OBSERVED_STREAMS).collect();
        streams.sort_by_key(|stream| stream.object);
        let omitted = self.streams.len() - streams.len();
        let mut unavailable = Vec::new();
        for stream in streams {
            match stream.shared.0.try_lock() {
                Ok(state) => {
                    for frame in state.cache.retained_frames() {
                        collector.video(frame, owner, stream.object, stream.asset.id);
                    }
                }
                Err(TryLockError::WouldBlock) => unavailable.push(json!({
                    "owner":owner,"object":stream.object,"source":stream.asset.id,"reason":"busy",
                    "payloadBytes":null,"capacityBytes":null})),
                Err(TryLockError::Poisoned(_)) => return Err(format!(
                    "CPU video cache observation failed owner={owner} object={} source={} phase=cache_memory_ledger: cache state poisoned",
                    stream.object, stream.asset.id).into()),
            }
        }
        let mut info = serde_json::Map::new();
        info.insert("streamCount".into(), json!(self.streams.len()));
        info.insert(
            "observedStreams".into(),
            json!(self.streams.len() - omitted - unavailable.len()),
        );
        info.insert("unobservedStreams".into(), json!(omitted));
        info.insert(
            "allActiveStreamCachesObserved".into(),
            json!(omitted == 0 && unavailable.is_empty()),
        );
        info.insert("unavailableStreams".into(), json!(unavailable));
        info.insert("atomicAcrossStreams".into(), json!(false));
        info.insert(
            "retiringWorkers".into(),
            json!(self
                .retiring
                .iter()
                .filter(|worker| !worker.is_finished())
                .count()),
        );
        Ok(info)
    }
}

#[cfg(test)]
mod tests;
