//! Sampled parameter and image-input ownership before CPU raster preparation.
//! This dependency projection has no fabricated image rectangles or precision:
//! the complete typed image IR still checks the accepted native proposals.
use super::*;
use motion_effects::execution_plan::{ExecutionNodeId, ExecutionPlanError, MAX_EXECUTION_NODES};
use motion_model::{EffectImageInput, EffectImageStage};
use std::collections::BTreeSet;
mod semantics;
pub(crate) use semantics::DefinitionSemantics;

#[derive(Clone, Copy, Debug)]
pub(super) enum PreparedImage {
    None,
    Empty,
    Asset {
        slot: usize,
    },
    Layer {
        draw: usize,
        stage: EffectImageStage,
        color: [f32; 4],
        rasterized: bool,
    },
}

#[derive(Debug)]
pub(super) struct PreparedEffect {
    params: [[f32; 4]; 32],
    image: PreparedImage,
    consumes_image: bool,
    semantics: Arc<DefinitionSemantics>,
}

impl PreparedEffect {
    pub(super) fn native_uniforms(&self) -> ([[f32; 4]; 32], i32) {
        let mut params = self.params;
        let resource = match self.image {
            PreparedImage::None => 0,
            PreparedImage::Empty => {
                params[30][0] = 2.;
                0
            }
            PreparedImage::Asset { slot } => {
                params[30][0] = 3.;
                -(slot as i32) - 1
            }
            PreparedImage::Layer {
                draw,
                stage,
                color,
                rasterized,
            } => {
                if stage == EffectImageStage::Effects {
                    params[30] = [1., 1., 0., 1.];
                    params[31] = [1.; 4];
                } else {
                    params[30] = [1., 0., 0., if rasterized { 1. } else { 0. }];
                    params[31] = color;
                }
                draw as i32 + 1
            }
        };
        (params, resource)
    }
}

pub(super) struct PreparationPlan {
    pub effects_by_layer: Vec<Vec<usize>>,
    pub inputs: Vec<Option<Result<PreparedEffect, RenderError>>>,
}

pub(super) fn validate_scene(scene: &Scene) -> Result<(), RenderError> {
    if scene.layers.len() > motion_model::MAX_LAYERS
        || scene.effects.len() > motion_model::MAX_LAYERS * motion_effects::MAX_EFFECTS_PER_LAYER
    {
        return Err("phase=effect_ir_preparation: sampled scene exceeds operation bounds".into());
    }
    if scene.composition_id.is_empty()
        || scene.composition_id.len() > 128
        || scene.composition_id.trim().is_empty()
        || scene.composition_id.chars().any(char::is_control)
    {
        return Err(
            RenderError::ExecutionPlan(ExecutionPlanError::InvalidOrigin {
                node: ExecutionNodeId(0),
            })
            .with_diagnostic("phase=effect_ir_preparation: invalid composition identity".into()),
        );
    }
    Ok(())
}

impl PlanBuilder {
    pub(super) fn prepare_effect_inputs(
        &self,
        scene: &Scene,
        assets: &[u64],
    ) -> Result<PreparationPlan, RenderError> {
        let mut plan = PreparationPlan {
            effects_by_layer: vec![Vec::new(); scene.layers.len()],
            inputs: (0..scene.effects.len()).map(|_| None).collect(),
        };
        for (index, effect) in scene.effects.iter().enumerate().filter(|(_, e)| e.enabled) {
            let Some(draw) = scene.layers.iter().position(|l| l.id == effect.layer) else {
                continue;
            };
            plan.effects_by_layer[draw].push(index);
            plan.inputs[index] = Some(self.prepare_effect_input(scene, assets, draw, index));
        }
        plan.check_dependencies(scene)?;
        Ok(plan)
    }

    fn prepare_effect_input(
        &self,
        scene: &Scene,
        assets: &[u64],
        draw: usize,
        index: usize,
    ) -> Result<PreparedEffect, RenderError> {
        let effect = &scene.effects[index];
        let resolved = self.resolved[index]
            .as_ref()
            .ok_or("resolved effect is absent")?;
        if let Some(error) = &resolved.error {
            return Err(RenderError::Shared(error.clone()));
        }
        for id in &resolved.programs {
            if let Some(error) = self.program_errors.get(id) {
                return Err(RenderError::Invalid(error.clone()));
            }
        }
        let def = &resolved.definition;
        if scene.layers[draw].adjustment && def.renderer != motion_effects::RendererKind::Image {
            return Err("adjustment layers support image effects only".into());
        }
        let mut params = [[0.; 4]; 32];
        for (i, p) in def.params.iter().enumerate() {
            let value = effect.values[resolved.mapping[i]];
            if !p.kind.valid_value(&value, p.min, p.max) {
                return Err(format!("parameter {} exceeds plugin range", p.id).into());
            }
            if !p.implemented && value != p.default {
                return Err(format!("AE parameter {} is not implemented", p.id).into());
            }
            params[i] = value;
        }
        match (&effect.shader_data, &def.shader_data) {
            (Some(values), Some(data)) => {
                values.write_uniforms(data, def.params.len(), &mut params)?
            }
            (None, Some(data)) => data.write_uniforms(def.params.len(), &mut params)?,
            (Some(_), None) => {
                return Err("instance shader data requires a declared code-package layout".into())
            }
            (None, None) => {}
        }
        let image = match effect.image_input {
            None => PreparedImage::None,
            Some(input) => {
                if !def.required_capabilities.iter().any(|c| c == "image_input") {
                    return Err("effect does not accept an image input".into());
                }
                match input {
                    EffectImageInput::Empty => PreparedImage::Empty,
                    EffectImageInput::Asset { asset } => PreparedImage::Asset {
                        slot: assets
                            .iter()
                            .position(|id| *id == asset)
                            .ok_or("effect image asset is not loaded")?,
                    },
                    EffectImageInput::Layer { layer, stage } => {
                        match scene.layers.iter().enumerate().find(|(_, l)| l.id == layer) {
                            Some((draw, layer)) => PreparedImage::Layer {
                                draw,
                                stage,
                                rasterized: layer.vector.is_some() || layer.composition,
                                color: [
                                    linear(layer.color[0]),
                                    linear(layer.color[1]),
                                    linear(layer.color[2]),
                                    layer.color[3],
                                ],
                            },
                            None => PreparedImage::Empty,
                        }
                    }
                }
            }
        };
        Ok(PreparedEffect {
            params,
            image,
            consumes_image: def.renderer == motion_effects::RendererKind::Image,
            semantics: resolved
                .semantics
                .clone()
                .ok_or("resolved effect semantics are absent")?,
        })
    }
}

impl PreparationPlan {
    fn check_dependencies(&self, scene: &Scene) -> Result<(), RenderError> {
        // Raw sources and ordinary image chains cannot form recursive source
        // dependencies. Keep that common case free of a second graph walk.
        if !self
            .inputs
            .iter()
            .filter_map(|p| p.as_ref())
            .filter_map(|p| p.as_ref().ok())
            .any(|p| {
                p.consumes_image
                    && matches!(
                        p.image,
                        PreparedImage::Layer {
                            stage: EffectImageStage::Effects,
                            ..
                        }
                    )
            })
        {
            return Ok(());
        }
        let mut dependencies = vec![Vec::new(); scene.layers.len()];
        let mut origins: Vec<_> = scene.layers.iter().map(|l| (l.id, None)).collect();
        let mut finals: Vec<_> = (0..scene.layers.len()).collect();
        let mut operations = Vec::new();
        for (draw, effects) in self.effects_by_layer.iter().enumerate() {
            for &index in effects {
                let Some(Ok(prepared)) = &self.inputs[index] else {
                    continue;
                };
                if !crate::scene::effect_has_output(&scene.effects[index]) {
                    continue;
                }
                let node = dependencies.len();
                dependencies.push(vec![finals[draw]]);
                origins.push((scene.layers[draw].id, Some(scene.effects[index].instance)));
                finals[draw] = node;
                if prepared.consumes_image {
                    operations.push((node, &prepared.image));
                }
            }
        }
        if dependencies.len() > MAX_EXECUTION_NODES {
            return Err("phase=effect_ir_preparation: operation budget exceeded".into());
        }
        for (node, image) in operations {
            if let PreparedImage::Layer { draw, stage, .. } = image {
                let source = if *stage == EffectImageStage::Effects {
                    finals[*draw]
                } else {
                    *draw
                };
                if !dependencies[node].contains(&source) {
                    dependencies[node].push(source);
                }
            }
        }
        let mut remaining: Vec<_> = dependencies.iter().map(Vec::len).collect();
        let mut consumers = vec![Vec::new(); dependencies.len()];
        let mut ready = BTreeSet::new();
        for (node, inputs) in dependencies.iter().enumerate() {
            if inputs.is_empty() {
                ready.insert(node);
            }
            for &input in inputs {
                consumers[input].push(node);
            }
        }
        let mut visited = 0;
        while let Some(node) = ready.pop_first() {
            visited += 1;
            for &consumer in &consumers[node] {
                remaining[consumer] -= 1;
                if remaining[consumer] == 0 {
                    ready.insert(consumer);
                }
            }
        }
        if visited == dependencies.len() {
            return Ok(());
        }
        // Report an actual cycle, excluding residual downstream tails. Both
        // traversal storage and diagnostic origins are bounded independently.
        let mut cursor = remaining
            .iter()
            .position(|n| *n != 0)
            .ok_or("dependency residual is absent")?;
        let mut path = Vec::new();
        let mut seen = vec![None; remaining.len()];
        loop {
            if let Some(start) = seen[cursor] {
                let cycle = &path[start..];
                let members: Vec<_> = cycle
                    .iter()
                    .take(16)
                    .map(|&id| ExecutionNodeId(id as u32))
                    .collect();
                let context: Vec<_> = cycle
                    .iter()
                    .take(8)
                    .map(|&id| {
                        let (layer, effect) = origins[id];
                        format!("layer={layer} effect={effect:?}")
                    })
                    .collect();
                let cause = ExecutionPlanError::Cycle {
                    members,
                    total: cycle.len(),
                };
                let context = format!(
                    "composition={} phase=effect_ir_preparation origins={context:?}: {cause}",
                    scene.composition_id
                );
                return Err(RenderError::ExecutionPlan(cause).with_diagnostic(context));
            }
            seen[cursor] = Some(path.len());
            path.push(cursor);
            cursor = dependencies[cursor]
                .iter()
                .copied()
                .find(|&p| remaining[p] != 0)
                .ok_or("dependency residual has no predecessor")?;
        }
    }
}

#[cfg(test)]
mod tests;
