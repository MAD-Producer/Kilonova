//! Diagnostic corruption of actual lowering inputs. No fabricated error returns.
//! Absent from release builds; each injection is consumed by one real IR check.
use motion_effects::execution_plan::{ExecutionLimits, ExecutionPlan};
use motion_effects::image_contract::ChannelPrecision;

#[derive(Clone, Copy, Debug)]
pub enum ExecutionDiagnosticFault {
    Cycle,
    TypeMismatch,
    PassBudget,
}

pub(super) fn apply(
    fault: Option<ExecutionDiagnosticFault>,
    mut plan: ExecutionPlan,
    mut limits: ExecutionLimits,
) -> (ExecutionPlan, ExecutionLimits) {
    let Some((fault, index)) = fault.zip(
        plan.nodes
            .iter()
            .position(|node| node.origin.effect.is_some() && !node.inputs.is_empty()),
    ) else {
        return (plan, limits);
    };
    let node = &mut plan.nodes[index];
    match fault {
        ExecutionDiagnosticFault::Cycle => {
            node.inputs[0].value = node.output;
            node.inputs[0].expected = node.output_type;
        }
        ExecutionDiagnosticFault::TypeMismatch => {
            let image = &mut node.inputs[0].expected.image;
            image.precision = if image.precision == ChannelPrecision::Float16 {
                ChannelPrecision::Unorm8
            } else {
                ChannelPrecision::Float16
            };
        }
        ExecutionDiagnosticFault::PassBudget => {
            limits.passes = plan.nodes[..index]
                .iter()
                .map(|node| u64::from(node.cost.passes))
                .sum();
        }
    }
    (plan, limits)
}

impl super::PlanBuilder {
    pub(crate) fn inject_execution_fault(&mut self, fault: ExecutionDiagnosticFault) {
        self.preview_inputs = None;
        self.execution_fault = Some(fault);
    }
}
