//! Conservative fragment texture-read bounds from validated Naga IR.
//! Counts image loads and samples, including SDK helpers and repeated calls.
//! Both branch arms are counted. The portable-source loop product is applied
//! once to the expanded call graph, so this is an upper bound, not GPU timing.
use crate::{Error, Result};
use naga::{Expression, Function, Module, ShaderStage, Statement};

fn function_reads(function: &Function, callees: &[Option<u64>]) -> Option<u64> {
    let mut reads = function
        .expressions
        .iter()
        .filter(|(_, expression)| {
            matches!(
                expression,
                Expression::ImageSample { .. } | Expression::ImageLoad { .. }
            )
        })
        .count() as u64;
    let mut blocks = vec![&function.body];
    while let Some(block) = blocks.pop() {
        for statement in block.iter() {
            match statement {
                Statement::Block(body) => blocks.push(body),
                Statement::If { accept, reject, .. } => blocks.extend([accept, reject]),
                Statement::Switch { cases, .. } => blocks.extend(cases.iter().map(|c| &c.body)),
                Statement::Loop {
                    body, continuing, ..
                } => blocks.extend([body, continuing]),
                Statement::Call { function, .. } => {
                    let count = callees.get(function.index()).copied().flatten()?;
                    reads = reads.checked_add(count)?;
                }
                _ => {}
            }
        }
    }
    Some(reads)
}

pub(super) fn fragment_reads(module: &Module, loop_work: u64) -> Result<u64> {
    // Validated Naga functions appear before all their callers. Iteration is
    // bounded by the source/expression limits and avoids recursive traversal.
    let mut callees = Vec::with_capacity(module.functions.len());
    for (_, function) in module.functions.iter() {
        // Overflow in an unreachable helper is not a runtime cost. Propagate
        // unknown bounds only when the actual entry point reaches that helper.
        callees.push(function_reads(function, &callees));
    }
    let fragment = module
        .entry_points
        .iter()
        .find(|entry| entry.stage == ShaderStage::Fragment && entry.name == "sdk_fragment")
        .ok_or_else(|| {
            Error::Invalid("fragment texture-read bound has no SDK entry point".into())
        })?;
    function_reads(&fragment.function, &callees)
        .and_then(|n| n.checked_mul(loop_work))
        .ok_or_else(|| {
            Error::Invalid("fragment loop-expanded texture-read bound overflows u64".into())
        })
}
