//! Compatible meshes and planes share actual geometry depth and alpha ordering.
//! Ordinary 2D, effect, matte and sampling boundaries stay intact.
use super::*;
use crate::scene_gpu::{ScenePass, ScenePassFrame, ScenePassInstance, ScenePlane};

fn samples(layer: &crate::DrawLayer) -> Option<u32> {
    let samples = layer.model_source.as_ref()?.samples;
    (matches!(samples, 1 | 4)
        && layer.three_d
        && layer.opacity == 1.
        && layer.color == [1.; 4]
        && layer.sampling.is_default())
    .then_some(samples)
}

fn plane_texture(layer: &crate::DrawLayer) -> Option<TextureKey> {
    layer.asset.map(TextureKey::Static)
        .or_else(|| layer.video.as_ref().map(|_| TextureKey::Video(layer.id)))
}

fn shared_plane(renderer: &Renderer, scene: &Scene, layer: &crate::DrawLayer) -> bool {
    layer.model_source.is_none()
        && plane_texture(layer).is_none_or(|key| {
            renderer
                .images
                .get(&key)
                .is_some_and(|image| image.plane_image.is_some())
        })
        && (layer.video.is_none() || scene.has_owned_video_depth(layer.id))
        && layer.vector.is_none()
        && layer.three_d
        && !layer.screen_overlay
        && layer.sampling.is_default()
}

fn native_matte_sources(renderer: &Renderer, scene: &Scene) -> std::collections::HashSet<u64> {
    if !scene.nested.is_empty() || scene.effects.iter().any(|e| e.scene.is_some())
        || scene.layers.iter().any(|l| l.adjustment || l.composition) {
        return Default::default();
    }
    scene.calibrated_depth_layers().map(|(id, _)| id).filter(|&id| {
        let consumed = scene.layers.iter().any(|l| l.track_matte.is_some_and(|m| m.source == id));
        consumed && scene.layers.iter().any(|l| l.id == id && shared_plane(renderer, scene, l))
            && !scene.effects.iter().any(|e| e.layer == id || e.image_input.and_then(motion_model::EffectImageInput::layer) == Some(id))
    }).collect()
}

fn blocks(renderer: &Renderer, scene: &Scene, native_mattes: &std::collections::HashSet<u64>) -> Vec<std::ops::Range<usize>> {
    // Effects/receivers still consume independent layer images. Do not replace
    // their source with a combined raster or silently cross those boundaries.
    if scene.layers.len() > MAX_LAYERS
        || scene.effects.iter().any(|effect| effect.scene.is_some())
        || !scene.nested.is_empty()
        || scene.layers.iter().any(|layer| layer.adjustment || layer.composition)
    {
        return Vec::new();
    }
    // A receiver outside a spatial run must not disable that run's depth.
    // Keep both matte consumers and their sources isolated, including visible
    // sources: another layer still needs their original independent raster.
    let mut independent_sources: std::collections::HashSet<_> = scene.layers.iter()
        .filter_map(|layer| layer.track_matte.map(|matte| matte.source)).collect();
    // A plain spatial source used only by calibrated receivers has already
    // been independently projected before this run. Its visible geometry must
    // still participate in physical depth rather than overpaint the result.
    independent_sources.retain(|source| {
        if native_mattes.contains(source) { return false; }
        scene.depth_surface(*source).is_some()
            || !scene.layers.iter().any(|layer| layer.id == *source && layer.three_d
            && layer.track_matte.is_none() && layer.masks.is_empty())
            || scene.layers.iter().any(|layer| layer.track_matte.is_some_and(|matte| matte.source == *source)
                && scene.depth_surface(layer.id).is_none())
    });
    // Image effects still need their owner's independent raster and the exact
    // source/effects-stage image of every explicit input. Never substitute the
    // combined spatial raster for either dependency.
    for effect in &scene.effects {
        independent_sources.insert(effect.layer);
        if let Some(source) = effect.image_input.and_then(motion_model::EffectImageInput::layer) {
            independent_sources.insert(source);
        }
    }
    let eligible = |layer: &crate::DrawLayer| {
        (layer.masks.is_empty() || (scene.depth_surface(layer.id).is_some() && plane_texture(layer).is_some()))
            && (layer.track_matte.is_none() || scene.depth_surface(layer.id).is_some())
            && !independent_sources.contains(&layer.id)
            && layer.blend == motion_model::compositing::LayerBlend::default()
            && layer.composite_visible
            && !layer.adjustment
            && !layer.composition
            && (samples(layer).is_some() || shared_plane(renderer, scene, layer))
    };
    // Hidden ordinary image dependencies have no compositing contribution.
    // Retain them for consumers, but do not split physical depth around them.
    let hidden_input = |layer: &crate::DrawLayer| !layer.composite_visible
        && (native_mattes.contains(&layer.id) || (independent_sources.contains(&layer.id) && layer.model_source.is_none()
        && scene.depth_surface(layer.id).is_none() && !layer.adjustment && !layer.composition));
    let mut blocks = Vec::new();
    let mut start = 0;
    while start < scene.layers.len() {
        if !eligible(&scene.layers[start]) {
            start += 1;
            continue;
        }
        let mut count = None;
        let mut end = start;
        while end < scene.layers.len() {
            if hidden_input(&scene.layers[end]) { end += 1; continue; }
            if !eligible(&scene.layers[end]) { break; }
            if let Some(current) = samples(&scene.layers[end]) {
                if count.is_some_and(|count| count != current) {
                    break;
                }
                count = Some(current);
            } else if !shared_plane(renderer, scene, &scene.layers[end]) {
                break;
            }
            end += 1;
        }
        // A pure-plane run retains its existing exact BSP implementation.
        if (count.is_some() && end - start >= 2) || scene.layers[start..end].iter().any(|layer| scene.depth_surface(layer.id).is_some()) {
            blocks.push(start..end);
        }
        start = end;
    }
    blocks
}

impl Renderer {
    pub(in crate::renderer) fn prepare_shared_model_scene(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<Option<(Scene, RenderStats, Vec<(TextureKey, GpuImage)>)>> {
        let native_mattes = native_matte_sources(self, scene);
        let blocks = blocks(self, scene, &native_mattes);
        for (object, calibration) in scene.calibrated_depth_layers() {
            let hidden_native = native_mattes.contains(&object)
                && scene.layers.iter().any(|l| l.id == object && !l.composite_visible);
            if !hidden_native && !blocks.iter().any(|block| scene.layers[block.clone()].iter().any(|layer| layer.id == object)) {
                return Err(RenderError::Invalid(format!("depth surface object={object} source={} phase=shared_geometry: effects, masks, mattes, nested or unsupported sampling require an explicit depth composition path", calibration.asset())));
            }
        }
        // Validate every calibrated source before reporting a dependent
        // receiver's spatial boundary, preserving the primary missing path.
        for (object, calibration) in scene.calibrated_depth_layers() {
            if let Some(matte) = scene.layers.iter().find(|layer| layer.id == object).and_then(|layer| layer.track_matte) {
                if scene.layers.iter().any(|source| source.id == matte.source && source.three_d && source.composite_visible)
                    && !blocks.iter().any(|block| {
                        let run = &scene.layers[block.clone()];
                        run.iter().any(|layer| layer.id == object) && run.iter().any(|layer| layer.id == matte.source)
                    }) {
                    return Err(RenderError::Invalid(format!("depth surface object={object} source={} phase=shared_geometry: visible spatial matte source={} requires an explicit depth composition path", calibration.asset(), matte.source)));
                }
            }
        }
        if blocks.is_empty() && native_mattes.is_empty() {
            return Ok(None);
        }
        let [_, _, mw, mh] = crate::fit_viewport([0., 0., width as f32, height as f32], [scene.width, scene.height]);
        let (mw, mh) = (mw.ceil().max(1.) as u32, mh.ceil().max(1.) as u32);
        let mut mattes = HashMap::new();
        let mut matte_stats = RenderStats::default();
        let mut matte_bytes = 0u64;
        let roots: Vec<_> = blocks.iter().flat_map(|b| scene.layers[b.clone()].iter())
            .filter_map(|layer| layer.track_matte.map(|matte| matte.source))
            .chain(native_mattes.iter().copied()).collect();
        for source in super::matte_sources::source_order(scene, &roots)? {
            let cost = u64::from(mw) * u64::from(mh) * crate::image_formats::color_storage(self.working_precision()).bytes_per_pixel();
            let required = self.resident_source_texture_bytes().saturating_add(matte_bytes).saturating_add(cost);
            if required > self.effect_gpu.builder.source_texture_budget {
                return Err(RenderError::budget(ResourceBudgetScope::SourceTextures, required,
                    self.effect_gpu.builder.source_texture_budget, format!("depth matte source={source} phase=project: source texture budget exceeded")));
            }
            let (target, stats) = self.project_depth_matte_source(scene, source, mw, mh, &mattes, matte_bytes)?;
            matte_stats.add(stats);
            matte_bytes += cost;
            mattes.insert(source, target);
        }
        // Mask coverage must enter the physical fragment before spatial
        // composition, not cut a hole in the combined raster afterwards.
        // Use private, admitted per-encode targets and uniforms: neither normal
        // 2D preparation nor another pending encode may overwrite this frame.
        let mut mask_scene = scene.clone();
        mask_scene.layers = blocks.iter().flat_map(|block| scene.layers[block.clone()].iter())
            .filter(|layer| scene.depth_surface(layer.id).is_some() && !layer.masks.is_empty()).cloned().collect();
        let masks = crate::mask_plan::MaskCache::default().build(&mask_scene, 1., false)?;
        let mut coverage = HashMap::new();
        let mut coverage_bytes = 0;
        if !masks.is_empty() {
            self.mask_gpu.fresh_frame().prepare_checked(&self.device, &self.queue, encoder,
                &self.image_layout, &mask_scene, &masks, &mut coverage, &mut coverage_bytes,
                self.resident_source_texture_bytes().saturating_add(matte_bytes), self.effect_gpu.builder.source_texture_budget, None)?;
        }
        let mut grouped = vec![false; scene.layers.len()];
        let mut keep = vec![true; scene.layers.len()];
        for block in &blocks {
            grouped[block.clone()].fill(true);
            keep[block.clone()].fill(false);
            for index in block.clone() {
                if !scene.layers[index].composite_visible { keep[index] = true; }
            }
            let anchor = block
                .clone()
                .find(|&index| scene.layers[index].model_source.is_some())
                .or_else(|| block.clone().find(|&index| scene.depth_surface(scene.layers[index].id).is_some()))
                .ok_or_else(|| {
                    RenderError::Invalid("shared geometry block lacks a model anchor".into())
                })?;
            keep[anchor] = true;
        }
        // Reuse one bounded scene scratch for isolated models and each run.
        // Every run is encoded in the enclosing command stream; no shared
        // raster is submitted early or reused across an authored 2D boundary.
        let mut scratch = scene.clone();
        scratch.layers = scene
            .layers
            .iter()
            .enumerate()
            .filter(|(index, layer)| !grouped[*index] && layer.model_source.is_some())
            .map(|(_, layer)| layer.clone())
            .collect();
        let mut stats = self.prepare_models(&scratch, width, height)?;
        stats.add(matte_stats);
        for block in blocks {
            scratch.layers.clear();
            scratch.layers.extend_from_slice(&scene.layers[block]);
            stats.add(self.prepare_shared_model_block(&scratch, width, height, encoder, &coverage, coverage_bytes + matte_bytes, &mattes)?);
        }
        let mut output = scene.clone();
        let mut index = 0;
        output.layers.retain(|layer| {
            let retain = keep[index] && (layer.composite_visible || !native_mattes.contains(&layer.id));
            index += 1;
            retain
        });
        for layer in &mut output.layers {
            if layer.model_source.is_none() && scene.depth_surface(layer.id).is_some() {
                output.shared_rasters.insert(layer.id);
                layer.asset = None;
                layer.masks = Default::default();
                layer.track_matte = None;
                layer.video = None;
                layer.size = [scene.width as f32, scene.height as f32];
                layer.source_size = layer.size;
                layer.source_rect = [0., 0., layer.size[0], layer.size[1]];
                layer.model = glam::Mat4::IDENTITY;
                layer.color = [1.; 4];
                layer.opacity = 1.;
                layer.screen_overlay = true;
                layer.three_d = false;
                layer.view_projection = glam::Mat4::from_scale(glam::Vec3::new(
                    2. / scene.width as f32, 2. / scene.height as f32, 1.));
            }
        }
        let inputs = super::super::projected_mattes::bindings(self, &mut output, scene, &mattes)?;
        Ok(Some((output, stats, inputs)))
    }

    fn prepare_shared_model_block(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        encoder: &mut wgpu::CommandEncoder,
        coverage: &HashMap<TextureKey, GpuImage>,
        coverage_bytes: u64,
        mattes: &HashMap<u64, RenderTarget>,
    ) -> Result<RenderStats> {
        let began = Instant::now();
        let anchor = scene
            .layers
            .iter()
            .find(|layer| layer.model_source.is_some())
            .or_else(|| scene.layers.iter().find(|layer| scene.depth_surface(layer.id).is_some()))
            .ok_or_else(|| RenderError::Invalid("shared model anchor is missing".into()))?;
        let sample_count = anchor.model_source.as_ref().map_or(4, |clip| clip.samples);
        let source_id = anchor.model_source.as_ref().map_or_else(
            || anchor.asset.or_else(|| anchor.video.as_ref().map(|v| v.asset)).unwrap_or(0),
            |clip| clip.asset);
        for layer in scene
            .layers
            .iter()
            .filter(|layer| layer.model_source.is_some())
        {
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
                .saturating_sub(self.resident_source_texture_bytes().saturating_add(coverage_bytes));
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
            .and_then(|n| n.checked_add(coverage_bytes))
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
        // Exactly two validated sample counts; alternating blocks must not
        // rebuild and retire each other's native depth/MSAA attachments.
        let slot = usize::from(sample_count == 4);
        let mut pass = match self.models.shared[slot].take() {
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
                    self.models.shared[slot] = Some(pass);
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
            for layer in scene
                .layers
                .iter()
                .filter(|layer| layer.model_source.is_some())
            {
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
            let mut frame = ScenePassFrame::new(scene.native_camera());
            frame.viewport = scene.native_camera_viewport(w, h);
            frame.samples = sample_count;
            let planes: Vec<_> = scene
                .layers
                .iter()
                .filter(|layer| layer.model_source.is_none() && layer.composite_visible)
                .map(|layer| Ok(ScenePlane {
                    object: layer.id,
                    surface: scene.depth_surface(layer.id),
                    world: layer.model,
                    size: layer.size,
                    color: [layer.color[0], layer.color[1], layer.color[2], layer.color[3] * layer.opacity],
                    coverage: coverage.get(&TextureKey::Mask(layer.id)).map(|image| image._texture.admitted_view()).transpose()?,
                    matte: layer.track_matte.map(|matte| -> Result<_> { Ok((mattes.get(&matte.source)
                        .ok_or_else(|| RenderError::Invalid(format!("depth matte object={} source={} phase=bind: projected source missing", layer.id, matte.source)))?
                        .texture.admitted_view()?, matte.mode)) }).transpose()?,
                    blend: layer.track_matte.is_some() || !layer.masks.is_empty() || layer.color[3] != 1. || layer.opacity != 1. || plane_texture(layer).is_some_and(|key| {
                        self.images.get(&key).is_none_or(|image| {
                            image.plane_image.is_none_or(|state| !state.opaque)
                        })
                    }),
                    texture: plane_texture(layer).map(|key| {
                        self.images.get(&key)
                            .ok_or_else(|| RenderError::Invalid(format!(
                                "scene plane object={} key={key:?} phase=shared_plane_image: prepared image is missing", layer.id
                            )))?._texture.admitted_view()
                    }).transpose()?,
                }))
                .collect::<Result<Vec<_>>>()?;
            pass.encode_composition(
                self,
                SceneRequest {
                    request: 0,
                    source: source_id,
                },
                encoder,
                &target,
                &mut sources,
                &instances,
                &frame,
                &planes,
            )
            .map_err(scene_error)
        })();
        self.models.gpu = owned.into_iter().collect();
        self.models.shared[slot] = Some(pass);
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
        // The allocation encoder owns the prior revisions until submission or
        // abandonment. Uploading a referenced color source must not mutate it.
        for key in scene.layers.iter().filter_map(plane_texture) {
            if let Some(state) = self
                .images
                .get_mut(&key)
                .and_then(|image| image.plane_image.as_mut())
            {
                state.frozen = true;
            }
        }
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
                    plane_image: None,
                },
            );
            self.texture_bytes = self.texture_bytes - previous_bytes + bytes;
            self.effect_gpu.invalidate();
        }
        self.models.targets.insert(anchor.id, target);
        let report = RenderStats {
            draw_calls: stats.draws as u32,
            parameter_upload_bytes: stats.parameter_bytes + stats.plane_vertex_bytes,
            cpu_prepare_us: began.elapsed().as_micros() as u64,
            ..Default::default()
        };
        Ok(report)
    }
}
