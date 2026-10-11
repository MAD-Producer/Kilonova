use serde::Serialize;

/// Disjoint logical capacities of renderer-owned textures, not driver VRAM.
/// Views and bind groups do not own another copy. Buffers, upload staging,
/// caller-owned targets, swapchain and decoder allocations are outside this scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureMemoryBreakdown {
    pub static_images: u64,
    pub video_rgba: u64,
    pub video_planes: u64,
    pub nested_compositions: u64,
    pub vector_sources: u64,
    pub effect_scratch: u64,
    pub plugin_resources: u64,
    pub curve_luts: u64,
    pub vector_raster_scratch: u64,
    pub vector_group_scratch: u64,
    pub layer_compositing_scratch: u64,
    pub mask_scratch: u64,
    pub mask_outputs: u64,
    pub effect_input_snapshots: u64,
    pub matte_blend_scratch: u64,
    pub system_textures: u64,
}
impl TextureMemoryBreakdown {
    pub fn total_bytes(&self) -> u64 {
        self.static_images
            + self.video_rgba
            + self.video_planes
            + self.nested_compositions
            + self.vector_sources
            + self.effect_scratch
            + self.plugin_resources
            + self.curve_luts
            + self.vector_raster_scratch
            + self.vector_group_scratch
            + self.layer_compositing_scratch
            + self.mask_scratch
            + self.mask_outputs
            + self.effect_input_snapshots
            + self.matte_blend_scratch
            + self.system_textures
    }
}
