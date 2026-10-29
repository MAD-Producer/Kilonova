//! Source-space image effects precede calibrated geometry and projected mattes.
use super::layer_raster::LayerRaster;
use super::*;
pub(super) struct DepthMaterial {
    raster: LayerRaster,
    pub(super) opaque: bool,
}
impl DepthMaterial {
    pub(super) fn view(&self) -> Result<crate::AdmittedTextureView> {
        self.raster.material_image()
    }
    pub(super) fn color(&self) -> [f32; 4] {
        let p = self.raster.layer();
        [p.color[0], p.color[1], p.color[2], p.color[3] * p.opacity]
    }
    pub(super) fn bytes(&self) -> u64 {
        self.raster.texture_bytes()
    }
}

// Only image-backed planar sources exclusively read by calibrated receivers
// enter this path. Other ordinary consumers still need their independent image
// bindings; do not remove their source from the authored scene prematurely.
fn ordinary_material_source(scene: &Scene, layer: &crate::DrawLayer) -> bool {
    if !layer.three_d
        || !layer.composite_visible
        || layer.asset.is_none()
        || layer.model_source.is_some()
        || layer.vector.is_some()
        || layer.screen_overlay
        || !layer.sampling.is_default()
        || layer.track_matte.is_some()
        || layer.blend != Default::default()
        || scene.effects.iter().any(|e| {
            e.image_input
                .and_then(motion_model::EffectImageInput::layer)
                == Some(layer.id)
        })
    {
        return false;
    }
    let mut consumed = false;
    for receiver in &scene.layers {
        if receiver.track_matte.is_some_and(|m| m.source == layer.id) {
            if scene.depth_surface(receiver.id).is_none() {
                return false;
            }
            consumed = true;
        }
    }
    consumed
}

impl Renderer {
    pub(super) fn prepare_depth_materials(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
    ) -> Result<(HashMap<u64, DepthMaterial>, RenderStats)> {
        // Ordinary-only scenes retain their existing BSP/effects path.
        if scene.calibrated_depth_layers().next().is_none() {
            return Ok((HashMap::new(), RenderStats::default()));
        }
        let mut specs = Vec::new();
        for layer in &scene.layers {
            let object = layer.id;
            let ordinary = ordinary_material_source(scene, layer);
            if scene.depth_surface(object).is_none() && !ordinary {
                continue;
            }
            if !(ordinary && !layer.masks.is_empty())
                && !scene
                    .effects
                    .iter()
                    .any(|e| e.layer == object && crate::scene::effect_has_output(e))
            {
                continue;
            }
            let context = format!(
                "depth surface object={object} source={} phase=material",
                layer
                    .asset
                    .or_else(|| layer.video.as_ref().map(|v| v.asset))
                    .unwrap_or(0)
            );
            let alpha = self.effect_gpu.builder.depth_material_alpha(scene, object)
                .map_err(|e| { let message = format!("{context}: {e}"); e.with_diagnostic(message) })?
                .ok_or_else(|| RenderError::Invalid(format!("{context}: effect requires an explicit depth composition path; material implementation/coordinates are not proven")))?;
            specs.push((object, alpha));
        }
        let mut materials = HashMap::new();
        let mut stats = RenderStats::default();
        for (object, alpha) in specs {
            let layer = scene
                .layers
                .iter()
                .find(|l| l.id == object)
                .ok_or_else(|| {
                    RenderError::Invalid(format!(
                        "depth surface object={object} phase=material: sampled source missing"
                    ))
                })?;
            let mut context = scene.clone();
            // Source-local masks precede image effects. The projected track
            // matte stays separate and is applied to the actual geometry later.
            context
                .layers
                .iter_mut()
                .find(|l| l.id == object)
                .ok_or("depth material source disappeared")?
                .track_matte = None;
            let cause_context = format!(
                "depth surface object={object} source={} phase=material",
                layer
                    .asset
                    .or_else(|| layer.video.as_ref().map(|v| v.asset))
                    .unwrap_or(0)
            );
            let (raster, report) = self
                .materialize_layer(&context, object, width, height, false)
                .map_err(|cause| {
                    let message = format!("{cause_context}: {cause}");
                    cause.with_diagnostic(message)
                })?
                .ok_or_else(|| {
                    RenderError::Invalid(format!(
                        "depth surface object={object} phase=material: source raster missing"
                    ))
                })?;
            if raster.layer().screen_overlay
                || raster.layer().size != layer.size
                || !raster.layer().model.abs_diff_eq(layer.model, 1e-5)
                || raster.layer().view_projection != layer.view_projection
            {
                return Err(RenderError::Invalid(format!("depth surface object={object} phase=material: changed source coordinates require an explicit depth composition path")));
            }
            let key = layer
                .asset
                .map(TextureKey::Static)
                .unwrap_or(TextureKey::Video(object));
            let opaque = alpha
                && layer.masks.is_empty()
                && layer.color[3] == 1.
                && self
                    .images
                    .get(&key)
                    .is_some_and(|i| i.plane_image.is_some_and(|p| p.opaque));
            let material = DepthMaterial { raster, opaque };
            let held: u64 = materials.values().map(DepthMaterial::bytes).sum();
            let required = self
                .resident_source_texture_bytes()
                .saturating_add(held)
                .saturating_add(material.bytes());
            if required > self.effect_gpu.builder.source_texture_budget {
                return Err(RenderError::budget(ResourceBudgetScope::SourceTextures, required,
                    self.effect_gpu.builder.source_texture_budget, format!("depth surface object={object} phase=material: retained material source budget exceeded")));
            }
            stats.add(report);
            materials.insert(object, material);
        }
        Ok((materials, stats))
    }
}
