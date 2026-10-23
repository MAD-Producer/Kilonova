use super::*;
use crate::memory::{CpuBufferAllocation, CpuMemoryInfo};
use serde_json::json;
use std::collections::{HashMap, HashSet};

impl FontStore {
    /// Explicit cache metadata; does not rasterize, read files, touch LRU or
    /// retain glyphs. Parser-owned storage and evicted external Arcs are unknown.
    pub fn memory_info(&self) -> CpuMemoryInfo {
        let mut identities = HashMap::new();
        let mut resources: Vec<CpuBufferAllocation> = Vec::new();
        for ((font, character, size), glyph) in &self.glyphs {
            let index = *identities.entry(Arc::as_ptr(glyph)).or_insert_with(|| {
                resources.push(CpuBufferAllocation::vector(
                    format!("glyph-{}", resources.len() + 1),
                    "glyph_coverage",
                    &glyph.coverage,
                ));
                resources.len() - 1
            });
            resources[index]
                .references
                .push(json!({"font":font,"codePoint":*character as u32,
                "sizePx":f32::from_bits(*size)}));
        }
        let faces: HashSet<_> = self
            .faces
            .iter()
            .map(|(_, face)| Arc::as_ptr(face))
            .collect();
        CpuMemoryInfo::new(
            "font_store_cached_glyph_vectors",
            resources,
            json!({"version":1,"identityScope":"one_snapshot",
                "cachedGlyphReferences":self.glyphs.len(),"cachedFaceReferences":self.faces.len(),
                "uniqueFaces":faces.len(),"legacyBookedGlyphPayloadBytes":self.glyph_bytes,
                "faceCapacityBytes":if faces.is_empty(){Some(0u64)}else{None},
                "unknownBytes":null,"excludes":["font_parser_internal_storage",
                    "external_evicted_glyphs_and_rasters","allocator_Arc_and_container_overhead"]}),
        )
    }
}

#[cfg(test)]
mod tests;
