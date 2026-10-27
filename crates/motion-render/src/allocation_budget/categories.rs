//! Immutable allocation purpose. Aliases and queue retention keep the same bucket.
use crate::TextureCategory;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum AllocationCategory {
    StaticImages,
    Video,
    Vector,
    Effects,
    Masks,
    Compositing,
    PreviewTargets,
    CaptureTargets,
    ExposureTargets,
    AuthoringInputs,
    PluginResources,
    SystemTextures,
    UniformBuffers,
    GeometryBuffers,
    StorageBuffers,
    ReadbackBuffers,
    UploadBuffers,
    TimingBuffers,
    OtherBuffers,
    UiAtlas,
    ModelSources,
}
pub(super) const CATEGORIES: [AllocationCategory; 21] = [
    AllocationCategory::StaticImages,
    AllocationCategory::Video,
    AllocationCategory::Vector,
    AllocationCategory::Effects,
    AllocationCategory::Masks,
    AllocationCategory::Compositing,
    AllocationCategory::PreviewTargets,
    AllocationCategory::CaptureTargets,
    AllocationCategory::ExposureTargets,
    AllocationCategory::AuthoringInputs,
    AllocationCategory::PluginResources,
    AllocationCategory::SystemTextures,
    AllocationCategory::UniformBuffers,
    AllocationCategory::GeometryBuffers,
    AllocationCategory::StorageBuffers,
    AllocationCategory::ReadbackBuffers,
    AllocationCategory::UploadBuffers,
    AllocationCategory::TimingBuffers,
    AllocationCategory::OtherBuffers,
    AllocationCategory::UiAtlas,
    AllocationCategory::ModelSources,
];
pub(super) const CATEGORY_COUNT: usize = CATEGORIES.len();

impl AllocationCategory {
    pub(crate) fn buffer(usage: wgpu::BufferUsages) -> Self {
        if usage.contains(wgpu::BufferUsages::MAP_READ) {
            Self::ReadbackBuffers
        } else if usage.contains(wgpu::BufferUsages::MAP_WRITE) {
            Self::UploadBuffers
        } else if usage.contains(wgpu::BufferUsages::QUERY_RESOLVE) {
            Self::TimingBuffers
        } else if usage.intersects(wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::INDEX) {
            Self::GeometryBuffers
        } else if usage.contains(wgpu::BufferUsages::STORAGE) {
            Self::StorageBuffers
        } else if usage.contains(wgpu::BufferUsages::UNIFORM) {
            Self::UniformBuffers
        } else {
            Self::OtherBuffers
        }
    }
}
impl From<TextureCategory> for AllocationCategory {
    fn from(value: TextureCategory) -> Self {
        match value {
            TextureCategory::StaticImages => Self::StaticImages,
            TextureCategory::ModelSources => Self::ModelSources,
            TextureCategory::VideoRgba | TextureCategory::VideoPlanes => Self::Video,
            TextureCategory::VectorSources
            | TextureCategory::VectorRasterScratch
            | TextureCategory::VectorGroupScratch => Self::Vector,
            TextureCategory::EffectScratch
            | TextureCategory::CurveLuts
            | TextureCategory::EffectInputSnapshots => Self::Effects,
            TextureCategory::MaskScratch | TextureCategory::MaskOutputs => Self::Masks,
            TextureCategory::NestedCompositions
            | TextureCategory::LayerCompositingScratch
            | TextureCategory::MatteBlendScratch => Self::Compositing,
            TextureCategory::PreviewTargets => Self::PreviewTargets,
            TextureCategory::CaptureTargets => Self::CaptureTargets,
            TextureCategory::ExposureTargets => Self::ExposureTargets,
            TextureCategory::AuthoringInputs => Self::AuthoringInputs,
            TextureCategory::PluginResources => Self::PluginResources,
            TextureCategory::SystemTextures => Self::SystemTextures,
            TextureCategory::UiAtlas => Self::UiAtlas,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationCategoryInfo {
    pub category: AllocationCategory,
    pub admitted_bytes: u64,
    /// Independent high-water mark; different categories need not peak together.
    pub peak_admitted_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationCategoryFrameInfo {
    pub category: AllocationCategory,
    pub start_admitted_bytes: u64,
    pub peak_admitted_bytes: u64,
    /// None until CPU scope and all registered queue callbacks have ended.
    pub end_admitted_bytes: Option<u64>,
}

#[cfg(test)]
mod tests;
