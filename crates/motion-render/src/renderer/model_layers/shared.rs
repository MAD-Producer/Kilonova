//! First project consumer for homogeneous spatial model compositions. Authored
//! plane, effect, matte, sampling and opacity boundaries retain their own path.
use super::*;
use crate::scene_gpu::{ScenePass, ScenePassFrame, ScenePassInstance};

fn candidate(scene: &Scene) -> bool {
    let Some(samples) = scene
        .layers
        .first()
        .and_then(|l| l.model_source.as_ref())
        .map(|c| c.samples)
    else {
        return false;
    };
    scene.layers.len() >= 2
        && scene.layers.len() <= MAX_LAYERS
        && scene.effects.is_empty()
        && scene.nested.is_empty()
        && scene.layers.iter().all(|layer| {
            layer
                .model_source
                .as_ref()
                .is_some_and(|clip| clip.samples == samples)
                && layer.three_d
                && layer.opacity == 1.
                && layer.color == [1.; 4]
                && layer.masks.is_empty()
                && layer.track_matte.is_none()
                && layer.blend == motion_model::compositing::LayerBlend::default()
                && layer.composite_visible
                && !layer.adjustment
                && !layer.composition
                && layer.sampling.is_default()
        })
}

impl Renderer {
    pub(in crate::renderer) fn prepare_shared_model_scene(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<Option<(Scene, RenderStats)>> {
        if !candidate(scene) {
            return Ok(None);
        }
        let began = Instant::now();
        let anchor = &scene.layers[0];
        let clip = anchor
            .model_source
            .as_ref()
            .ok_or_else(|| RenderError::Invalid("shared model anchor is missing".into()))?;
        for layer in &scene.layers {
            let source_id = layer
                .model_source
                .as_ref()
                .ok_or_else(|| RenderError::Invalid("shared model source is missing".into()))?
                .asset;
            if self.models.gpu.contains_key(&source_id) {
                continue;
            }
            let source = self
                .models
                .sources
                .get(&source_id)
                .ok_or_else(|| {
                    RenderError::Invalid(format!(
                        "model object={} source={source_id} not configured",
                        layer.id
                    ))
                })?
                .scene
                .clone();
            self.trim_image_cache(0);
            let available = self
                .effect_gpu
                .builder
                .source_texture_budget
                .saturating_sub(self.resident_source_texture_bytes());
            let gpu = GpuScene::upload(
                self,
                SceneRequest {
                    request: 0,
                    source: source_id,
                },
                source,
                SceneGpuLimits {
                    source_texture_bytes: available,
                    ..Default::default()
                },
            )
            .map_err(scene_error)?;
            self.models.gpu.insert(source_id, gpu);
        }
        let [_, _, w, h] = crate::fit_viewport(
            [0., 0., width as f32, height as f32],
            [scene.width, scene.height],
        );
        let (w, h) = (w.ceil().max(1.) as u32, h.ceil().max(1.) as u32);
        let key = TextureKey::Model(anchor.id);
        let previous_bytes = self.images.get(&key).map_or(0, |image| image.bytes);
        let bytes = u64::from(w)
            .checked_mul(u64::from(h))
            .and_then(|n| {
                n.checked_mul(
                    crate::image_formats::color_storage(self.working_precision()).bytes_per_pixel(),
                )
            })
            .ok_or_else(|| RenderError::Invalid("shared model raster capacity overflow".into()))?;
        let required = self
            .resident_source_texture_bytes()
            .checked_sub(previous_bytes)
            .and_then(|n| n.checked_add(bytes))
            .ok_or_else(|| {
                RenderError::Invalid("shared model source accounting overflow".into())
            })?;
        if required > self.effect_gpu.builder.source_texture_budget {
            return Err(RenderError::budget(
                ResourceBudgetScope::SourceTextures,
                required,
                self.effect_gpu.builder.source_texture_budget,
                format!(
                    "model object={} shared raster exceeds source texture budget",
                    anchor.id
                ),
            ));
        }
        let mut pass = match self.models.shared.take() {
            Some(pass) => pass,
            None => {
                ScenePass::new(self, SceneGpuLimits::default().max_draws).map_err(scene_error)?
            }
        };
        let (target, backup, fresh) = match self.models.targets.remove(&anchor.id) {
            Some(target)
                if target.width == w
                    && target.height == h
                    && target.texture.format() == self.target_format =>
            {
                (target, None, false)
            }
            previous => match self.render_target_in(w, h, crate::TextureCategory::ModelSources) {
                Ok(target) => (target, previous, true),
                Err(error) => {
                    if let Some(previous) = previous {
                        self.models.targets.insert(anchor.id, previous);
                    }
                    self.models.shared = Some(pass);
                    return Err(error);
                }
            },
        };
        let mut owned: Vec<_> = std::mem::take(&mut self.models.gpu).into_iter().collect();
        let result = (|| {
            let indices: HashMap<_, _> = owned
                .iter()
                .enumerate()
                .map(|(i, (id, _))| (*id, i))
                .collect();
            let mut instances = Vec::with_capacity(scene.layers.len());
            for layer in &scene.layers {
                let clip = layer
                    .model_source
                    .as_ref()
                    .ok_or_else(|| RenderError::Invalid("shared model source is missing".into()))?;
                let world = layer.model
                    * glam::Mat4::from_translation(
                        -glam::Vec3::from_array(clip.pivot) * clip.pixels_per_metre,
                    );
                // Like ordinary model preparation, zero scale is an empty visual.
                if world.determinant().abs() <= 1e-12 {
                    continue;
                }
                let index = *indices.get(&clip.asset).ok_or_else(|| {
                    RenderError::Invalid(format!("shared model source={} is missing", clip.asset))
                })?;
                let mut instance = ScenePassInstance::new(
                    index,
                    SceneRequest {
                        request: 0,
                        source: clip.asset,
                    },
                    clip.scene as usize,
                    clip.pixels_per_metre,
                );
                instance.world = world;
                instance.animation = layer.model_animation;
                instances.push(instance);
            }
            let mut sources: Vec<_> = owned.iter_mut().map(|(_, gpu)| gpu).collect();
            let mut frame = ScenePassFrame::new(scene.camera);
            frame.samples = clip.samples;
            pass.encode_composition(
                self,
                SceneRequest {
                    request: 0,
                    source: clip.asset,
                },
                encoder,
                &target,
                &mut sources,
                &instances,
                &frame,
            )
            .map_err(scene_error)
        })();
        self.models.gpu = owned.into_iter().collect();
        self.models.shared = Some(pass);
        let stats = match result {
            Ok(stats) => stats,
            Err(error) => {
                if !fresh {
                    self.models.targets.insert(anchor.id, target);
                } else if let Some(previous) = backup {
                    self.models.targets.insert(anchor.id, previous);
                }
                return Err(error);
            }
        };
        if self
            .images
            .get(&key)
            .is_none_or(|image| image.view.texture() != &*target.texture)
        {
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Shared model composition raster"),
                layout: &self.image_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&target.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.images.insert(
                key,
                GpuImage {
                    _texture: target.texture.clone(),
                    view: target.view.view.clone(),
                    bind_group: group,
                    bytes,
                    size: (w, h),
                    video_stamp: None,
                },
            );
            self.texture_bytes = self.texture_bytes - previous_bytes + bytes;
            self.effect_gpu.invalidate();
        }
        self.models.targets.insert(anchor.id, target);
        let mut output = scene.clone();
        output.layers.truncate(1);
        let report = RenderStats {
            draw_calls: stats.draws as u32,
            parameter_upload_bytes: stats.parameter_bytes,
            cpu_prepare_us: began.elapsed().as_micros() as u64,
            ..Default::default()
        };
        Ok(Some((output, report)))
    }
}
