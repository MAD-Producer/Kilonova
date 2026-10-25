//! Lower a checked package DAG into the existing bounded native working slots.
use super::*;
use motion_effects::execution_plan::{ExecutionNodeId, ExecutionPlanError, ImageValueId};
use motion_effects::execution_template::{
    TemplateContext, TemplateError, image_template_capabilities, image_template_limits,
};
use std::collections::BTreeMap;

pub(super) struct RuntimePass {
    pub program: u32,
    pub pass: u16,
    pub input: i32,
    pub source: i32,
    pub input_import: bool,
    pub source_import: bool,
    pub output: u32,
}
pub(super) struct RuntimeSchedule {
    pub passes: Vec<RuntimePass>,
    pub slots: u32,
}

/// A graph's structural/code/capacity refusal cannot turn into a different
/// image by dropping the operation. Ordinary non-template preview failures
/// retain their existing bypass policy.
pub(super) fn requires_refusal(error: &RenderError) -> bool {
    match error {
        RenderError::Effects(motion_effects::Error::ExecutionTemplate(_))
        | RenderError::ExecutionPlan(_) => true,
        RenderError::Context { cause, .. } | RenderError::Diagnostic { cause, .. } => {
            requires_refusal(cause)
        }
        RenderError::Shared(cause) => requires_refusal(cause),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn schedule(
    definition: &EffectDefinition,
    programs: &[u32],
    implementations: &[EffectProgram],
    codes: &mut effect_execution::CodeIdentities,
    composition: &str,
    layer: u64,
    effect: u64,
    precision: ChannelPrecision,
    input_size: [u32; 2],
    output_size: [u32; 2],
    work: u32,
    direct_output: bool,
) -> Result<Option<RuntimeSchedule>, RenderError> {
    let Some(template) = &definition.execution else {
        return Ok(None);
    };
    if composition.is_empty()
        || composition.len() > 128
        || composition.trim().is_empty()
        || composition.chars().any(char::is_control)
    {
        return Err(RenderError::ExecutionPlan(
            ExecutionPlanError::InvalidOrigin {
                node: ExecutionNodeId(0),
            },
        ));
    }
    let identities = programs
        .iter()
        .map(|&id| {
            implementations
                .get(id as usize)
                .map(|p| codes.get(id, p))
                .ok_or_else(|| {
                    RenderError::Invalid("phase=runtime_code: native program is absent".into())
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let checked = template
        .check(
            definition,
            &identities,
            &TemplateContext {
                origin: motion_effects::execution_plan::OperationOrigin {
                    composition: composition.into(),
                    layer,
                    effect: Some(effect),
                },
                precision,
                input_size,
                output_size,
            },
            &image_template_capabilities(),
            image_template_limits(),
        )
        .map_err(|cause| RenderError::from(motion_effects::Error::from(cause)))?;
    let nodes: BTreeMap<_, _> = checked.plan().nodes.iter().map(|n| (n.id, n)).collect();
    let last_use: BTreeMap<_, _> = checked
        .checked()
        .lifetimes()
        .iter()
        .map(|v| (v.value, v.last_consumer_step as usize))
        .collect();
    let mut values = BTreeMap::from([(ImageValueId(0), work)]);
    let mut passes = Vec::with_capacity(programs.len());
    let mut slots = 1 << work;
    for (ordinal, &id) in checked.checked().order().iter().enumerate() {
        let Some(pass) = checked.pass(id) else {
            continue;
        };
        let node = nodes[&id];
        let input = node.inputs[0].value;
        let source = node.inputs[1].value;
        let terminal = checked.plan().outputs.contains(&node.output);
        let output = if terminal && direct_output {
            0
        } else {
            (work..work + 3)
                .find(|slot| !values.values().any(|v| v == slot))
                .ok_or_else(|| {
                    RenderError::from(motion_effects::Error::from(TemplateError::Storage {
                        node: id,
                        required: values.len() as u64 + 1,
                        maximum: 3,
                    }))
                })?
        };
        // Inputs stay live until this pass completes; never write a sampled
        // texture as an attachment in the same pass, even on its last use.
        passes.push(RuntimePass {
            program: programs[usize::from(pass)],
            pass,
            input: values[&input] as i32,
            source: values[&source] as i32,
            input_import: input == ImageValueId(0),
            source_import: source == ImageValueId(0),
            output,
        });
        values.retain(|value, _| last_use[value] > ordinal);
        values.insert(node.output, output);
        slots |= 1 << output;
    }
    Ok(Some(RuntimeSchedule { passes, slots }))
}
