//! Effect-level image dependencies. Logical operations own neither a device nor
//! a texture. Their native pass ranges are lowering metadata, not node IDs.
use super::*;
use motion_effects::{
    execution_plan::*,
    image_contract::{ImageContract, ImageMeaning},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Weak,
};

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

#[derive(Clone, Copy)]
struct Code {
    digest: [u8; 32],
    bytes: u64,
    loops: u64,
}
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
    fn get(&mut self, id: u32, program: &EffectProgram) -> Code {
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
        let mut hash = Sha256::new();
        hash.update(b"motion-execution-code-v1");
        let mut bytes = 0_u64;
        for text in [&s.wgsl, &s.glsl.vertex, &s.glsl.fragment] {
            hash.update((text.len() as u64).to_le_bytes());
            hash.update(text.as_bytes());
            bytes = bytes.saturating_add(text.len() as u64);
        }
        for (tag, bindings) in [(0_u8, &s.glsl.blocks), (1, &s.glsl.textures)] {
            hash.update([tag]);
            hash.update((bindings.len() as u64).to_le_bytes());
            for (name, binding) in bindings {
                hash.update((name.len() as u64).to_le_bytes());
                hash.update(name.as_bytes());
                for word in binding {
                    hash.update(word.to_le_bytes());
                }
                bytes = bytes.saturating_add(name.len() as u64);
            }
        }
        hash.update([u8::from(s.sprite), u8::from(s.additive)]);
        hash.update(s.loop_work.to_le_bytes());
        let code = Code {
            digest: hash.finalize().into(),
            bytes,
            loops: s.loop_work,
        };
        self.hashes = self.hashes.saturating_add(1);
        self.hashed_bytes = self.hashed_bytes.saturating_add(bytes);
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
        draws: &[PlannedDraw],
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
            if range.start >= range.end
                || range.start < draw.pass_start
                || range.end > draw.pass_end
            {
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
        let mut cursors: Vec<_> = draws.iter().map(|d| d.pass_start).collect();
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
            .any(|(&end, draw)| end != draw.pass_end)
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
            lower(
                &self.frame,
                &self.pass_clocks,
                &self.programs,
                scene,
                targets,
                &mut self.code_identities,
                self.device_dimension,
            )
        });
        self.effect_execution = Some(result.map_err(|error| {
            let message = format!("composition={context} phase=effect_ir: {error}");
            error.with_diagnostic(message)
        })?);
        Ok(())
    }
}

fn image_type(
    precision: ChannelPrecision,
    layer: &crate::DrawLayer,
    size: [u32; 2],
) -> ImageValueType {
    ImageValueType {
        image: ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: AlphaMode::Premultiplied,
            },
            precision,
        ),
        rect: PixelRect {
            origin: [0; 2],
            size,
        },
        coordinates: if layer.adjustment || layer.screen_overlay {
            CoordinateDomain::Composition
        } else {
            CoordinateDomain::Layer { layer: layer.id }
        },
        depth: None,
    }
}

fn lower(
    frame: &EffectFramePlan,
    clocks: &[Option<usize>],
    programs: &[EffectProgram],
    scene: &Scene,
    targets: &[u64],
    codes: &mut CodeIdentities,
    dimension: u32,
) -> Result<EffectExecution, RenderError> {
    if frame.draws.len() > motion_model::MAX_LAYERS
        || scene.effects.len() > motion_model::MAX_LAYERS * motion_effects::MAX_EFFECTS_PER_LAYER
        || frame.draws.len() != scene.layers.len()
        || clocks.len() != frame.passes.len()
    {
        return Err("effect IR draft origins do not match the scene".into());
    }
    if scene.composition_id.is_empty()
        || scene.composition_id.len() > 128
        || scene.composition_id.trim().is_empty()
        || scene.composition_id.chars().any(char::is_control)
    {
        return Err(RenderError::ExecutionPlan(
            ExecutionPlanError::InvalidOrigin {
                node: ExecutionNodeId(0),
            },
        ));
    }
    let mut bindings = Vec::new();
    let mut types = Vec::new();
    let mut finals = Vec::new();
    // Canonical source imports are logical layer views. Conversion/raster work
    // is charged to the native operation that materializes that import. This
    // does not assert that an asset's stored bytes already use this precision.
    for (draw, layer) in frame.draws.iter().zip(&scene.layers) {
        let scale = frame
            .passes
            .get(draw.pass_start)
            .filter(|_| draw.pass_start < draw.pass_end)
            .map_or(1., |p| p.uniform.output_mode[3]);
        let region = if layer.adjustment {
            [0., 0., scene.width as f32, scene.height as f32]
        } else {
            layer.source_rect
        };
        let size = [
            (region[2] * scale).ceil().max(1.) as u32,
            (region[3] * scale).ceil().max(1.) as u32,
        ];
        let id = bindings.len();
        types.push(image_type(frame.precision, layer, size));
        bindings.push(EffectOperationBinding {
            node: ExecutionNodeId(id as u32),
            draw: id,
            scene_effect: None,
            original_passes: draw.pass_start..draw.pass_start,
            output_region: region,
            resource_tokens: vec![],
            lookup_tables: vec![],
            instance_data: vec![],
        });
        finals.push(id);
    }
    let mut effects = vec![None; scene.effects.len()];
    let mut primary = vec![None; bindings.len()];
    for (draw_index, draw) in frame.draws.iter().enumerate() {
        let mut at = draw.pass_start;
        while at < draw.pass_end {
            let effect = clocks[at];
            let mut end = at + 1;
            while end < draw.pass_end && clocks[end] == effect {
                end += 1;
            }
            if let Some(i) = effect {
                if scene.effects.get(i).is_none_or(|e| e.layer != draw.layer)
                    || effects[i].is_some()
                {
                    return Err(format!(
                        "layer={} phase=effect_ir: invalid or repeated effect origin",
                        draw.layer
                    )
                    .into());
                }
            }
            let last = &frame.passes[end - 1];
            if last.output != 0
                || (!matches!(last.program, 0 | 2)
                    && (last.uniform.output_mode[0] != 0. || last.uniform.output_mode[1] != 0.))
            {
                return Err(format!("layer={} phase=effect_ir: effect boundary must be canonical linear premultiplied", draw.layer).into());
            }
            let id = bindings.len();
            let mut binding = EffectOperationBinding {
                node: ExecutionNodeId(id as u32),
                draw: draw_index,
                scene_effect: effect,
                original_passes: at..end,
                output_region: last.uniform.region,
                resource_tokens: vec![],
                lookup_tables: vec![],
                instance_data: vec![],
            };
            for p in &frame.passes[at..end] {
                for (value, values, absent) in [
                    (p.resource_input, &mut binding.resource_tokens, 0),
                    (p.lut, &mut binding.lookup_tables, -1),
                ] {
                    if value != absent && !values.contains(&value) {
                        values.push(value);
                    }
                }
                if p.instance_data != 0 && !binding.instance_data.contains(&p.instance_data) {
                    binding.instance_data.push(p.instance_data);
                }
            }
            types.push(image_type(
                frame.precision,
                &scene.layers[draw_index],
                [last.width, last.height],
            ));
            bindings.push(binding);
            primary.push(Some(finals[draw_index]));
            finals[draw_index] = id;
            if let Some(i) = effect {
                effects[i] = Some(id);
            }
            at = end;
        }
    }
    if bindings.len() > MAX_EXECUTION_NODES {
        return Err("effect IR node budget exceeded".into());
    }
    let mut dependencies = vec![Vec::new(); frame.draws.len()];
    let mut roots = BTreeSet::new();
    let mut externally_read = vec![false; frame.draws.len()];
    let mut nodes = Vec::with_capacity(bindings.len());
    for (id, binding) in bindings.iter().enumerate() {
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
        let mut code = Sha256::new();
        code.update(b"motion-effect-operation-v1");
        let mut shader_bytes = 0_u64;
        let mut loops = 0_u64;
        for pass in &frame.passes[binding.original_passes.clone()] {
            let program = programs.get(pass.program as usize).ok_or_else(|| {
                RenderError::Invalid(format!(
                    "layer={} phase=effect_ir: absent program={}",
                    scene.layers[binding.draw].id, pass.program
                ))
            })?;
            let identity = codes.get(pass.program, program);
            code.update(identity.digest);
            shader_bytes = shader_bytes
                .checked_add(identity.bytes)
                .ok_or("effect IR shader cost overflow")?;
            loops = loops
                .checked_add(identity.loops)
                .ok_or("effect IR loop cost overflow")?;
            if pass.resource_input > 0 {
                let draw = (pass.resource_input - 1) as usize;
                if draw >= frame.draws.len() {
                    return Err("effect IR image input draw is absent".into());
                }
                let source = if pass.uniform.params[30][1] > 0.5 {
                    if frame.draws[draw].pass_start == frame.draws[draw].pass_end {
                        return Err("effect IR effected image input is not materialized".into());
                    }
                    externally_read[draw] = true;
                    roots.insert(draw);
                    if !dependencies[binding.draw].contains(&draw) {
                        dependencies[binding.draw].push(draw);
                    }
                    finals[draw]
                } else {
                    draw
                };
                if !resource_values.contains(&source) {
                    resource_values.push(source);
                }
            }
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
        let effect = binding.scene_effect.map(|i| &scene.effects[i]);
        nodes.push(ExecutionNode {
            id: binding.node,
            origin: OperationOrigin {
                composition: scene.composition_id.clone(),
                layer: scene.layers[binding.draw].id,
                effect: effect.map(|e| e.instance),
            },
            inputs,
            output: ImageValueId(id as u32),
            output_type: types[id],
            semantics: OperationSemantics {
                domain: if binding.original_passes.is_empty() {
                    ExecutionDomain::Source
                } else if effect.is_some_and(|e| e.scene.is_some()) {
                    ExecutionDomain::External
                } else if effect.is_some() {
                    ExecutionDomain::Image
                } else {
                    ExecutionDomain::Materialization
                },
                time: TimeDependency::CurrentSample,
                halo: None,
                stateful: false,
                side_effects: false,
            },
            cost: OperationCost {
                shader_bytes,
                static_loop_work: loops,
                samples_per_fragment: None,
                passes: binding.original_passes.len() as u32,
            },
            code_identity: code.finalize().into(),
            uniform_cells: motion_effects::MAX_PARAMS as u16,
        });
    }
    for c in &frame.compositing {
        if c.matte >= 0 {
            let draw = c.matte as usize;
            if draw >= frame.draws.len() {
                return Err("effect IR matte draw is absent".into());
            }
            roots.insert(draw);
            externally_read[draw] = true;
        }
    }
    for target in targets {
        let draw = frame
            .draws
            .iter()
            .position(|d| d.layer == *target)
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
            .filter(|(i, _)| frame.draws[*i].pass_start < frame.draws[*i].pass_end)
            .map(|(_, &id)| ImageValueId(id as u32))
            .collect(),
    };
    let checked = plan
        .check(
            &native_execution_capabilities(),
            native_effect_limits(dimension),
        )
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
    let mut selected = vec![false; frame.draws.len()];
    let mut pending: Vec<_> = roots.into_iter().collect();
    while let Some(draw) = pending.pop() {
        if selected[draw] {
            continue;
        }
        if frame.draws[draw].pass_start == frame.draws[draw].pass_end {
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
    let mapping: Vec<_> = (0..frame.passes.len()).map(|i| i..i + 1).collect();
    execution.lower_tasks(&mapping, &frame.draws)?;
    Ok(execution)
}

#[cfg(test)]
mod tests;
