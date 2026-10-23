//! Explicit CPU ownership read; never used by ordinary frame polling.
use super::*;
use crate::resource_memory::PixelMemorySnapshot;
use crate::video_frame::VideoPixels;
use std::{collections::HashMap, sync::TryLockError};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameMemory {
    id: String,
    format: &'static str,
    payload_bytes: usize,
    capacity_bytes: usize,
    references: Vec<Value>,
}

impl VideoFrames {
    /// Pixel payloads and Vec capacities retained by active stream caches.
    /// Busy streams remain unknown. A poisoned cache preserves an actual error.
    /// Snapshot-local Arc clones prevent eviction/address reuse during deduplication;
    /// no frame or pointer identity survives this call.
    pub fn cache_memory_ledger(&self) -> Result<PixelMemorySnapshot> {
        let mut streams: Vec<_> = self.streams.values().collect();
        streams.sort_by_key(|stream| stream.object);
        let mut held = Vec::new();
        let mut unavailable = Vec::new();
        for stream in streams {
            match stream.shared.0.try_lock() {
                Ok(state) => {
                    held.extend(state.cache.retained_frames().map(|frame| {
                        (stream.object, stream.asset.id, Arc::clone(frame))
                    }));
                }
                Err(TryLockError::WouldBlock) => unavailable.push(json!({
                    "object":stream.object,"source":stream.asset.id,"reason":"busy",
                    "payloadBytes":null,"capacityBytes":null})),
                Err(TryLockError::Poisoned(_)) => return Err(format!(
                    "CPU video cache observation failed object={} source={} phase=cache_memory_ledger: cache state poisoned",
                    stream.object, stream.asset.id).into()),
            }
        }
        let mut identities = HashMap::new();
        let mut resources: Vec<FrameMemory> = Vec::new();
        let mut payload = 0u64;
        let mut capacity = 0u64;
        for (object, source, frame) in &held {
            // The pointer is a temporary equality key only; never serialize it.
            let identity = Arc::as_ptr(frame);
            let index = *identities.entry(identity).or_insert_with(|| {
                let (bytes, reserved, format) = match &frame.pixels {
                    VideoPixels::Rgba(pixels) => (pixels.len(), pixels.capacity(), "rgba8"),
                    VideoPixels::Yuv(pixels) => (
                        pixels.y.len() + pixels.uv.len(),
                        pixels.y.capacity() + pixels.uv.capacity(),
                        "yuv420",
                    ),
                };
                payload += bytes as u64;
                capacity += reserved as u64;
                resources.push(FrameMemory {
                    id: format!("cpu-video-frame-{}", resources.len() + 1),
                    format,
                    payload_bytes: bytes,
                    capacity_bytes: reserved,
                    references: Vec::new(),
                });
                resources.len() - 1
            });
            resources[index]
                .references
                .push(json!({"object":object,"source":source,"ptsUs":frame.pts}));
        }
        Ok(PixelMemorySnapshot {
            known_payload_bytes: payload,
            known_capacity_bytes: capacity,
            details: json!({"version":1,"scope":"active_stream_cached_pixel_vectors",
            "resourceCount":resources.len(),"resources":resources,
            "streamCount":self.streams.len(),"unavailableStreams":unavailable,
            "allActiveStreamCachesObserved":unavailable.is_empty(),
            "atomicAcrossStreams":false,"identityScope":"one_snapshot",
            "retiringWorkers":self.retiring.iter().filter(|worker| !worker.is_finished()).count(),
            "completeCoverage":false,"unknownBytes":null,
            "excludes":["retiring_worker_caches","in_flight_decoder_output","external_frame_readers",
                "decoder_internal_allocations","Arc_and_container_overhead"]}),
        })
    }
}

#[cfg(test)]
mod tests;
