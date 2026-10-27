//! Small project-owned node resource references. Payloads never enter snapshots.
use crate::{ensure, EffectInstance, Project, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const NODE_SOURCE_VERSION: u32 = 2;
pub const MAX_NODE_RESOURCE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_NODE_IMAGE_BYTES: u64 = 15 * 1024 * 1024;
pub const MAX_NODE_PROJECT_RESOURCES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeResource {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
impl NodeResource {
    pub fn validate(&self, extension: &str) -> Result<()> {
        ensure(
            self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
                && self.path == format!("assets/node-{}.{}", self.sha256, extension)
                && self.bytes > 0
                && self.bytes <= MAX_NODE_RESOURCE_BYTES,
            "invalid project-owned node resource",
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeImageReference {
    pub width: u32,
    pub height: u32,
    pub resource: NodeResource,
}

/// Also used by presets and layer clipboard closure collection. No file IO or
/// PNG/base64 decode occurs while validating frequently cloned project state.
pub fn source_resources(source: &serde_json::Value) -> Result<Vec<NodeResource>> {
    ensure(
        source["format_version"] == NODE_SOURCE_VERSION
            && source["key"]
                .as_str()
                .is_some_and(|key| !key.trim().is_empty() && key.len() <= 128)
            && source["document"].is_object()
            && source["options"].is_object(),
        "invalid editable node effect source",
    )?;
    let mut resources = Vec::new();
    if let Some(images) = source["document"].get("images") {
        let images = images
            .as_object()
            .ok_or_else(|| crate::Error::Invalid("invalid node image index".into()))?;
        ensure(images.len() <= 32, "too many node images")?;
        let mut encoded = 0u64;
        let mut decoded = 0u64;
        for (id, image) in images {
            let image: NodeImageReference = serde_json::from_value(image.clone())?;
            image.resource.validate("png")?;
            ensure(id == &image.resource.sha256, "node image identity mismatch")?;
            ensure(image.width > 0 && image.height > 0, "empty node image")?;
            encoded += image.resource.bytes;
            let size = u64::from(image.width)
                .checked_mul(u64::from(image.height))
                .and_then(|n| n.checked_mul(4))
                .ok_or_else(|| crate::Error::Invalid("node image size overflow".into()))?;
            decoded = decoded
                .checked_add(size)
                .ok_or_else(|| crate::Error::Invalid("node image size overflow".into()))?;
            ensure(
                encoded <= MAX_NODE_IMAGE_BYTES && decoded <= 128 * 1024 * 1024,
                "node image budget exceeded",
            )?;
            resources.push(image.resource);
        }
    }
    if let Some(runtime) = source.get("runtime") {
        let runtime: NodeResource = serde_json::from_value(runtime.clone())?;
        runtime.validate("msfx")?;
        resources.push(runtime);
    }
    Ok(resources)
}

pub fn effect_resources<'a>(
    effects: impl IntoIterator<Item = &'a EffectInstance>,
) -> Result<Vec<NodeResource>> {
    let mut resources = BTreeMap::new();
    for effect in effects {
        if let Some(source) = &effect.node_graph {
            for resource in source_resources(source)? {
                if let Some(previous) = resources.insert(resource.path.clone(), resource.clone()) {
                    ensure(previous == resource, "conflicting node resource metadata")?;
                }
                ensure(
                    resources.len() <= MAX_NODE_PROJECT_RESOURCES,
                    "too many project-owned node resources",
                )?;
            }
        }
    }
    Ok(resources.into_values().collect())
}
impl Project {
    pub fn node_resources(&self) -> Result<Vec<NodeResource>> {
        effect_resources(
            self.layers
                .iter()
                .chain(self.compositions.iter().flat_map(|c| &c.layers))
                .flat_map(|l| &l.effects),
        )
    }
}
