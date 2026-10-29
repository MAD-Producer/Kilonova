//! Frozen implementation proof for coordinate-preserving depth materials.
use super::*;
impl PlanBuilder {
    /// A depth material must preserve source coordinates and have a checked
    /// implementation. The boolean records whether sampled modes preserve alpha.
    pub(crate) fn depth_material_alpha(
        &mut self,
        scene: &Scene,
        layer: u64,
    ) -> Result<Option<bool>, RenderError> {
        self.synchronize(scene)?;
        let mut alpha = true;
        for (index, effect) in
            scene.effects.iter().enumerate().filter(|(_, effect)| {
                effect.layer == layer && crate::scene::effect_has_output(effect)
            })
        {
            let resolved = self.resolved[index]
                .as_ref()
                .ok_or("depth material definition is absent")?;
            if let Some(error) = &resolved.error {
                return Err(RenderError::Shared(error.clone()));
            }
            let motion_effects::pointwise::Eligibility::Proven(proof) = &resolved.pointwise else {
                return Ok(None);
            };
            if effect.image_input.is_some() {
                return Ok(None);
            }
            let [program] = resolved.programs.as_slice() else {
                return Ok(None);
            };
            if let Some(error) = self.program_errors.get(program) {
                return Err(RenderError::Invalid(error.clone()));
            }
            let Some(program) = self.programs.get(*program as usize) else {
                return Ok(None);
            };
            if !program.resources.is_empty() || !proof.matches_compiled(&program.shader)? {
                return Ok(None);
            }
            if proof.op() == motion_effects::pointwise::PointwiseOp::Invert {
                let value = |id: &str| {
                    effect
                        .param_ids
                        .iter()
                        .position(|p| p == id)
                        .and_then(|i| effect.values.get(i))
                        .map(|v| v[0])
                };
                // Channel five inverts alpha; RGB/R/G/B modes leave it intact.
                alpha &= value("blend") == Some(100.)
                    || matches!(value("channel"), Some(1. | 2. | 3. | 4.));
            }
        }
        Ok(Some(alpha))
    }
}
