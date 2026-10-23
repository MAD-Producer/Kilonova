//! Query-local ownership inventory; pointers never leave this collector.
use super::PixelMemorySnapshot;
use crate::video_frame::{DecodedFrame, VideoPixels};
use motion_render::image_resources::Pixels;
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};

const MAX_ALLOCATIONS: usize = 4096;
const MAX_REFERENCES: usize = 8;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PixelResource {
    id: String,
    format: &'static str,
    payload_bytes: usize,
    capacity_bytes: usize,
    references: Vec<Value>,
    omitted_references: u64,
}

#[derive(Default)]
pub(crate) struct PixelCacheCollector {
    identities: HashMap<(bool, usize), usize>,
    resources: Vec<PixelResource>,
    // Hold observed identities until every cache has been inspected, preventing
    // eviction/address reuse. All of these temporary clones die with the query.
    _video: Vec<Arc<DecodedFrame>>,
    _images: Vec<Arc<Pixels>>,
    unobserved_references: u64,
}

impl PixelCacheCollector {
    fn observe(
        &mut self,
        identity: (bool, usize),
        format: &'static str,
        payload: usize,
        capacity: usize,
        reference: Value,
    ) -> Option<(usize, bool)> {
        let (index, new) = if let Some(index) = self.identities.get(&identity) {
            (*index, false)
        } else {
            if self.resources.len() == MAX_ALLOCATIONS {
                self.unobserved_references = self.unobserved_references.saturating_add(1);
                return None;
            }
            let index = self.resources.len();
            self.identities.insert(identity, index);
            self.resources.push(PixelResource {
                id: format!("cpu-cached-pixels-{}", index + 1),
                format,
                payload_bytes: payload,
                capacity_bytes: capacity,
                references: Vec::new(),
                omitted_references: 0,
            });
            (index, true)
        };
        let resource = &mut self.resources[index];
        if resource.references.len() < MAX_REFERENCES {
            resource.references.push(reference);
        } else {
            resource.omitted_references = resource.omitted_references.saturating_add(1);
        }
        Some((index, new))
    }

    pub(crate) fn video(
        &mut self,
        frame: &Arc<DecodedFrame>,
        owner: &'static str,
        object: u64,
        source: u64,
    ) {
        let (payload, capacity, format) = match &frame.pixels {
            VideoPixels::Rgba(pixels) => (pixels.len(), pixels.capacity(), "rgba8"),
            VideoPixels::Yuv(pixels) => (
                pixels.y.len() + pixels.uv.len(),
                pixels.y.capacity() + pixels.uv.capacity(),
                "yuv420",
            ),
        };
        if let Some((_, true)) = self.observe(
            (false, Arc::as_ptr(frame) as usize),
            format,
            payload,
            capacity,
            json!({"owner":owner,"object":object,"source":source,"ptsUs":frame.pts}),
        ) {
            self._video.push(Arc::clone(frame));
        }
    }

    pub(crate) fn image(&mut self, image: &Arc<Pixels>, source: u64) -> Option<String> {
        let (index, new) = self.observe(
            (true, Arc::as_ptr(image) as usize),
            "rgba8",
            image.rgba.len(),
            image.rgba.capacity(),
            json!({"owner":"original_source_image","source":source}),
        )?;
        if new {
            self._images.push(Arc::clone(image));
        }
        Some(self.resources[index].id.clone())
    }

    pub(crate) fn finish(self, scope: &'static str) -> PixelMemorySnapshot {
        PixelMemorySnapshot {
            known_payload_bytes: self.resources.iter().map(|r| r.payload_bytes as u64).sum(),
            known_capacity_bytes: self.resources.iter().map(|r| r.capacity_bytes as u64).sum(),
            details: json!({"version":1,"scope":scope,
                "resourceCount":self.resources.len(),"resources":self.resources,
                "unobservedReferences":self.unobserved_references,
                "allObservedAllocationsKnown":self.unobserved_references==0,
                "identityScope":"one_snapshot","atomicAcrossCaches":false,
                "completeCoverage":false,"unknownBytes":null,
                "excludes":["in_flight_image_worker_and_completion_pixels","retiring_worker_caches",
                    "in_flight_decoder_output","external_frame_and_image_readers",
                    "decoder_internal_allocations","Arc_and_container_overhead"]}),
        }
    }
}

#[cfg(test)]
mod tests;
