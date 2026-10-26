//! Image execution IR. Authoring documents and per-frame GPU allocations have
//! separate ownership. Pointwise declarations never constitute a fusion proof.
use crate::image_contract::{ImageContract, ImageFormatError, ImageMeaning};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod validation;
pub use validation::ExecutionPlanError;

#[cfg(test)]
mod tests;

pub const EXECUTION_PLAN_VERSION: u32 = 1;
pub const MAX_EXECUTION_NODES: usize = 4096;
pub const MAX_EXECUTION_EDGES: usize = 16384;
pub const MAX_NODE_INPUTS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionCapability {
    ImageDagV1,
    PointwiseGroupsV1,
    History,
    SpatialProducers,
    StatefulExecution,
    SideEffects,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExecutionNodeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImageValueId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationOrigin {
    pub composition: String,
    pub layer: u64,
    pub effect: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PixelRect {
    pub origin: [i32; 2],
    pub size: [u32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CoordinateDomain {
    Layer {
        layer: u64,
    },
    Composition,
    /// Texel coordinates of an imported resource, independent of any layer ROI.
    Resource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepthConvention {
    RelativeNearZeroFarOne,
    RelativeNearOneFarZero,
    PositiveSceneDistance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageValueType {
    pub image: ImageContract,
    pub rect: PixelRect,
    pub coordinates: CoordinateDomain,
    pub depth: Option<DepthConvention>,
}

#[derive(Debug, thiserror::Error)]
pub enum ValueTypeError {
    #[error("phase=value_contract: {0}")]
    Image(#[from] ImageFormatError),
    #[error(
        "phase=value_contract: pixel rectangle must have positive size and signed coordinates without overflow"
    )]
    Rectangle,
    #[error(
        "phase=value_contract: depth convention {depth:?} disagrees with image meaning {meaning:?}"
    )]
    Depth {
        meaning: ImageMeaning,
        depth: Option<DepthConvention>,
    },
}
impl ImageValueType {
    pub fn validate(self) -> Result<(), ValueTypeError> {
        // This validates logical meaning/precision/version only. Actual GPU
        // format and usage support are negotiated by the consuming renderer.
        self.image.preferred_storage()?;
        for axis in 0..2 {
            let end = i64::from(self.rect.origin[axis]) + i64::from(self.rect.size[axis]);
            if self.rect.size[axis] == 0 || i32::try_from(end).is_err() {
                return Err(ValueTypeError::Rectangle);
            }
        }
        let valid = match self.image.meaning {
            ImageMeaning::RelativeDepth {} => matches!(
                self.depth,
                Some(
                    DepthConvention::RelativeNearZeroFarOne
                        | DepthConvention::RelativeNearOneFarZero
                )
            ),
            ImageMeaning::LinearDepth {} => {
                self.depth == Some(DepthConvention::PositiveSceneDistance)
            }
            _ => self.depth.is_none(),
        };
        if !valid {
            return Err(ValueTypeError::Depth {
                meaning: self.image.meaning,
                depth: self.depth,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ImagePortRole {
    Primary,
    OriginalSource,
    Resource { index: u16 },
    Matte,
    Backdrop,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageInputPort {
    pub role: ImagePortRole,
    pub value: ImageValueId,
    pub expected: ImageValueType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimeDependency {
    Independent,
    CurrentSample,
    History { samples: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionDomain {
    Source,
    Image,
    Conversion,
    Composition,
    Materialization,
    Spatial,
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationSemantics {
    pub domain: ExecutionDomain,
    pub time: TimeDependency,
    /// Left, top, right, bottom in the declared pixel coordinate domain.
    /// None means unknown, never a zero-radius proof.
    pub halo: Option<[u32; 4]>,
    pub stateful: bool,
    pub side_effects: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationCost {
    pub shader_bytes: u64,
    pub static_loop_work: u64,
    /// None means unproved, never zero. An admission policy may require a bound.
    pub samples_per_fragment: Option<u64>,
    pub passes: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionNode {
    pub id: ExecutionNodeId,
    pub origin: OperationOrigin,
    pub inputs: Vec<ImageInputPort>,
    pub output: ImageValueId,
    pub output_type: ImageValueType,
    pub semantics: OperationSemantics,
    pub cost: OperationCost,
    /// Actual code/static-specialization digest, excluding sampled numbers and
    /// origin IDs. The owning compiler derives it from resolved implementations.
    pub code_identity: [u8; 32],
    pub uniform_cells: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPlan {
    pub version: u32,
    pub required_capabilities: BTreeSet<ExecutionCapability>,
    pub nodes: Vec<ExecutionNode>,
    /// Explicit terminal consumers keep values live after their producer.
    pub outputs: Vec<ImageValueId>,
}

/// These are logical preparation limits supplied by the caller. Physical GPU
/// memory admission and actual negotiated storage remain separate requirements.
/// Default application admission budgets for one checked graph. These bound
/// aggregate compiled text and conservative static work, not elapsed GPU time,
/// texture filtering taps, total covered pixels or physical memory residency.
pub const DEFAULT_EXECUTION_SHADER_BYTES: u64 = 64 * 1024 * 1024;
pub const DEFAULT_EXECUTION_LOOP_WORK: u64 = 1024 * 1024;
pub const DEFAULT_EXECUTION_SAMPLE_READS: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct ExecutionLimits {
    pub nodes: usize,
    pub edges: usize,
    pub passes: u64,
    pub shader_bytes: u64,
    pub static_loop_work: u64,
    pub samples_per_fragment: u64,
    pub require_sample_bounds: bool,
    pub uniform_cells_per_node: u16,
    pub resource_inputs_per_node: u16,
    pub rectangle_side: u32,
    pub halo_pixels: u32,
    pub history_samples: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ValueLifetime {
    pub value: ImageValueId,
    pub producer_step: u32,
    pub last_consumer_step: u32,
    pub consumer_count: u32,
    pub terminal: bool,
}

/// Produced only by validation. These are logical execution ordinals, not
/// physical texture slots or a claim that one node is one GPU pass.
#[derive(Clone, Debug, Serialize)]
pub struct CheckedExecutionOrder {
    order: Vec<ExecutionNodeId>,
    lifetimes: Vec<ValueLifetime>,
    total_cost: OperationCost,
}
impl CheckedExecutionOrder {
    pub fn order(&self) -> &[ExecutionNodeId] {
        &self.order
    }
    pub fn lifetimes(&self) -> &[ValueLifetime] {
        &self.lifetimes
    }
    pub fn total_cost(&self) -> OperationCost {
        self.total_cost
    }
}
