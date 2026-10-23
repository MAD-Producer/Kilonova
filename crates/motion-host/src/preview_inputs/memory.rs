use super::*;
use crate::resource_memory::PixelCacheCollector;
use serde_json::{json, Value};

impl PreviewInputReader {
    /// Observe retained pixels only. In particular, do not call poll(), current()
    /// or request(): those consume completions, inspect files or advance state.
    pub(crate) fn collect_cached_pixels(
        &self,
        collector: &mut PixelCacheCollector,
    ) -> Result<Value> {
        let video = self
            .video
            .collect_cache_memory(collector, "original_source_video")?;
        let image = self.cached.as_ref().map(|(key, pixels)| {
            let resource = collector.image(pixels, key.source.id);
            json!({"source":key.source.id,"width":pixels.width,"height":pixels.height,
                "resource":resource})
        });
        Ok(json!({"videoStreams":video,"cachedImage":image,
            "imageDecode":{"pending":self.image.busy(),"payloadBytes":null,"capacityBytes":null,
                "scope":"worker_and_completion_unobserved"}}))
    }
}
