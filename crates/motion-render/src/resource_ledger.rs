//! Application-owned texture identities. This is not driver residency accounting.
use serde::Serialize;
use std::{collections::BTreeMap, ops::Deref, sync::atomic::{AtomicU64, Ordering}};

static NEXT_TEXTURE_ID: AtomicU64 = AtomicU64::new(1);

/// Clones keep the allocation identity; replacing an allocation gets a new ID.
/// This wrapper adds no extra texture handle and does not delay destruction.
#[derive(Clone)]
pub(crate) struct OwnedTexture {
    texture: wgpu::Texture,
    id: u64,
}
impl Deref for OwnedTexture {
    type Target = wgpu::Texture;
    fn deref(&self) -> &Self::Target { &self.texture }
}
pub(crate) trait DeviceTextureExt {
    fn create_owned_texture(&self, descriptor: &wgpu::TextureDescriptor<'_>) -> OwnedTexture;
}
impl DeviceTextureExt for wgpu::Device {
    fn create_owned_texture(&self, descriptor: &wgpu::TextureDescriptor<'_>) -> OwnedTexture {
        OwnedTexture {
            texture: self.create_texture(descriptor),
            id: NEXT_TEXTURE_ID.fetch_add(1, Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextureCategory {
    StaticImages, VideoRgba, VideoPlanes, NestedCompositions, VectorSources,
    EffectScratch, PluginResources, CurveLuts, VectorRasterScratch,
    VectorGroupScratch, LayerCompositingScratch, MaskScratch, MaskOutputs,
    EffectInputSnapshots, MatteBlendScratch, SystemTextures, PreviewTargets,
    AuthoringInputs, CaptureTargets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextureState { Retained, PreviewFront, PreviewCandidate }

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureReference {
    pub category: TextureCategory,
    pub owner: String,
    pub state: TextureState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureResource {
    /// Process-local identity, encoded as a string for JavaScript consumers.
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub depth_or_array_layers: u32,
    pub mip_levels: u32,
    pub samples: u32,
    pub format: String,
    /// Uncompressed/block capacity including mips and MSAA, without driver padding.
    pub logical_bytes: Option<u64>,
    /// Multiple application handles referring to this allocation count only once.
    pub references: Vec<TextureReference>,
}

/// Metadata-only snapshot: it never retains a GPU texture, view or bind group.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureResourceLedger {
    resources: BTreeMap<u64, TextureResource>,
}
impl TextureResourceLedger {
    pub fn resources(&self) -> impl Iterator<Item = &TextureResource> { self.resources.values() }
    pub fn total_known_bytes(&self) -> u64 {
        self.resources().filter_map(|r| r.logical_bytes).sum()
    }
    pub fn unknown_capacity_count(&self) -> usize {
        self.resources().filter(|r| r.logical_bytes.is_none()).count()
    }
    /// Merge metadata from independently owned targets/previews, deduplicating IDs.
    pub fn merge(&mut self, other: Self) {
        for (id, resource) in other.resources {
            if let Some(existing) = self.resources.get_mut(&id) {
                for reference in resource.references {
                    if !existing.references.contains(&reference) { existing.references.push(reference); }
                }
                existing.references.sort_by(|a,b| a.owner.cmp(&b.owner));
            } else {
                self.resources.insert(id, resource);
            }
        }
    }
    pub(crate) fn add(&mut self, texture: &OwnedTexture, category: TextureCategory, owner: String, state: TextureState) {
        let resource = self.resources.entry(texture.id).or_insert_with(|| TextureResource {
            id: format!("texture-{}", texture.id),
            width: texture.width(), height: texture.height(),
            depth_or_array_layers: texture.depth_or_array_layers(),
            mip_levels: texture.mip_level_count(), samples: texture.sample_count(),
            format: format!("{:?}", texture.format()),
            logical_bytes: logical_bytes(texture), references: Vec::new(),
        });
        let reference = TextureReference { category, owner, state };
        if !resource.references.contains(&reference) { resource.references.push(reference); }
        resource.references.sort_by(|a,b| a.owner.cmp(&b.owner));
    }
    pub(crate) fn retained(&mut self, texture: &OwnedTexture, category: TextureCategory, owner: String) {
        self.add(texture, category, owner, TextureState::Retained);
    }
}

fn logical_bytes(texture: &wgpu::Texture) -> Option<u64> {
    let format = texture.format();
    let block_bytes = u64::from(format.block_copy_size(None)?);
    let (bw, bh) = format.block_dimensions();
    let mut total = 0u64;
    for level in 0..texture.mip_level_count() {
        let size = texture.size().mip_level_size(level, texture.dimension());
        let bytes = u64::from(size.width.div_ceil(bw))
            .checked_mul(u64::from(size.height.div_ceil(bh)))?
            .checked_mul(u64::from(size.depth_or_array_layers))?
            .checked_mul(block_bytes)?.checked_mul(u64::from(texture.sample_count()))?;
        total = total.checked_add(bytes)?;
    }
    Some(total)
}
