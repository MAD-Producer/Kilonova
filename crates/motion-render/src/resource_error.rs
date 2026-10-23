//! Structured policy refusals, distinct from malformed input and GPU failures.
use serde::Serialize;

/// Logical policy scope. CPU decode buffers and native GPU allowances are
/// independent; none of these scopes measures physical/resident GPU memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourceBudgetScope {
    RenderAllocations,
    SourceTextures,
    ActiveImages,
    DecodedImages,
    VectorSources,
    VectorWorkingSet,
    MaskOutputs,
    EffectScratch,
    MaskAndEffectScratch,
    LayerCompositing,
    PreviewTarget,
    ExposureTargets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ResourceRejection {
    Budget {
        scope: ResourceBudgetScope,
        required_bytes: u64,
        budget_bytes: u64,
    },
    TextureDimensions {
        width: u32,
        height: u32,
        max_dimension: u32,
    },
}
