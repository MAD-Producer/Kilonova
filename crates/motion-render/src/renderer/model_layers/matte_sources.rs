//! Independent matte images use actual projected source geometry.
use super::*;
use crate::renderer::depth_materials::DepthMaterial;
use crate::scene_gpu::{ScenePass, ScenePassFrame, ScenePlane};

/// Iterative dependency traversal bounds storage by the sampled layer count.
/// Ordinary and calibrated parents read independently projected dependencies.
/// Cycles are rejected before any dependency is rendered.
pub(in crate::renderer) fn source_order(scene: &Scene, roots: &[u64]) -> Result<Vec<u64>> {
    let mut marks = HashMap::new();
    let mut ordered = Vec::new();
    for &root in roots {
        let mut pending = vec![(root, false)];
        while let Some((source, expanded)) = pending.pop() {
            if expanded {
                marks.insert(source, 2u8);
                ordered.push(source);
                continue;
            }
            match marks.get(&source) {
                Some(2) => continue,
                Some(1) => {
                    return Err(RenderError::Invalid(format!(
                        "depth matte source={source} phase=dependencies: cyclic source chain"
                    )))
                }
                _ => {}
            }
            let layer = scene
                .layers
                .iter()
                .find(|l| l.id == source)
                .ok_or_else(|| {
                    RenderError::Invalid(format!(
                        "depth matte source={source} phase=dependencies: source is absent"
                    ))
                })?;
            marks.insert(source, 1);
            pending.push((source, true));
            if let Some(matte) = layer.track_matte {
                pending.push((matte.source, false));
            }
        }
    }
    Ok(ordered)
}

impl Renderer {
    pub(in crate::renderer) fn project_native_matte_source(
        &mut self,
        scene: &Scene,
        source: u64,
        width: u32,
        height: u32,
        projected: &HashMap<u64, RenderTarget>,
        retained_bytes: u64,
        materials: &HashMap<u64, DepthMaterial>,
    ) -> Result<(RenderTarget, RenderStats)> {
        let began = Instant::now();
        let layer = scene
            .layers
            .iter()
            .find(|l| l.id == source)
            .ok_or_else(|| {
                RenderError::Invalid(format!(
                    "depth matte source={source} phase=geometry: source is absent"
                ))
            })?;
        let key = layer
            .asset
            .map(TextureKey::Static)
            .or_else(|| layer.video.as_ref().map(|_| TextureKey::Video(source)))
            .ok_or_else(|| {
                RenderError::Invalid(format!(
                    "depth matte source={source} phase=geometry: source color is absent"
                ))
            })?;
        let material = materials.get(&source);
        let texture = if let Some(material) = material { material.view()? } else { self.images.get(&key)
            .ok_or_else(|| RenderError::Invalid(format!("depth matte object={source} key={key:?} phase=geometry: prepared color is missing")))?
            ._texture.admitted_view()? };
        let target =
            self.render_target_in(width, height, crate::TextureCategory::MatteBlendScratch)?;
        let mut encoder = self.allocation_command_encoder_for(
            crate::gpu_commands::CommandStage::RetainedLayer,
            &wgpu::CommandEncoderDescriptor {
                label: Some("independent native matte geometry"),
            },
        )?;
        let mut mask_scene = scene.clone();
        mask_scene.layers = vec![layer.clone()];
        if material.is_some() { mask_scene.layers[0].masks = Default::default(); }
        let masks = crate::mask_plan::MaskCache::default().build(&mask_scene, 1., false)?;
        let mut coverage = HashMap::new();
        let mut coverage_bytes = 0;
        let color_bytes = u64::from(width)
            * u64::from(height)
            * crate::image_formats::color_storage(self.working_precision()).bytes_per_pixel();
        if !masks.is_empty() {
            self.mask_gpu.fresh_frame().prepare_checked(
                &self.device,
                &self.queue,
                &mut encoder,
                &self.image_layout,
                &mask_scene,
                &masks,
                &mut coverage,
                &mut coverage_bytes,
                self.resident_source_texture_bytes()
                    .saturating_add(retained_bytes)
                    .saturating_add(color_bytes),
                self.effect_gpu.builder.source_texture_budget,
                None,
            )?;
        }
        let matte = layer.track_matte.map(|matte| -> Result<_> {
            Ok((projected.get(&matte.source)
                .ok_or_else(|| RenderError::Invalid(format!("depth matte object={source} source={} phase=geometry: projected dependency missing", matte.source)))?
                .texture.admitted_view()?, matte.mode))
        }).transpose()?;
        let plane = ScenePlane {
            object: source,
            working_material: material.is_some(),
            world: layer.model,
            size: layer.size,
            color: material.map_or([layer.color[0], layer.color[1], layer.color[2], layer.color[3] * layer.opacity], DepthMaterial::color),
            texture: Some(texture),
            surface: scene.depth_surface(source),
            coverage: coverage
                .get(&TextureKey::Mask(source))
                .map(|image| image._texture.admitted_view())
                .transpose()?,
            matte,
            blend: true,
        };
        let mut frame = ScenePassFrame::new(scene.native_camera());
        frame.viewport = scene.native_camera_viewport(width, height);
        frame.samples = 4;
        let mut pass = match self.models.matte_pass.take() {
            Some(pass) => pass,
            None => {
                ScenePass::new(self, SceneGpuLimits::default().max_draws).map_err(scene_error)?
            }
        };
        let encoded = pass
            .encode_composition(
                self,
                SceneRequest { request: 0, source },
                &mut encoder,
                &target,
                &mut [],
                &[],
                &frame,
                &[plane],
            )
            .map_err(scene_error);
        let ledger = pass.texture_resource_ledger();
        self.models.matte_pass = Some(pass);
        let stats = encoded?;
        self.submit_allocated(encoder.finish(), |resources| resources.merge(ledger))?;
        self.check_health()?;
        Ok((
            target,
            RenderStats {
                draw_calls: stats.draws as u32,
                parameter_upload_bytes: stats.parameter_bytes + stats.plane_vertex_bytes,
                cpu_prepare_us: began.elapsed().as_micros() as u64,
                ..Default::default()
            },
        ))
    }
}
