//! Bounded pointwise IR. Eligibility is a proof of a particular implementation,
//! not a promise made by a package name or an author-provided metadata flag.
use crate::{
    AlphaMode, BoundsExpr, EdgeMode, EffectDefinition, EffectPackage, Error, MAX_PARAMS,
    ParamDefinition, RendererKind, Result, WorkingSpace, builtin,
    image_contract::{ChannelPrecision, ImageMeaning, ImageStorage},
    shader::CompiledShader,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

mod codegen;
pub use codegen::{FusedShader, PortableFusionMetrics};

pub const POINTWISE_ABI_VERSION: u32 = 1;
pub const MAX_POINTWISE_STAGES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PointwiseOp {
    Invert,
    Tint,
}
impl PointwiseOp {
    pub fn id(self) -> &'static str {
        match self {
            Self::Invert => "invert",
            Self::Tint => "tint",
        }
    }
    fn source_sha256(self) -> &'static str {
        match self {
            Self::Invert => "c32103f24378d4e51918a4eda25b3029ccfcac1d864cd212d71bfb61ad08d448",
            Self::Tint => "e39bf4925adc80ad84102c595d06d3ca22997ff13c4e364a2e3aae19d86c335d",
        }
    }
    fn parameter_ids(self) -> &'static [&'static str] {
        match self {
            Self::Invert => &["channel", "blend", "effect_opacity"],
            Self::Tint => &["effect_opacity", "p0001", "p0002", "p0003"],
        }
    }
}

/// Expected optimization boundaries are not render failures or warning sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainReason {
    UnknownOperation,
    UnsupportedSdk,
    DefinitionChanged,
    SourceChanged,
    CompiledImplementationChanged,
    UnsupportedBoundary,
    SingleStage,
    StageLimit,
    ParameterLimit,
    DuplicateNamespace,
}

#[derive(Clone, Debug)]
pub struct PointwiseProof {
    op: PointwiseOp,
    edge_mode: EdgeMode,
    parameters: Vec<ParamDefinition>,
}
impl PointwiseProof {
    pub fn op(&self) -> PointwiseOp {
        self.op
    }
    pub fn parameters(&self) -> &[ParamDefinition] {
        &self.parameters
    }
    pub fn edge_mode(&self) -> EdgeMode {
        self.edge_mode
    }
    /// Also check the native program actually selected by a host. Public host
    /// program tables must not turn a package proof into a proof of other code.
    pub fn matches_compiled(&self, actual: &CompiledShader) -> Result<bool> {
        let reference = builtin::package()?;
        Ok(reference
            .shaders
            .get(&(self.op.id().to_owned(), 0))
            .is_some_and(|expected| same_compilation(actual, expected)))
    }
}
#[derive(Clone, Debug)]
pub enum Eligibility {
    Proven(PointwiseProof),
    Retain(RetainReason),
}

fn same_compilation(a: &CompiledShader, b: &CompiledShader) -> bool {
    a.wgsl == b.wgsl
        && a.glsl.vertex == b.glsl.vertex
        && a.glsl.fragment == b.glsl.fragment
        && a.glsl.blocks == b.glsl.blocks
        && a.glsl.textures == b.glsl.textures
        && a.sprite == b.sprite
        && a.additive == b.additive
        && a.loop_work == b.loop_work
}

/// A changed built-in implementation must be proved again. Frozen source hashes
/// prevent a future reference-package update from silently blessing a stale IR.
/// Both compiled backends are compared; the public package hash is insufficient.
pub fn prove_builtin(
    package: &EffectPackage,
    definition: &EffectDefinition,
) -> Result<Eligibility> {
    use Eligibility::Retain;
    let op = match definition.id.as_str() {
        "invert" => PointwiseOp::Invert,
        "tint" => PointwiseOp::Tint,
        _ => return Ok(Retain(RetainReason::UnknownOperation)),
    };
    if package.manifest.sdk_version != 6 {
        return Ok(Retain(RetainReason::UnsupportedSdk));
    }
    let reference = builtin::package()?;
    let Some(expected) = reference.manifest.effects.iter().find(|e| e.id == op.id()) else {
        return Ok(Retain(RetainReason::DefinitionChanged));
    };
    if definition != expected {
        return Ok(Retain(RetainReason::DefinitionChanged));
    }
    let [pass] = definition.passes.as_slice() else {
        return Ok(Retain(RetainReason::UnsupportedBoundary));
    };
    if definition.renderer != RendererKind::Image
        || !definition.resources.is_empty()
        || definition.output_bounds.is_some()
        || definition.padding != BoundsExpr::default()
        || definition.working_space != WorkingSpace::Srgb
        || definition.alpha_mode != AlphaMode::Straight
        || !matches!(
            definition.edge_mode,
            EdgeMode::Transparent | EdgeMode::Clamp
        )
        || definition.edge_param.is_some()
        || definition.scene.is_some()
        || !definition
            .params
            .iter()
            .map(|p| p.id.as_str())
            .eq(op.parameter_ids().iter().copied())
    {
        return Ok(Retain(RetainReason::UnsupportedBoundary));
    }
    let Some(bytes) = package.files.get(&pass.shader) else {
        return Ok(Retain(RetainReason::SourceChanged));
    };
    if bytes.len() > 256 * 1024 {
        return Ok(Retain(RetainReason::SourceChanged));
    }
    let Ok(source) = std::str::from_utf8(bytes) else {
        return Ok(Retain(RetainReason::SourceChanged));
    };
    let hash = format!(
        "{:x}",
        Sha256::digest(source.replace("\r\n", "\n").as_bytes())
    );
    if hash != op.source_sha256() {
        return Ok(Retain(RetainReason::SourceChanged));
    }
    let key = (definition.id.clone(), 0);
    match (package.shaders.get(&key), reference.shaders.get(&key)) {
        (Some(actual), Some(expected)) if same_compilation(actual, expected) => {}
        _ => return Ok(Retain(RetainReason::CompiledImplementationChanged)),
    }
    Ok(Eligibility::Proven(PointwiseProof {
        op,
        edge_mode: definition.edge_mode,
        parameters: definition.params.clone(),
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct StageNamespace {
    pub object: u64,
    pub effect: u64,
}

#[derive(Clone, Debug)]
pub struct PointwiseStage {
    proof: PointwiseProof,
    namespace: StageNamespace,
    values: Vec<[f32; 4]>,
}
impl PointwiseStage {
    pub fn new(
        proof: PointwiseProof,
        namespace: StageNamespace,
        values: &[[f32; 4]],
    ) -> Result<Self> {
        if values.len() != proof.parameters.len() {
            return Err(Error::Invalid(format!(
                "pointwise parameter count: object={} effect={} expected={} actual={}",
                namespace.object,
                namespace.effect,
                proof.parameters.len(),
                values.len()
            )));
        }
        for (parameter, value) in proof.parameters.iter().zip(values) {
            if !parameter
                .kind
                .valid_value(value, parameter.min, parameter.max)
            {
                return Err(Error::Invalid(format!(
                    "pointwise parameter outside declared domain: object={} effect={} param={}",
                    namespace.object, namespace.effect, parameter.id
                )));
            }
        }
        Ok(Self {
            proof,
            namespace,
            values: values.to_vec(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct UniformSlot(u8);
impl UniformSlot {
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ParameterBinding {
    pub namespace: StageNamespace,
    pub parameter: String,
    pub slot: UniformSlot,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StageInstruction {
    pub namespace: StageNamespace,
    pub op: PointwiseOp,
    pub edge_mode: EdgeMode,
    pub parameter_slots: std::ops::Range<usize>,
    pub opacity_slot: UniformSlot,
}

/// These are sampled-value contracts and physical quantization boundaries.
/// No numeric data texture can enter this color-only IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct BoundaryContract {
    pub meaning: ImageMeaning,
    pub storage: ImageStorage,
}
pub fn canonical_boundary(precision: ChannelPrecision) -> BoundaryContract {
    BoundaryContract {
        meaning: ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        storage: match precision {
            ChannelPrecision::Unorm8 => ImageStorage::Rgba8UnormSrgb,
            ChannelPrecision::Unorm16 => ImageStorage::Rgba16Unorm,
            ChannelPrecision::Float16 => ImageStorage::Rgba16Float,
            ChannelPrecision::Float32 => ImageStorage::Rgba32Float,
        },
    }
}
pub fn working_boundary(precision: ChannelPrecision) -> BoundaryContract {
    let storage = match precision {
        ChannelPrecision::Unorm8 => ImageStorage::Rgba8Unorm,
        other => canonical_boundary(other).storage,
    };
    BoundaryContract {
        meaning: ImageMeaning::Color {
            space: WorkingSpace::Srgb,
            alpha: AlphaMode::Straight,
        },
        storage,
    }
}

#[derive(Clone, Debug)]
pub struct PointwiseChain {
    precision: ChannelPrecision,
    instructions: Vec<StageInstruction>,
    bindings: Vec<ParameterBinding>,
    uniforms: [[f32; 4]; MAX_PARAMS],
}
impl PointwiseChain {
    pub fn new(
        precision: ChannelPrecision,
        stages: &[PointwiseStage],
    ) -> std::result::Result<Self, RetainReason> {
        if stages.len() < 2 {
            return Err(RetainReason::SingleStage);
        }
        if stages.len() > MAX_POINTWISE_STAGES {
            return Err(RetainReason::StageLimit);
        }
        let cells: usize = stages.iter().map(|s| s.values.len()).sum();
        if cells > MAX_PARAMS {
            return Err(RetainReason::ParameterLimit);
        }
        let mut namespaces = BTreeSet::new();
        let mut instructions = Vec::with_capacity(stages.len());
        let mut bindings = Vec::with_capacity(cells);
        let mut uniforms = [[0.; 4]; MAX_PARAMS];
        for stage in stages {
            if !namespaces.insert(stage.namespace) {
                return Err(RetainReason::DuplicateNamespace);
            }
            let base = bindings.len();
            let mut opacity_slot = None;
            for (parameter, value) in stage.proof.parameters.iter().zip(&stage.values) {
                let slot = UniformSlot(bindings.len() as u8);
                uniforms[slot.index()] = *value;
                if parameter.id == "effect_opacity" {
                    opacity_slot = Some(slot);
                }
                bindings.push(ParameterBinding {
                    namespace: stage.namespace,
                    parameter: parameter.id.clone(),
                    slot,
                });
            }
            // The proof requires this exact finite schema; malformed public input
            // never reaches this branch through an unvalidated package name.
            let Some(opacity_slot) = opacity_slot else {
                return Err(RetainReason::UnsupportedBoundary);
            };
            instructions.push(StageInstruction {
                namespace: stage.namespace,
                op: stage.proof.op,
                edge_mode: stage.proof.edge_mode,
                parameter_slots: base..bindings.len(),
                opacity_slot,
            });
        }
        Ok(Self {
            precision,
            instructions,
            bindings,
            uniforms,
        })
    }
    pub fn precision(&self) -> ChannelPrecision {
        self.precision
    }
    pub fn instructions(&self) -> &[StageInstruction] {
        &self.instructions
    }
    pub fn bindings(&self) -> &[ParameterBinding] {
        &self.bindings
    }
    pub fn uniforms(&self) -> &[[f32; 4]; MAX_PARAMS] {
        &self.uniforms
    }
    /// Numeric values and instance identities do not specialize generated code.
    pub fn structure_key(&self) -> String {
        let mut key = format!(
            "pointwise-v{POINTWISE_ABI_VERSION}-{}",
            self.precision as u32
        );
        for instruction in &self.instructions {
            key.push('-');
            key.push_str(instruction.op.id());
        }
        key
    }
}

#[cfg(test)]
mod tests;
