//! Effect-level image dependencies. Logical operations own neither a device nor
//! a texture. Their native pass ranges are lowering metadata, not node IDs.
use super::*;
use motion_effects::execution_plan::*;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Weak,
};

mod admission;
use admission::NativeAnnotations;

const MAX_CODE_IDENTITIES: usize = 1024;

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct ExecutionCodeCacheInfo {
    /// Lifetime work for this PlanBuilder; registry resets clear entries only.
    pub hashes: u64,
    /// WGSL/GLSL and reflection-name UTF-8 bytes, excluding binary hash framing.
    pub hashed_text_bytes: u64,
    pub entries: usize,
    pub entry_limit: usize,
}

type Code = shader::CompiledCodeIdentity;
struct CodeEntry {
    shader: Weak<shader::CompiledShader>,
    code: Code,
    used: u64,
}
#[derive(Default)]
pub(super) struct CodeIdentities {
    entries: BTreeMap<u32, CodeEntry>,
    clock: u64,
    hashes: u64,
    hashed_bytes: u64,
}
impl CodeIdentities {
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub(super) fn get(&mut self, id: u32, program: &EffectProgram) -> Code {
        self.clock = self.clock.saturating_add(1);
        if let Some(entry) = self.entries.get_mut(&id) {
            if entry
                .shader
                .upgrade()
                .is_some_and(|s| Arc::ptr_eq(&s, &program.shader))
            {
                entry.used = self.clock;
                return entry.code;
            }
        }
        let s = &program.shader;
        let code = shader::compiled_code_identity(s);
        self.hashes = self.hashes.saturating_add(1);
        self.hashed_bytes = self.hashed_bytes.saturating_add(code.text_bytes);
        if !self.entries.contains_key(&id) && self.entries.len() == MAX_CODE_IDENTITIES {
            if let Some((&oldest, _)) = self.entries.iter().min_by_key(|(_, e)| e.used) {
                self.entries.remove(&oldest);
            }
        }
        // Weak ownership bounds metadata without extending a shader's lifetime.
        // A public Arc mutation/replacement invalidates the pointer proof.
        self.entries.insert(
            id,
            CodeEntry {
                shader: Arc::downgrade(s),
                code,
                used: self.clock,
            },
        );
        code
    }
    fn info(&self) -> ExecutionCodeCacheInfo {
        ExecutionCodeCacheInfo {
            hashes: self.hashes,
            hashed_text_bytes: self.hashed_bytes,
            entries: self.entries.len(),
            entry_limit: MAX_CODE_IDENTITIES,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct EffectOperationBinding {
    pub node: ExecutionNodeId,
    pub draw: usize,
    pub scene_effect: Option<usize>,
    /// Unfused native drafts. Group lowering may replace several such ranges.
    pub original_passes: Range<usize>,
    /// Exact layer/composition-space view, separate from the integer texel grid.
    pub output_region: [f32; 4],
    /// Existing native imports; their binding admission remains authoritative.
    pub resource_tokens: Vec<i32>,
    pub lookup_tables: Vec<i32>,
    pub instance_data: Vec<u32>,
    /// Imported numeric channels retain their resource-local texel contract.
    pub imported_image: Option<ImportedImage>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct EffectNativeTask {
    pub members: Vec<ExecutionNodeId>,
    pub draw: usize,
    pub passes: Range<usize>,
}

#[derive(Debug)]
pub struct EffectExecution {
    plan: ExecutionPlan,
    checked: CheckedExecutionOrder,
    bindings: Vec<EffectOperationBinding>,
    effects: Vec<Option<usize>>,
    consumers: Vec<Vec<usize>>,
    externally_read: Vec<bool>,
    snapshots: Vec<usize>,
    tasks: Vec<EffectNativeTask>,
    pass_order: Vec<Vec<usize>>,
}
impl EffectExecution {
    pub fn plan(&self) -> &ExecutionPlan {
        &self.plan
    }
    pub fn checked(&self) -> &CheckedExecutionOrder {
        &self.checked
    }
    pub fn bindings(&self) -> &[EffectOperationBinding] {
        &self.bindings
    }
    pub(crate) fn accepts_package_import(&self, import: &PackageImageImport) -> bool {
        self.bindings.iter().any(|binding| {
            matches!(binding.imported_image.as_ref(),
            Some(ImportedImage::Package(value)) if value == import)
        })
    }
    pub fn frame_import(&self, source: FrameImageSource) -> Option<&FrameImageImport> {
        self.bindings
            .iter()
            .find_map(|binding| match binding.imported_image.as_ref() {
                Some(ImportedImage::Frame(value)) if value.source() == source => Some(value),
                _ => None,
            })
    }
    pub fn snapshot_order(&self) -> &[usize] {
        &self.snapshots
    }
    pub fn native_tasks(&self) -> &[EffectNativeTask] {
        &self.tasks
    }
    pub(super) fn externally_read(&self) -> &[bool] {
        &self.externally_read
    }

    /// Each removed value must be local to this group. Multiple ports of one
    /// consumer count once. This supplements, never replaces, shader proofs.
    pub(super) fn allows_group(&self, effects: &[usize]) -> bool {
        let members: Option<Vec<_>> = effects
            .iter()
            .map(|&i| self.effects.get(i).copied().flatten())
            .collect();
        let Some(members) = members.filter(|v| v.len() >= 2) else {
            return false;
        };
        for pair in members.windows(2) {
            let previous = &self.plan.nodes[pair[0]];
            let next = &self.plan.nodes[pair[1]];
            if previous.origin.layer != next.origin.layer
                || !next
                    .inputs
                    .iter()
                    .any(|p| p.role == ImagePortRole::Primary && p.value == previous.output)
            {
                return false;
            }
        }
        members[..members.len() - 1].iter().all(|&i| {
            !self.plan.outputs.contains(&self.plan.nodes[i].output)
                && self.consumers[i].iter().all(|c| members.contains(c))
        })
    }

    /// Map the unfused operations to actual tasks without consulting the
    /// truncated diagnostics report. A task may implement several IR nodes.
    pub(super) fn lower_tasks(
        &mut self,
        mapping: &[Range<usize>],
        draws: &[Range<usize>],
    ) -> Result<(), RenderError> {
        let mut tasks: Vec<EffectNativeTask> = Vec::new();
        let mut owners = vec![None; self.bindings.len()];
        for binding in &self.bindings {
            if binding.original_passes.is_empty() {
                continue;
            }
            let ranges = mapping
                .get(binding.original_passes.clone())
                .ok_or("effect IR task mapping is absent")?;
            let range = ranges.first().ok_or("effect IR task is empty")?.start
                ..ranges.last().ok_or("effect IR task is empty")?.end;
            let draw = &draws[binding.draw];
            if range.start >= range.end || range.start < draw.start || range.end > draw.end {
                return Err("effect IR task escapes its atomic draw".into());
            }
            let owner = if tasks
                .last()
                .is_some_and(|t| t.draw == binding.draw && range.start < t.passes.end)
            {
                let id = tasks.len() - 1;
                tasks[id].passes.end = tasks[id].passes.end.max(range.end);
                tasks[id].members.push(binding.node);
                id
            } else {
                let id = tasks.len();
                tasks.push(EffectNativeTask {
                    members: vec![binding.node],
                    draw: binding.draw,
                    passes: range,
                });
                id
            };
            owners[binding.node.0 as usize] = Some(owner);
        }
        let mut remaining: Vec<_> = tasks.iter().map(|t| t.members.len()).collect();
        let mut cursors: Vec<_> = draws.iter().map(|d| d.start).collect();
        let mut order = vec![Vec::new(); draws.len()];
        for id in self.checked.order() {
            let Some(task) = owners[id.0 as usize] else {
                continue;
            };
            remaining[task] -= 1;
            if remaining[task] != 0 {
                continue;
            }
            let task = &tasks[task];
            // Scratch lifetimes currently describe this exact atomic order.
            // Never silently reorder tasks against the admitted placement.
            if task.passes.start != cursors[task.draw] {
                return Err("effect IR task order disagrees with scratch lifetimes".into());
            }
            order[task.draw].extend(task.passes.clone());
            cursors[task.draw] = task.passes.end;
        }
        if cursors
            .iter()
            .zip(draws)
            .any(|(&end, draw)| end != draw.end)
        {
            return Err("effect IR native task coverage is incomplete".into());
        }
        self.tasks = tasks;
        self.pass_order = order;
        Ok(())
    }
}

pub fn native_effect_limits(max_dimension: u32) -> ExecutionLimits {
    let mut limits = native_snapshot_limits(max_dimension);
    limits.nodes = MAX_EXECUTION_NODES;
    limits.edges = MAX_EXECUTION_EDGES;
    // Source views may be virtual (for example a large constant-color plane),
    // rather than device textures. Concrete pass dimensions remain checked by
    // scratch admission and native allocation, before any GPU storage is used.
    limits.rectangle_side = i32::MAX as u32;
    limits
}

impl PlanBuilder {
    /// Semantic plan for the last successfully lowered frame, before fusion.
    /// Public frame drafts are mutable; this is a read-only planning receipt.
    pub fn effect_execution(&self) -> Option<&EffectExecution> {
        self.effect_execution.as_ref()
    }
    pub fn execution_code_cache_info(&self) -> ExecutionCodeCacheInfo {
        self.code_identities.info()
    }
    pub(crate) fn effect_snapshot_order(&self) -> &[usize] {
        self.effect_execution
            .as_ref()
            .map_or(&[], EffectExecution::snapshot_order)
    }
    pub(crate) fn effect_pass_order(&self, draw: usize) -> &[usize] {
        self.effect_execution
            .as_ref()
            .and_then(|e| e.pass_order.get(draw))
            .map_or(&[], Vec::as_slice)
    }
    pub(super) fn lower_effect_execution(
        &mut self,
        scene: &Scene,
        targets: &[u64],
    ) -> Result<(), RenderError> {
        self.effect_execution = None;
        if self.frame.passes.is_empty() {
            return Ok(());
        }
        // This validates every draft and def/use edge without physical placement.
        let context: String = scene
            .composition_id
            .chars()
            .filter(|c| !c.is_control())
            .take(128)
            .collect();
        let result = scratch::logical_graph(&self.frame, None).and_then(|_| {
            let semantic = self
                .semantic_frame
                .as_ref()
                .ok_or("semantic frame is absent before native lowering")?;
            let annotations = NativeAnnotations::admit(
                semantic,
                &self.frame,
                &self.pass_clocks,
                &self.programs,
                scene,
                &mut self.code_identities,
            )?;
            lower(
                semantic,
                annotations,
                targets,
                self.device_dimension,
                #[cfg(all(debug_assertions, feature = "gpu-tests"))]
                self.execution_fault.take(),
            )
        });
        self.effect_execution = Some(result.map_err(|error| {
            let message = format!("composition={context} phase=effect_ir: {error}");
            error.with_diagnostic(message)
        })?);
        Ok(())
    }
}

fn lower(
    semantic: &preparation::SemanticFrame,
    annotations: NativeAnnotations,
    targets: &[u64],
    dimension: u32,
    #[cfg(all(debug_assertions, feature = "gpu-tests"))] fault: Option<
        super::ExecutionDiagnosticFault,
    >,
) -> Result<EffectExecution, RenderError> {
    let NativeAnnotations {
        mut nodes,
        bindings,
        semantic_origins,
        mapping,
        effects,
        draw_ranges,
        pass_count,
    } = annotations;
    let types: Vec<_> = nodes.iter().map(|node| node.output_type).collect();
    let compact = |node| -> Result<usize, RenderError> {
        mapping[semantic.resolved(node)?].ok_or_else(|| "semantic producer was not lowered".into())
    };
    let finals: Vec<_> = semantic
        .layers
        .iter()
        .map(|l| compact(l.output))
        .collect::<Result<_, _>>()?;
    let primary: Vec<_> = semantic_origins
        .iter()
        .map(|&i| semantic.nodes[i].input.map(&compact).transpose())
        .collect::<Result<_, _>>()?;
    if bindings.len() > MAX_EXECUTION_NODES {
        return Err("effect IR node budget exceeded".into());
    }
    let mut dependencies = vec![Vec::new(); draw_ranges.len()];
    let mut roots = BTreeSet::new();
    let mut externally_read = vec![false; draw_ranges.len()];
    for (id, binding) in bindings.iter().take(semantic_origins.len()).enumerate() {
        let mut inputs = Vec::new();
        if let Some(source) = primary[id] {
            for role in [ImagePortRole::Primary, ImagePortRole::OriginalSource] {
                inputs.push(ImageInputPort {
                    role,
                    value: ImageValueId(source as u32),
                    expected: types[source],
                });
            }
        }
        let mut resource_values = Vec::new();
        // The sampled typed selector owns raw/effected identity. Uniform token
        // encoding is a backend detail and cannot redefine dependency edges.
        if let preparation::PreparedImage::Layer { draw, stage, .. } =
            semantic.nodes[semantic_origins[id]].image
        {
            let source = if stage == motion_model::EffectImageStage::Effects {
                if draw_ranges[draw].is_empty() {
                    return Err("effect IR effected image input is not materialized".into());
                }
                externally_read[draw] = true;
                roots.insert(draw);
                if !dependencies[binding.draw].contains(&draw) {
                    dependencies[binding.draw].push(draw);
                }
                finals[draw]
            } else {
                compact(semantic.layers[draw].source)?
            };
            resource_values.push(source);
        }
        for (index, source) in resource_values.into_iter().enumerate() {
            inputs.push(ImageInputPort {
                role: ImagePortRole::Resource {
                    index: index as u16,
                },
                value: ImageValueId(source as u32),
                expected: types[source],
            });
        }
        // Native import edges have already passed their own admission. The
        // primary and sampled-layer edges are owned only by the semantic frame.
        inputs.append(&mut nodes[id].inputs);
        nodes[id].inputs = inputs;
    }
    for layer in &semantic.layers {
        if let Some(draw) = layer.matte.and_then(|matte| matte.draw) {
            roots.insert(draw);
            externally_read[draw] = true;
        }
    }
    for target in targets {
        let draw = semantic
            .layers
            .iter()
            .position(|layer| semantic.nodes[layer.source].layer == *target)
            .ok_or("effect IR receiver is absent")?;
        roots.insert(draw);
    }
    let plan = ExecutionPlan {
        version: EXECUTION_PLAN_VERSION,
        required_capabilities: BTreeSet::from([ExecutionCapability::ImageDagV1]),
        nodes,
        outputs: finals
            .iter()
            .enumerate()
            .filter(|(i, _)| !draw_ranges[*i].is_empty())
            .map(|(_, &id)| ImageValueId(id as u32))
            .collect(),
    };
    let limits = native_effect_limits(dimension);
    #[cfg(all(debug_assertions, feature = "gpu-tests"))]
    let (plan, limits) = super::execution_fault::apply(fault, plan, limits);
    let checked = plan
        .check(&native_execution_capabilities(), limits)
        .map_err(|error| {
            let ids: Vec<_> = match &error {
                ExecutionPlanError::Cycle { members, .. } => {
                    members.iter().take(8).copied().collect()
                }
                ExecutionPlanError::TypeMismatch { node, .. }
                | ExecutionPlanError::Budget {
                    node: Some(node), ..
                }
                | ExecutionPlanError::UnknownCost { node, .. } => vec![*node],
                _ => vec![],
            };
            let origins: Vec<_> = ids
                .iter()
                .filter_map(|id| plan.nodes.get(id.0 as usize))
                .map(|n| format!("layer={} effect={:?}", n.origin.layer, n.origin.effect))
                .collect();
            let message = format!("phase=effect_ir_check origins={origins:?}: {error}");
            RenderError::ExecutionPlan(error).with_diagnostic(message)
        })?;
    let mut consumers = vec![Vec::new(); plan.nodes.len()];
    for (index, node) in plan.nodes.iter().enumerate() {
        for input in &node.inputs {
            let uses = &mut consumers[input.value.0 as usize];
            if !uses.contains(&index) {
                uses.push(index);
            }
        }
    }
    let mut selected = vec![false; draw_ranges.len()];
    let mut pending: Vec<_> = roots.into_iter().collect();
    while let Some(draw) = pending.pop() {
        if selected[draw] {
            continue;
        }
        if draw_ranges[draw].is_empty() {
            return Err("effect IR snapshot has no native output".into());
        }
        selected[draw] = true;
        pending.extend(dependencies[draw].iter().copied());
    }
    let snapshots = checked
        .order()
        .iter()
        .filter_map(|id| {
            let binding = &bindings[id.0 as usize];
            (selected[binding.draw] && finals[binding.draw] == id.0 as usize)
                .then_some(binding.draw)
        })
        .collect();
    let mut execution = EffectExecution {
        plan,
        checked,
        bindings,
        effects,
        consumers,
        externally_read,
        snapshots,
        tasks: vec![],
        pass_order: vec![],
    };
    let mapping: Vec<_> = (0..pass_count).map(|i| i..i + 1).collect();
    execution.lower_tasks(&mapping, &draw_ranges)?;
    Ok(execution)
}

#[cfg(test)]
mod tests;
