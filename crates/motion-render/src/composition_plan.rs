//! Versioned, postorder GPU composition bundle. Child pixels stay on the GPU.
use crate::effect_plan::PlanBuilder;
use crate::{RenderError, Scene};

/// Composition-bundle failures with the offending reference or measured limit.
#[derive(Debug, thiserror::Error)]
pub enum CompositionPlanError {
    #[error("composition frame exceeds depth or active instance limit")]
    InstanceLimit { depth: usize, visited: usize },
    #[error("composition {composition}: sampled reference layer {layer} is missing")]
    MissingReference { composition: String, layer: u64 },
    #[error("composition frame bundle exceeds 32 MiB")]
    BundleSize {
        required_bytes: usize,
        budget_bytes: usize,
    },
    #[error("video asset missing")]
    MissingVideoAsset {
        composition: String,
        layer: u64,
        asset: u64,
    },
    #[error("{0}")]
    Encoding(#[source] Box<RenderError>),
}

pub const MAGIC: u32 = 0x4243534d;
pub const VERSION: u32 = 1;
pub const HEADER: usize = 32;
pub const NODE: usize = 80;
pub const VIDEO: usize = 40;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
struct Entry<'a> {
    scene: &'a Scene,
    reference: u64,
    parent: usize,
    slot: i32,
    previous_sibling: usize,
}
fn entries<'a>(
    s: &'a Scene,
    reference: u64,
    slot: i32,
    out: &mut Vec<Entry<'a>>,
    depth: usize,
    visited: &mut usize,
) -> Result<usize, RenderError> {
    if depth >= motion_model::composition::MAX_COMPOSITION_DEPTH
        || *visited >= motion_model::composition::MAX_RENDER_COMPOSITION_INSTANCES
    {
        return Err(CompositionPlanError::InstanceLimit {
            depth,
            visited: *visited,
        }
        .into());
    }
    *visited += 1;
    // Link immediate child roots in the existing postorder buffer. Each link is
    // visited once, without allocating a separate Vec for every parent.
    let mut last_child = usize::MAX;
    for nested in &s.nested {
        let layer = s
            .layers
            .iter()
            .find(|layer| layer.id == nested.layer)
            .ok_or_else(|| CompositionPlanError::MissingReference {
                composition: s.composition_id.clone(),
                layer: nested.layer,
            })?;
        let child = entries(
            &nested.scene,
            nested.layer,
            -(layer.order as i32 + 1),
            out,
            depth + 1,
            visited,
        )?;
        out[child].previous_sibling = last_child;
        last_child = child;
    }
    let index = out.len();
    out.push(Entry {
        scene: s,
        reference,
        parent: usize::MAX,
        slot,
        previous_sibling: usize::MAX,
    });
    while last_child != usize::MAX {
        out[last_child].parent = index;
        last_child = out[last_child].previous_sibling;
    }
    Ok(index)
}
fn word(out: &mut [u8], offset: usize, v: u32) {
    out[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
}
fn long(out: &mut [u8], offset: usize, v: u64) {
    out[offset..offset + 8].copy_from_slice(&v.to_le_bytes());
}
/// Write the existing version-1 bundle, retaining typed plan failures.
/// On error, callers must discard the output, which may contain a partial node.
pub fn build(
    builder: &mut PlanBuilder,
    scene: &Scene,
    project: &motion_model::Project,
    assets: &[u64],
    out: &mut Vec<u8>,
) -> Result<(), RenderError> {
    let mut nodes = Vec::new();
    out.clear();
    let root = entries(scene, 0, 0, &mut nodes, 0, &mut 0)?;
    out.resize(HEADER + nodes.len() * NODE, 0);
    for (i, e) in nodes.iter().enumerate() {
        let plan = builder
            .build(e.scene, assets, e.scene.width, e.scene.height, true)
            .map_err(|cause| {
                // The old bundle boundary displayed the raw plan diagnostic,
                // without RenderError's GPU validation prefix. Keep that text
                // while retaining the original error and resource refusal.
                let diagnostic = e.scene.diagnostic(&cause.planning_message());
                RenderError::Context {
                    message: diagnostic,
                    cause: Box::new(cause),
                }
            })?;
        let size = plan.buffer_bytes(e.scene);
        let start = out.len();
        let videos = e
            .scene
            .layers
            .iter()
            .filter(|layer| layer.video.is_some())
            .count();
        let required_bytes = start + size + videos * VIDEO;
        if required_bytes > MAX_BYTES {
            return Err(CompositionPlanError::BundleSize {
                required_bytes,
                budget_bytes: MAX_BYTES,
            }
            .into());
        }
        out.resize(required_bytes, 0);
        plan.write(e.scene, &mut out[start..start + size])
            .map_err(|cause| CompositionPlanError::Encoding(Box::new(cause)))?;
        let d = HEADER + i * NODE;
        long(out, d, e.reference);
        word(out, d + 8, e.parent as u32);
        word(out, d + 12, e.slot as u32);
        word(out, d + 16, e.scene.width);
        word(out, d + 20, e.scene.height);
        for c in 0..4 {
            word(out, d + 24 + c * 4, e.scene.background[c].to_bits());
        }
        word(out, d + 40, start as u32);
        word(out, d + 44, size as u32);
        word(out, d + 48, (start + size) as u32);
        word(out, d + 52, videos as u32);
        long(out, d + 56, e.scene.frame.to_bits());
        word(out, d + 64, e.scene.fps);
        for (j, (l, s)) in e
            .scene
            .layers
            .iter()
            .filter_map(|layer| layer.video.as_ref().map(|sample| (layer, sample)))
            .enumerate()
        {
            let v = start + size + j * VIDEO;
            long(out, v, l.id);
            long(out, v + 8, s.asset);
            long(out, v + 16, s.source_time_us);
            let a = project
                .video_assets
                .iter()
                .find(|a| a.id == s.asset)
                .ok_or_else(|| CompositionPlanError::MissingVideoAsset {
                    composition: e.scene.composition_id.clone(),
                    layer: l.id,
                    asset: s.asset,
                })?;
            word(out, v + 24, (-(l.order as i32 + 1)) as u32);
            word(out, v + 28, a.display_width);
            word(out, v + 32, a.display_height);
        }
    }
    word(out, 0, MAGIC);
    word(out, 4, VERSION);
    word(out, 8, nodes.len() as u32);
    let len = out.len();
    word(out, 12, len as u32);
    word(out, 16, root as u32);
    word(out, 20, HEADER as u32);
    word(out, 24, NODE as u32);
    word(out, 28, VIDEO as u32);
    Ok(())
}
