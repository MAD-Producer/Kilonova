//! First native execution-IR consumer: atomic effected-layer snapshots.
//! Passes within a draw keep their existing scratch owner and allocation plan.
//! The checked typed graph, rather than recursive traversal, orders snapshots.
use super::{EffectFramePlan, EffectProgram, MAX_SCRATCH_PASSES, PlanBuilder};
use crate::{RenderError, Scene};
use motion_effects::{
    AlphaMode, WorkingSpace,
    execution_plan::*,
    image_contract::{ImageContract, ImageMeaning},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub struct SnapshotExecution {
    plan: Option<ExecutionPlan>,
    checked: Option<CheckedExecutionOrder>,
    draws: Vec<usize>,
}
impl SnapshotExecution {
    /// No graph or traversal allocation for an ordinary frame without readers.
    pub fn plan(&self) -> Option<&ExecutionPlan> {
        self.plan.as_ref()
    }
    pub fn checked(&self) -> Option<&CheckedExecutionOrder> {
        self.checked.as_ref()
    }
    pub fn draw_order(&self) -> &[usize] {
        &self.draws
    }
}

/// Records actual successful encode/copy calls, not predicted graph node counts.
/// At most MAX_LAYERS records for the latest local composition encode attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SnapshotEncode {
    pub layer: u64,
    pub encoded_passes: usize,
}

pub fn native_execution_capabilities() -> BTreeSet<ExecutionCapability> {
    BTreeSet::from([ExecutionCapability::ImageDagV1])
}

pub fn native_snapshot_limits(max_dimension: u32) -> ExecutionLimits {
    ExecutionLimits {
        nodes: motion_model::MAX_LAYERS,
        edges: motion_model::MAX_LAYERS * motion_effects::MAX_EFFECTS_PER_LAYER,
        passes: MAX_SCRATCH_PASSES as u64,
        // Original package/shader and physical-memory admission still applies.
        shader_bytes: u64::MAX,
        static_loop_work: u64::MAX,
        samples_per_fragment: u64::MAX,
        require_sample_bounds: false,
        uniform_cells_per_node: motion_effects::MAX_PARAMS as u16,
        resource_inputs_per_node: motion_effects::MAX_EFFECTS_PER_LAYER as u16,
        rectangle_side: max_dimension,
        halo_pixels: max_dimension,
        history_samples: 0,
    }
}

impl PlanBuilder {
    pub fn snapshot_execution(
        &self,
        scene: &Scene,
        targets: &[u64],
    ) -> Result<SnapshotExecution, RenderError> {
        if !needs_snapshots(&self.frame, targets) {
            return Ok(SnapshotExecution::default());
        }
        self.snapshot_execution_checked(
            scene,
            targets,
            &native_execution_capabilities(),
            native_snapshot_limits(self.device_dimension),
        )
    }

    /// Capability and logical-budget negotiation used by the native consumer.
    /// This does not allocate textures, compile shaders or authorize fusion.
    pub fn snapshot_execution_checked(
        &self,
        scene: &Scene,
        targets: &[u64],
        capabilities: &BTreeSet<ExecutionCapability>,
        limits: ExecutionLimits,
    ) -> Result<SnapshotExecution, RenderError> {
        lower(
            &self.frame,
            &self.programs,
            &scene.composition_id,
            targets,
            capabilities,
            limits,
        )
    }
}

fn effected_source(pass: &super::EffectPass) -> Option<usize> {
    (pass.resource_input > 0 && pass.uniform.params[30][1] > 0.5)
        .then(|| (pass.resource_input - 1) as usize)
}

fn needs_snapshots(frame: &EffectFramePlan, targets: &[u64]) -> bool {
    !targets.is_empty()
        || frame
            .passes
            .iter()
            .any(|pass| effected_source(pass).is_some())
        || frame.compositing.iter().any(|c| c.matte >= 0)
}

fn lower(
    frame: &EffectFramePlan,
    programs: &[EffectProgram],
    composition: &str,
    targets: &[u64],
    capabilities: &BTreeSet<ExecutionCapability>,
    limits: ExecutionLimits,
) -> Result<SnapshotExecution, RenderError> {
    if !needs_snapshots(frame, targets) {
        return Ok(SnapshotExecution::default());
    }
    let context: String = composition.chars().take(128).collect();
    if frame.draws.len() > motion_model::MAX_LAYERS || frame.passes.len() > MAX_SCRATCH_PASSES {
        return Err(format!(
            "composition={context} phase=snapshot_lowering: native draw/pass limits exceeded"
        )
        .into());
    }
    let mut dependencies = vec![Vec::new(); frame.draws.len()];
    let mut roots = BTreeSet::new();
    for (index, draw) in frame.draws.iter().enumerate() {
        let passes = frame.passes.get(draw.pass_start..draw.pass_end).ok_or_else(|| {
            RenderError::Invalid(format!("composition={context} layer={} phase=snapshot_lowering: invalid pass range {}..{}", draw.layer, draw.pass_start, draw.pass_end))
        })?;
        for pass in passes {
            if let Some(source) = effected_source(pass) {
                if source >= frame.draws.len() {
                    return Err(format!("composition={context} layer={} phase=snapshot_lowering: absent input draw={source}", draw.layer).into());
                }
                if !dependencies[index].contains(&source) {
                    dependencies[index].push(source);
                }
                roots.insert(source);
            }
        }
    }
    // A matte reads a raw effected snapshot. The matte operation itself is
    // applied later by composition and is not included in that source snapshot.
    for composite in &frame.compositing {
        if composite.matte >= 0 {
            let source = composite.matte as usize;
            if source >= frame.draws.len() {
                return Err(format!(
                    "composition={context} phase=snapshot_lowering: absent matte draw={source}"
                )
                .into());
            }
            roots.insert(source);
        }
    }
    for &layer in targets {
        let index = frame.draws.iter().position(|draw| draw.layer == layer)
            .ok_or_else(|| RenderError::Invalid(format!("composition={context} layer={layer} phase=snapshot_lowering: materialized receiver is absent")))?;
        roots.insert(index);
    }
    let mut selected = vec![false; frame.draws.len()];
    let mut pending: Vec<_> = roots.iter().copied().collect();
    while let Some(index) = pending.pop() {
        if selected[index] {
            continue;
        }
        selected[index] = true;
        pending.extend(dependencies[index].iter().copied());
    }
    let mut types = vec![None; frame.draws.len()];
    for (index, draw) in frame.draws.iter().enumerate().filter(|(i, _)| selected[*i]) {
        let final_pass = draw.pass_end.checked_sub(1)
            .filter(|_| draw.pass_start < draw.pass_end)
            .and_then(|last| frame.passes.get(last))
            .ok_or_else(|| RenderError::Invalid(format!("composition={context} layer={} phase=snapshot_lowering: required snapshot has no materialized output", draw.layer)))?;
        types[index] = Some(ImageValueType {
            image: ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Premultiplied,
                },
                frame.precision,
            ),
            rect: PixelRect {
                origin: [0; 2],
                size: [final_pass.width, final_pass.height],
            },
            coordinates: CoordinateDomain::Layer { layer: draw.layer },
            depth: None,
        });
    }
    let mut nodes = Vec::new();
    for (index, draw) in frame.draws.iter().enumerate().filter(|(i, _)| selected[*i]) {
        let mut code = Sha256::new();
        code.update(b"motion-atomic-snapshot-v1");
        let mut shader_bytes = 0_u64;
        let mut loop_work = 0_u64;
        for (offset, pass) in frame.passes[draw.pass_start..draw.pass_end]
            .iter()
            .enumerate()
        {
            let program = programs.get(pass.program as usize).ok_or_else(|| RenderError::Invalid(format!(
                "composition={context} layer={} pass={} phase=snapshot_lowering: absent program={}", draw.layer, draw.pass_start + offset, pass.program)))?;
            for text in [
                &program.shader.wgsl,
                &program.shader.glsl.vertex,
                &program.shader.glsl.fragment,
            ] {
                code.update((text.len() as u64).to_le_bytes());
                code.update(text.as_bytes());
                shader_bytes = shader_bytes.checked_add(text.len() as u64)
                    .ok_or_else(|| RenderError::Invalid(format!("composition={context} layer={} phase=snapshot_lowering: shader byte cost overflow",draw.layer)))?;
            }
            code.update([
                u8::from(program.shader.sprite),
                u8::from(program.shader.additive),
            ]);
            loop_work = loop_work.checked_add(program.shader.loop_work)
                .ok_or_else(|| RenderError::Invalid(format!("composition={context} layer={} phase=snapshot_lowering: loop metric overflow",draw.layer)))?;
        }
        let output_type = types[index].ok_or("snapshot output type is absent")?;
        nodes.push(ExecutionNode {
            id: ExecutionNodeId(index as u32),
            origin: OperationOrigin {
                composition: composition.into(),
                layer: draw.layer,
                effect: None,
            },
            inputs: dependencies[index]
                .iter()
                .enumerate()
                .map(|(slot, &source)| {
                    Ok(ImageInputPort {
                        role: ImagePortRole::Resource { index: slot as u16 },
                        value: ImageValueId(source as u32),
                        expected: types[source].ok_or("snapshot input type is absent")?,
                    })
                })
                .collect::<Result<_, RenderError>>()?,
            output: ImageValueId(index as u32),
            output_type,
            semantics: OperationSemantics {
                domain: ExecutionDomain::Materialization,
                time: TimeDependency::CurrentSample,
                halo: None,
                stateful: false,
                side_effects: false,
            },
            cost: OperationCost {
                shader_bytes,
                static_loop_work: loop_work,
                // The SDK loop metric is not a proved texture-sampling bound.
                samples_per_fragment: None,
                passes: (draw.pass_end - draw.pass_start) as u32,
            },
            code_identity: code.finalize().into(),
            uniform_cells: motion_effects::MAX_PARAMS as u16,
        });
    }
    let plan = ExecutionPlan {
        version: EXECUTION_PLAN_VERSION,
        required_capabilities: BTreeSet::from([ExecutionCapability::ImageDagV1]),
        nodes,
        // Snapshot copies outlive atomic scratch. These explicit terminal
        // consumers are separate from inter-snapshot edges and composition.
        outputs: roots.into_iter().map(|i| ImageValueId(i as u32)).collect(),
    };
    let checked = plan.check(capabilities, limits).map_err(|error| {
        let layers: Vec<_> = match &error {
            ExecutionPlanError::Cycle { members, .. } => members
                .iter()
                .filter_map(|id| frame.draws.get(id.0 as usize).map(|draw| draw.layer))
                .collect(),
            _ => vec![],
        };
        let message =
            format!("composition={context} phase=snapshot_dag layers={layers:?}: {error}");
        RenderError::ExecutionPlan(error).with_diagnostic(message)
    })?;
    let draws = checked.order().iter().map(|id| id.0 as usize).collect();
    Ok(SnapshotExecution {
        plan: Some(plan),
        checked: Some(checked),
        draws,
    })
}
