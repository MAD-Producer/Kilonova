//! Immutable effect rules are shared with the resolved code package. Sampled
//! values are bound separately; a region recipe is evaluated only against the
//! previous effect that physical admission actually accepted.
use super::*;
use motion_effects::{BoundsExpr, OutputBounds, ParamKind};
use std::collections::BTreeMap;

#[derive(Debug)]
enum RegionRule {
    Padding(BoundsExpr),
    Rectangle(OutputBounds),
}

#[derive(Debug)]
pub(crate) struct DefinitionSemantics {
    region: RegionRule,
    parameters: BTreeMap<String, usize>,
    edge_parameter: Option<(usize, bool)>,
    edge: f32,
    working: [f32; 2],
    opacity: Option<usize>,
}

impl DefinitionSemantics {
    pub(in crate::effect_plan) fn new(def: &EffectDefinition) -> Result<Self, RenderError> {
        let parameters: BTreeMap<_, _> = def
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();
        let edge_parameter = def
            .edge_param
            .as_ref()
            .map(|name| {
                let index = *parameters.get(name).ok_or("unknown edge parameter")?;
                Ok::<_, RenderError>((index, def.params[index].kind == ParamKind::Enum))
            })
            .transpose()?;
        Ok(Self {
            region: def.output_bounds.clone().map_or_else(
                || RegionRule::Padding(def.padding.clone()),
                RegionRule::Rectangle,
            ),
            edge_parameter,
            edge: match def.edge_mode {
                EdgeMode::Transparent => 0.,
                EdgeMode::Clamp => 1.,
                EdgeMode::Repeat => 2.,
                EdgeMode::Mirror => 3.,
            },
            working: [
                if def.working_space == WorkingSpace::Srgb {
                    1.
                } else {
                    0.
                },
                if def.alpha_mode == AlphaMode::Straight {
                    1.
                } else {
                    0.
                },
            ],
            opacity: parameters.get("effect_opacity").copied(),
            parameters,
        })
    }

    pub(in crate::effect_plan) fn output_region(
        &self,
        params: &[[f32; 4]; 32],
        input: [f32; 4],
    ) -> Result<[f32; 4], RenderError> {
        let lookup = |id: &str, component: usize| {
            self.parameters
                .get(id)
                .and_then(|&index| params.get(index))
                .and_then(|value| value.get(component))
                .copied()
        };
        match &self.region {
            RegionRule::Rectangle(rect) => {
                rect.evaluate_with(lookup, input).map_err(RenderError::from)
            }
            RegionRule::Padding(padding) => {
                let pad = padding
                    .evaluate_with(lookup, input)
                    .map_err(RenderError::from)?;
                if pad < 0. {
                    return Err("negative output padding".into());
                }
                Ok([
                    input[0] - pad,
                    input[1] - pad,
                    input[2] + 2. * pad,
                    input[3] + 2. * pad,
                ])
            }
        }
    }

    pub(in crate::effect_plan) fn direct_output(&self) -> bool {
        matches!(self.region, RegionRule::Rectangle(_))
    }
    pub(in crate::effect_plan) fn working(&self) -> [f32; 2] {
        self.working
    }
    pub(in crate::effect_plan) fn working_slot(&self) -> u32 {
        if self.working[0] == 1. {
            1
        } else {
            4
        }
    }
    pub(in crate::effect_plan) fn edge(&self, params: &[[f32; 4]; 32]) -> f32 {
        self.edge_parameter.map_or(self.edge, |(index, is_enum)| {
            if is_enum {
                params[index][0]
            } else {
                if params[index][0] > 0.5 {
                    1.
                } else {
                    0.
                }
            }
        })
    }
    pub(in crate::effect_plan) fn opacity(&self, params: &[[f32; 4]; 32]) -> f32 {
        self.opacity.map_or(1., |index| params[index][0] / 100.)
    }
}

impl PreparedEffect {
    pub(in crate::effect_plan) fn output_region(
        &self,
        input: [f32; 4],
    ) -> Result<[f32; 4], RenderError> {
        self.semantics.output_region(&self.params, input)
    }
    pub(in crate::effect_plan) fn direct_output(&self) -> bool {
        self.semantics.direct_output()
    }
    pub(in crate::effect_plan) fn working(&self) -> [f32; 2] {
        self.semantics.working()
    }
    pub(in crate::effect_plan) fn working_slot(&self) -> u32 {
        self.semantics.working_slot()
    }
    pub(in crate::effect_plan) fn edge(&self) -> f32 {
        self.semantics.edge(&self.params)
    }
    pub(in crate::effect_plan) fn opacity(&self) -> f32 {
        self.semantics.opacity(&self.params)
    }
}

#[cfg(test)]
mod tests;
