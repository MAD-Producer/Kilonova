//! On-demand attachment diagnostics. Default rendering remains Clear/Store;
//! coverage is a correctness prerequisite, not a claim that Load is faster.
use super::{scratch, scratch_graph::Input, *};
use serde::Serialize;

pub(crate) fn color_attachment_operations() -> wgpu::Operations<wgpu::Color> {
    // Keep the measured default until both correctness and performance qualify
    // a replacement. This definition is also used by the native encode path.
    wgpu::Operations {
        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        store: wgpu::StoreOp::Store,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentClearReasons {
    pub partial_geometry: bool,
    pub fragment_discard: bool,
    pub unknown_shader_coverage: bool,
    pub destination_blending: bool,
    pub unused_capacity: bool,
}
impl AttachmentClearReasons {
    fn required(self) -> bool {
        self.partial_geometry || self.fragment_discard || self.unknown_shader_coverage
            || self.destination_blending || self.unused_capacity
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentConsumers {
    /// Distinct passes, rather than texture ports, reading this logical value.
    pub sampled_by_passes: u32,
    pub last_sampled_pass: Option<usize>,
    /// Existing atomic draw's snapshot copy or composition read. An exported
    /// value cannot be discarded just because no later effect pass reads it.
    pub snapshot_or_composition: u32,
    pub last_execution_point: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectAttachmentIntent {
    pub pass: usize,
    pub program: u32,
    pub logical_value: usize,
    pub allocation: u32,
    pub viewport: [u32; 2],
    pub capacity: [u32; 2],
    pub format: ScratchFormat,
    pub clear_reasons: AttachmentClearReasons,
    pub full_overwrite_candidate: bool,
    /// There is no inter-pass attachment Load in the current effect planner.
    /// Sprite blending reads its freshly cleared destination within the pass.
    pub preserves_previous_pass: bool,
    pub consumers: AttachmentConsumers,
    pub store_required: bool,
    /// These are the actual default native operations, not an optimization.
    pub clears_transparent: bool,
    pub stores: bool,
}

impl PlanBuilder {
    /// Inspect the completed native plan explicitly. This performs bounded
    /// planning work and allocations; it is never called by the Present path.
    /// The packed plan ABI and existing scratch placement are unchanged.
    pub fn attachment_intents(&self) -> Result<Vec<EffectAttachmentIntent>, RenderError> {
        let frame = &self.frame;
        if frame.scratch_bindings.len() != frame.passes.len() {
            return Err("attachment intents phase=bindings: pass/binding counts differ".into());
        }
        let graph = scratch::logical_graph(frame, None)?;
        let mut consumers = vec![AttachmentConsumers::default(); graph.resources.len()];
        for (index, pass) in graph.passes.iter().enumerate() {
            for (port, input) in [pass.input, pass.source].into_iter().enumerate() {
                if port == 1 && input == pass.input { continue; }
                if let Input::Resource(value) = input {
                    consumers[value].sampled_by_passes = consumers[value].sampled_by_passes.saturating_add(1);
                    consumers[value].last_sampled_pass = Some(index);
                }
            }
        }
        for &value in graph.draw_outputs.iter().flatten() {
            consumers[value].snapshot_or_composition = consumers[value].snapshot_or_composition.saturating_add(1);
        }
        let mut result = Vec::with_capacity(frame.passes.len());
        for (index, pass) in frame.passes.iter().enumerate() {
            let program = self.programs.get(pass.program as usize).ok_or_else(||
                RenderError::Invalid(format!("attachment intents pass={index} program={} phase=shader: missing program", pass.program)))?;
            let physical = frame.scratch_bindings[index];
            let allocation = frame.scratch_allocations.get(physical.output as usize).ok_or_else(||
                RenderError::Invalid(format!("attachment intents pass={index} allocation={} phase=capacity: missing descriptor", physical.output)))?;
            if pass.width == 0 || pass.height == 0 || pass.width > allocation.width || pass.height > allocation.height {
                return Err(format!("attachment intents pass={index} allocation={} phase=capacity: viewport {}x{} exceeds or empties {}x{}",
                    physical.output, pass.width, pass.height, allocation.width, allocation.height).into());
            }
            let reasons = AttachmentClearReasons {
                partial_geometry: pass.sprite || program.shader.sprite,
                fragment_discard: program.shader.may_discard == Some(true),
                unknown_shader_coverage: program.shader.may_discard.is_none(),
                destination_blending: pass.sprite || program.shader.sprite || program.shader.additive,
                unused_capacity: pass.width != allocation.width || pass.height != allocation.height,
            };
            let value = graph.passes[index].output;
            let mut uses = consumers[value];
            uses.last_execution_point = graph.resources[value].last_use;
            let operations = color_attachment_operations();
            result.push(EffectAttachmentIntent {
                pass: index, program: pass.program, logical_value: value, allocation: physical.output,
                viewport: [pass.width, pass.height], capacity: [allocation.width, allocation.height],
                format: allocation.class.format, clear_reasons: reasons,
                full_overwrite_candidate: !reasons.required(), preserves_previous_pass: false,
                consumers: uses, store_required: uses.sampled_by_passes != 0 || uses.snapshot_or_composition != 0,
                clears_transparent: matches!(operations.load, wgpu::LoadOp::Clear(color) if color == wgpu::Color::TRANSPARENT),
                stores: operations.store == wgpu::StoreOp::Store,
            });
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
