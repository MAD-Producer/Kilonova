//! Owned effects-stage images and receiver-time projected matte coverage.
use super::*;
use crate::effect_gpu::FxTexture;

fn constant_receiver(scene: &Scene, layer: &crate::DrawLayer) -> bool {
    layer.asset.is_none()
        && layer.video.is_none()
        && layer.vector.is_none()
        && !layer.composition
        && layer.masks.is_empty()
        && !scene
            .effects
            .iter()
            .any(|effect| effect.layer == layer.id && effect.enabled)
}

fn clear_constant(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("constant receiver source"),
        multiview_mask: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
}

/// A GPU-only local image. Geometry, opacity, blend and matte remain separate
/// so a temporal phase can perform ordinary plane sorting and compositing.
/// The image survives subsequent renders. Drop releases its logical capacity;
/// submitted resources retire through the existing submission ledger.
pub struct LayerRaster {
    image: FxTexture,
    matte: Option<FxTexture>,
    plane: crate::DrawLayer,
    matte_mode: u32,
    additive: bool,
    composition: String,
    composition_size: [u32; 2],
    output_size: [u32; 2],
    preview: bool,
    frame: f64,
    reservation: super::exposure::Reservation,
}
impl LayerRaster {
    pub fn layer(&self) -> &crate::DrawLayer {
        &self.plane
    }
    pub fn frame(&self) -> f64 {
        self.frame
    }
    pub fn size(&self) -> [u32; 2] {
        let size = self.image.texture.size();
        [size.width, size.height]
    }
    pub fn texture_bytes(&self) -> u64 {
        self.reservation.bytes
    }
    pub fn texture_resource_ledger(&self) -> crate::TextureResourceLedger {
        let mut ledger = crate::TextureResourceLedger::default();
        ledger.retained(
            &self.image.texture,
            crate::TextureCategory::ExposureTargets,
            format!("exposure/receiver/{}/local", self.plane.id),
        );
        if let Some(matte) = &self.matte {
            ledger.retained(
                &matte.texture,
                crate::TextureCategory::ExposureTargets,
                format!("exposure/receiver/{}/projected-matte", self.plane.id),
            );
        }
        ledger
    }
}

enum PhaseLayer<'a> {
    Raster(&'a LayerRaster),
    Adjustment(&'a crate::DrawLayer, usize),
}
impl PhaseLayer<'_> {
    fn plane(&self) -> &crate::DrawLayer {
        match self {
            Self::Raster(raster) => &raster.plane,
            Self::Adjustment(layer, _) => layer,
        }
    }
}

impl Renderer {
    /// Materialize one receiver using its entire sampled dependency context.
    /// Assets and exact video samples must already be ready, as for draw().
    /// None is a normal absent receiver; adjustment layers require phase-time
    /// backdrop processing and cannot be represented as independent images.
    pub fn materialize_layer(
        &mut self,
        scene: &Scene,
        receiver: u64,
        width: u32,
        height: u32,
        preview: bool,
    ) -> Result<Option<(LayerRaster, RenderStats)>> {
        self.check_health()?;
        self.check_raster_extent(scene, width, height)?;
        let Some(layer) = scene.layers.iter().find(|layer| layer.id == receiver) else {
            return Ok(None);
        };
        if layer.adjustment {
            return Err(RenderError::Invalid(format!(
                "exposure object={receiver} phase=local_materialize: adjustment requires the phase backdrop")));
        }
        let context = format!(
            "exposure object={receiver} sample_frame={} phase=local_materialize",
            scene.frame
        );
        self.check_exposure_media(scene, &context)?;
        if layer.track_matte.is_none() && constant_receiver(scene, layer) {
            // Keep the original floating-point tint/alpha. Baking a constant
            // into RGBA8 before applying opacity introduces extra rounding and
            // needlessly allocates a full source-sized image for a solid.
            let started = Instant::now();
            self.check_resident_sources().map_err(|error| {
                let message = format!("{context}: {error}");
                error.with_diagnostic(message)
            })?;
            let reservation = self.reserve_exposure_bytes(4, &context)?;
            let image = crate::effect_gpu::texture(
                &self.device,
                &self.image_layout,
                &self.sampler,
                1,
                1,
                TARGET_FORMAT,
            );
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("constant receiver image"),
                });
            clear_constant(&mut encoder, &image.view);
            let mut plane = layer.clone();
            plane.sampling = Default::default();
            let raster = LayerRaster {
                image,
                matte: None,
                plane,
                matte_mode: 0,
                additive: false,
                composition: scene.composition_id.clone(),
                composition_size: [scene.width, scene.height],
                output_size: [width, height],
                preview,
                frame: scene.frame,
                reservation,
            };
            self.submit_tracked(encoder.finish(), |ledger| {
                ledger.merge(raster.texture_resource_ledger())
            });
            self.check_health()?;
            self.effect_diagnostics.clear();
            return Ok(Some((
                raster,
                RenderStats {
                    cpu_prepare_us: started.elapsed().as_micros() as u64,
                    texture_bytes: self.texture_bytes
                        + self.video_plane_bytes()
                        + self.effect_gpu.state.bytes()
                        + self.layer_gpu.bytes()
                        + self.compositing_gpu.as_ref().map_or(0, |gpu| gpu.bytes())
                        + self.exposure_texture_bytes(),
                    ..Default::default()
                },
            )));
        }
        let (children, diagnostics) =
            self.prepare_nested(scene, width, height, preview)
                .map_err(|error| {
                    let message = format!("{context}: {error}");
                    error.with_diagnostic(message)
                })?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("receiver-time local image"),
            });
        let mut output = None;
        let mut stats = self
            .encode_effects(
                scene,
                None,
                width,
                height,
                &mut encoder,
                None,
                preview,
                Some((receiver, &mut output)),
                false,
            )
            .map_err(|error| {
                let message = format!("{context}: {error}");
                error.with_diagnostic(message)
            })?;
        let raster = output
            .ok_or_else(|| RenderError::Invalid(format!("{context}: missing local image")))?;
        self.submit_tracked(encoder.finish(), |ledger| {
            ledger.merge(raster.texture_resource_ledger())
        });
        self.check_health().map_err(|error| {
            let message = format!("{context} phase=submit: {error}");
            error.with_diagnostic(message)
        })?;
        stats.add(children);
        self.effect_diagnostics.extend(diagnostics);
        Ok(Some((raster, stats)))
    }

    fn check_raster_extent(&self, scene: &Scene, width: u32, height: u32) -> Result<()> {
        let max = self.device.limits().max_texture_dimension_2d;
        if width == 0
            || height == 0
            || scene.width == 0
            || scene.height == 0
            || !scene.frame.is_finite()
            || scene.layers.len() > MAX_LAYERS
            || u64::from(width) * u64::from(height) > CAPTURE_PIXEL_LIMIT
        {
            return Err(RenderError::Invalid(
                "exposure phase=local_extent: invalid extent, time or layer count".into(),
            ));
        }
        if width > max || height > max {
            return Err(RenderError::Resource {
                rejection: ResourceRejection::TextureDimensions {
                    width,
                    height,
                    max_dimension: max,
                },
                message: "exposure phase=local_extent: exceeds device dimensions".into(),
            });
        }
        Ok(())
    }

    pub(super) fn copy_layer_raster(
        &self,
        scene: &Scene,
        receiver: u64,
        output_size: [u32; 2],
        matte_size: [u32; 2],
        preview: bool,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<LayerRaster> {
        let frame = &self.effect_gpu.builder.frame;
        let index = frame
            .draws
            .iter()
            .position(|draw| draw.layer == receiver)
            .ok_or_else(|| {
                RenderError::Invalid(format!(
                    "exposure object={receiver} phase=local_copy: absent receiver"
                ))
            })?;
        let constant = constant_receiver(scene, &scene.layers[index]);
        let source = self
            .images
            .get(&if constant {
                TextureKey::Static(0)
            } else {
                TextureKey::EffectInput(receiver)
            })
            .ok_or_else(|| {
                RenderError::Invalid(format!(
                    "exposure object={receiver} phase=local_copy: missing effects-stage image"
                ))
            })?;
        let composite = frame.compositing[index];
        let matte_source = if composite.matte >= 0 {
            let source_layer = frame.draws[composite.matte as usize].layer;
            Some(self.compositing_gpu.as_ref().and_then(|gpu| gpu.mattes.get(&(source_layer, composite.mode >= 3)))
                .ok_or_else(|| RenderError::Invalid(format!("exposure object={receiver} source={source_layer} phase=local_copy: missing projected matte")))?)
        } else {
            None
        };
        let bytes = u64::from(source.size.0) * u64::from(source.size.1) * 4
            + if matte_source.is_some() {
                u64::from(matte_size[0]) * u64::from(matte_size[1])
            } else {
                0
            };
        let reservation = self.reserve_exposure_bytes(
            bytes,
            &format!("exposure object={receiver} phase=local_allocate"),
        )?;
        let image = crate::effect_gpu::texture(
            &self.device,
            &self.image_layout,
            &self.sampler,
            source.size.0,
            source.size.1,
            TARGET_FORMAT,
        );
        let copy = |encoder: &mut wgpu::CommandEncoder,
                    src: &wgpu::Texture,
                    dst: &wgpu::Texture,
                    size: [u32; 2]| {
            encoder.copy_texture_to_texture(
                src.as_image_copy(),
                dst.as_image_copy(),
                wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
            );
        };
        if constant {
            clear_constant(encoder, &image.view);
        } else {
            copy(
                encoder,
                &source._texture,
                &image.texture,
                [source.size.0, source.size.1],
            );
        }
        let matte = matte_source.map(|source| {
            let target = crate::effect_gpu::texture(
                &self.device,
                &self.image_layout,
                &self.sampler,
                matte_size[0],
                matte_size[1],
                wgpu::TextureFormat::R8Unorm,
            );
            copy(encoder, &source.texture, &target.texture, matte_size);
            target
        });
        let mut plane = self.effect_gpu.builder.materialized_plane(scene, index);
        if constant {
            plane.color = scene.layers[index].color;
        }
        Ok(LayerRaster {
            image,
            matte,
            plane,
            matte_mode: composite.mode,
            additive: frame.draws[index].words[23] > 0.5,
            composition: scene.composition_id.clone(),
            composition_size: [scene.width, scene.height],
            output_size,
            preview,
            frame: scene.frame,
            reservation,
        })
    }

    /// Composite retained receiver images in authored order, then ordinary BSP
    /// order for spatial planes. No effect execution, decoding or pixel readback.
    /// Context supplies the phase's background and global camera. Each retained
    /// plane keeps its own receiver-time projection. All images must use the
    /// same composition, output extent and preview policy.
    pub fn draw_layer_rasters(
        &mut self,
        context: &Scene,
        rasters: &[&LayerRaster],
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<RenderStats> {
        self.draw_layer_rasters_tracked(context, rasters, view, width, height, |_| {})
    }
    pub(super) fn draw_layer_rasters_tracked(
        &mut self,
        context: &Scene,
        rasters: &[&LayerRaster],
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        output_resources: impl FnOnce(&mut crate::TextureResourceLedger),
    ) -> Result<RenderStats> {
        self.draw_exposure_phase_tracked(
            context,
            rasters,
            view,
            width,
            height,
            None,
            output_resources,
        )
    }
    /// Compose receiver-time images and center-time adjustments against each
    /// phase's lower backdrop. Prepare exact center-time media before calling.
    pub fn draw_exposure_phase(
        &mut self,
        context: &Scene,
        rasters: &[&LayerRaster],
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        preview: bool,
    ) -> Result<RenderStats> {
        self.draw_exposure_phase_tracked(
            context,
            rasters,
            view,
            width,
            height,
            Some(preview),
            |_| {},
        )
    }
    pub(super) fn draw_exposure_phase_tracked(
        &mut self,
        context: &Scene,
        rasters: &[&LayerRaster],
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        adjustments: Option<bool>,
        output_resources: impl FnOnce(&mut crate::TextureResourceLedger),
    ) -> Result<RenderStats> {
        self.check_health()?;
        let started = Instant::now();
        self.check_raster_extent(context, width, height)?;
        let output = view.texture();
        if output.width() != width
            || output.height() != height
            || output.format() != self.target_format
        {
            return Err(RenderError::Invalid(
                "exposure phase=local_composite: output extent or format mismatch".into(),
            ));
        }
        if rasters.len() > MAX_LAYERS {
            return Err(RenderError::Invalid(
                "exposure phase=local_composite: too many receivers".into(),
            ));
        }
        if adjustments.is_none()
            && context
                .layers
                .iter()
                .any(|layer| layer.adjustment && layer.composite_visible)
        {
            return Err(RenderError::Invalid(
                "exposure phase=local_composite: adjustment backdrop processing is not connected"
                    .into(),
            ));
        }
        let active = self.exposure_texture_bytes();
        let budget = self.memory_policy().scratch_budget_bytes;
        if active > budget {
            return Err(RenderError::budget(
                ResourceBudgetScope::ExposureTargets,
                active,
                budget,
                "exposure phase=local_composite: live temporal textures exceed current budget",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for raster in rasters {
            if !Arc::ptr_eq(&raster.reservation.counter, &self.exposure_bytes) {
                return Err(RenderError::Invalid(format!(
                    "exposure object={} phase=local_composite: belongs to another renderer",
                    raster.plane.id
                )));
            }
            if raster.composition != context.composition_id
                || raster.composition_size != [context.width, context.height]
                || raster.output_size != [width, height]
                || adjustments.is_some_and(|preview| preview != raster.preview)
                || rasters
                    .first()
                    .is_some_and(|first| first.preview != raster.preview)
            {
                return Err(RenderError::Invalid(format!("exposure object={} phase=local_composite: incompatible composition, extent or preview policy", raster.plane.id)));
            }
            if !seen.insert(raster.plane.id) {
                return Err(RenderError::Invalid(format!(
                    "exposure object={} phase=local_composite: duplicate receiver",
                    raster.plane.id
                )));
            }
        }
        self.check_resident_sources()?;
        let mut ordered: Vec<_> = rasters
            .iter()
            .map(|raster| PhaseLayer::Raster(*raster))
            .collect();
        if adjustments.is_some() {
            for (index, layer) in context
                .layers
                .iter()
                .enumerate()
                .filter(|(_, layer)| layer.adjustment)
            {
                if !seen.insert(layer.id) {
                    return Err(RenderError::Invalid(format!(
                        "exposure object={} phase=local_composite: duplicate adjustment receiver",
                        layer.id
                    )));
                }
                ordered.push(PhaseLayer::Adjustment(layer, index));
            }
        }
        if ordered.len() > MAX_LAYERS {
            return Err(RenderError::Invalid(
                "exposure phase=local_composite: too many receivers and adjustments".into(),
            ));
        }
        let prepare_adjustments = ordered
            .iter()
            .any(|layer| matches!(layer, PhaseLayer::Adjustment(..)));
        let preparation = if prepare_adjustments {
            self.prepare_phase_adjustments(context, width, height, adjustments.unwrap_or(false))?
        } else {
            RenderStats::default()
        };
        let has_adjustment = prepare_adjustments
            && ordered.iter().any(|layer| match layer {
                PhaseLayer::Adjustment(_, index) => {
                    let draw = &self.effect_gpu.builder.frame.draws[*index];
                    draw.pass_start < draw.pass_end
                }
                _ => false,
            });
        ordered.sort_by_key(|layer| layer.plane().order);
        let scene =
            context.plane_context(ordered.iter().map(|layer| layer.plane().clone()).collect());
        let sizes: Vec<_> = scene.layers.iter().map(|layer| layer.size).collect();
        let transfer = scene
            .layers
            .iter()
            .any(|layer| layer.composite_visible && layer.blend != Default::default());
        let has_accumulator = transfer || has_adjustment;
        let compositing = transfer || rasters.iter().any(|raster| raster.matte_mode != 0);
        let scale = (width as f32 / scene.width as f32)
            .min(height as f32 / scene.height as f32)
            .min(1.)
            .max(0.001);
        let size = [
            (scene.width as f32 * scale).ceil() as u32,
            (scene.height as f32 * scale).ceil() as u32,
        ];
        let [vx, vy, vw, vh] = crate::fit_viewport(
            [0., 0., width as f32, height as f32],
            [scene.width, scene.height],
        );
        let viewport = if has_accumulator {
            [0., 0., size[0] as f32, size[1] as f32]
        } else {
            [vx, vy, vw, vh]
        };
        let mut geometry = PlaneCompositor::new();
        // Exposure phases use the same physical-pixel fringe as ordinary
        // drawing, including when transfer/adjustment uses a working canvas.
        geometry.set_coverage_viewport(Some([viewport[2], viewport[3]]));
        geometry
            .prepare_with_sizes(&scene, &sizes)
            .map_err(|error| {
                RenderError::Invalid(format!("exposure phase=local_composite geometry: {error}"))
            })?;
        // Apply the existing independent composite/source allowances before
        // allocating accumulators, including after a policy reduction.
        let source_bytes =
            self.texture_bytes + self.video_plane_bytes() + self.effect_gpu.state.resource_bytes;
        let source_budget = self.effect_gpu.builder.source_texture_budget;
        if compositing && source_bytes + 1 > source_budget {
            return Err(RenderError::budget(
                ResourceBudgetScope::SourceTextures,
                source_bytes + 1,
                source_budget,
                "exposure phase=local_composite: matte fallback exceeds source texture budget",
            ));
        }
        let composite_bytes =
            u64::from(size[0]) * u64::from(size[1]) * if transfer { 12 } else { 8 };
        if has_accumulator && composite_bytes > source_budget {
            return Err(RenderError::budget(ResourceBudgetScope::LayerCompositing, composite_bytes, source_budget,
                "exposure phase=local_composite: transfer accumulators exceed source texture budget"));
        }
        let changed = self.layer_gpu.prepare_accumulators(
            &self.device,
            &self.image_layout,
            &self.sampler,
            self.target_format,
            has_accumulator.then_some(size),
        );
        if changed {
            self.effect_gpu.state.invalidate();
        }
        if compositing {
            let builds = &mut self.compositing_pipeline_set_builds;
            let gpu = self.compositing_gpu.get_or_insert_with(|| {
                *builds += 1;
                crate::compositing_gpu::CompositingGpu::new(
                    &self.device,
                    &self.queue,
                    &self.uniform_layout,
                    &self.image_layout,
                    &self.sampler,
                    self.target_format,
                )
            });
            gpu.prepare_mattes(
                &self.device,
                &self.image_layout,
                &self.sampler,
                size,
                &[],
                source_bytes,
                self.effect_gpu.builder.source_texture_budget,
            )?;
            let source_changed = gpu.prepare_source(
                &self.device,
                &self.image_layout,
                &self.sampler,
                transfer,
                self.effect_gpu.builder.source_texture_budget,
            )?;
            gpu.prepare_blend_inputs(
                &self.device,
                &self.queue,
                &self.sampler,
                &self.layer_gpu.accumulators,
                &scene.layers,
                changed || source_changed,
            );
        }
        if prepare_adjustments {
            self.effect_gpu.state.prepare(
                &self.device,
                &self.queue,
                &self.image_layout,
                &self.effect_gpu.builder,
                context,
                self.texture_bytes + self.video_plane_bytes(),
                self.effect_gpu.builder.source_texture_budget,
            )?;
        }
        self.geometry_upload.clear();
        self.geometry_upload
            .extend(geometry.vertices.iter().map(|vertex| GeometryVertex {
                position: vertex.position,
                uv: vertex.uv,
            }));
        if !self.geometry_upload.is_empty() {
            self.queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&self.geometry_upload),
            );
        }
        for (i, entry) in ordered.iter().enumerate() {
            let uniform = match entry {
                PhaseLayer::Adjustment(_, index) => {
                    let w = &self.effect_gpu.builder.frame.draws[*index].words;
                    DrawUniform {
                        mvp: std::array::from_fn(|col| std::array::from_fn(|row| w[col * 4 + row])),
                        color: std::array::from_fn(|j| w[16 + j]),
                        extent_opacity: [w[20], w[21], w[22], 0.],
                        uv_scale: [w[25], w[26], context.width as f32, context.height as f32],
                        screen_uv: crate::plane_coordinates::screen_map(geometry.uv_maps[i], viewport, ordered[i].plane().id)?,
                    }
                }
                PhaseLayer::Raster(raster) => {
                    let flags = if raster.matte_mode != 0 {
                        2 + 4 * u32::from(raster.matte_mode == 2 || raster.matte_mode == 4)
                    } else {
                        0
                    };
                    let mut color = raster.plane.color;
                    for channel in &mut color[..3] {
                        *channel = srgb_to_linear(*channel);
                    }
                    DrawUniform {
                        mvp: raster.plane.view_projection.to_cols_array_2d(),
                        color,
                        extent_opacity: [
                            viewport[0],
                            viewport[1],
                            raster.plane.opacity,
                            flags as f32,
                        ],
                        uv_scale: [1., 1., viewport[2], viewport[3]],
                        screen_uv: crate::plane_coordinates::screen_map(geometry.uv_maps[i], viewport, ordered[i].plane().id)?,
                    }
                }
            };
            self.upload[i * self.uniform_stride..i * self.uniform_stride + DRAW_SIZE as usize]
                .copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        let bytes = ordered.len() * self.uniform_stride;
        if bytes > 0 {
            self.queue
                .write_buffer(&self.uniform_buffer, 0, &self.upload[..bytes]);
        }
        let alpha = scene.background[3];
        let clear = wgpu::Color {
            r: (srgb_to_linear(scene.background[0]) * alpha) as f64,
            g: (srgb_to_linear(scene.background[1]) * alpha) as f64,
            b: (srgb_to_linear(scene.background[2]) * alpha) as f64,
            a: alpha as f64,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("retained receiver phase"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("retained phase clear"),
                multiview_mask: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: if has_accumulator {
                        &self.layer_gpu.accumulators[0].view
                    } else {
                        view
                    },
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if has_accumulator {
                            wgpu::Color::TRANSPARENT
                        } else {
                            clear
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        let mut accumulator = 0;
        let mut draws = 0;
        for batch in &geometry.batches {
            let i = batch.layer;
            if !ordered[i].plane().composite_visible {
                continue;
            }
            let raster = match &ordered[i] {
                PhaseLayer::Raster(raster) => *raster,
                PhaseLayer::Adjustment(layer, index) => {
                    let frame = &self.effect_gpu.builder.frame;
                    let draw = &frame.draws[*index];
                    if draw.pass_start == draw.pass_end || draw.words[22] <= 0. {
                        continue;
                    }
                    for pass in draw.pass_start..draw.pass_end {
                        self.effect_gpu
                            .state
                            .encode_pass(
                                pass,
                                frame,
                                &self.asset_order,
                                None,
                                Some(&self.layer_gpu.accumulators[accumulator].view),
                                &self.images,
                                &self.device,
                                &self.queue,
                                &mut encoder,
                            )
                            .map_err(|error| {
                                let message = format!(
                                    "exposure object={} phase=adjustment_effect: {error}",
                                    layer.id
                                );
                                error.with_diagnostic(message)
                            })?;
                        draws += 1;
                    }
                    accumulator = self.encode_adjusted_backdrop(&mut encoder, i, accumulator)?;
                    draws += 1;
                    continue;
                }
            };
            let layer_transfer = raster.plane.blend != Default::default();
            let target = if layer_transfer {
                &self
                    .compositing_gpu
                    .as_ref()
                    .unwrap()
                    .source
                    .as_ref()
                    .unwrap()
                    .view
            } else if has_accumulator {
                &self.layer_gpu.accumulators[accumulator].view
            } else {
                view
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("retained phase plane"),
                multiview_mask: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if layer_transfer {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_viewport(viewport[0], viewport[1], viewport[2], viewport[3], 0., 1.);
            pass.set_pipeline(if compositing {
                let gpu = self.compositing_gpu.as_ref().unwrap();
                if raster.additive && !layer_transfer {
                    &gpu.additive
                } else {
                    &gpu.normal
                }
            } else if raster.additive {
                &self.additive_pipeline
            } else {
                &self.pipeline
            });
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_bind_group(0, &self.uniform_group, &[(i * self.uniform_stride) as u32]);
            pass.set_bind_group(1, &raster.image.composite, &[]);
            if compositing {
                let gpu = self.compositing_gpu.as_ref().unwrap();
                pass.set_bind_group(2, &gpu.zero.composite, &[]);
                pass.set_bind_group(
                    3,
                    &raster.matte.as_ref().unwrap_or(&gpu.zero).composite,
                    &[],
                );
            }
            pass.draw(batch.vertices.clone(), 0..1);
            drop(pass);
            draws += 1;
            if layer_transfer {
                let gpu = self.compositing_gpu.as_ref().unwrap();
                let next = 1 - accumulator;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("retained phase blend"),
                    multiview_mask: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.layer_gpu.accumulators[next].view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&gpu.blend);
                pass.set_bind_group(0, &gpu.blend_group, &[gpu.blend_offset(i)]);
                pass.set_bind_group(1, &gpu.blend_inputs[accumulator], &[]);
                pass.draw(0..3, 0..1);
                accumulator = next;
                draws += 1;
            }
        }
        if has_accumulator {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("retained phase present"),
                multiview_mask: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_viewport(vx, vy, vw, vh, 0., 1.);
            pass.set_pipeline(&self.layer_gpu.present);
            pass.set_bind_group(0, &self.uniform_group, &[0]);
            pass.set_bind_group(1, &self.layer_gpu.accumulators[accumulator].composite, &[]);
            pass.draw(0..3, 0..1);
            draws += 1;
        }
        self.submit_tracked(encoder.finish(), |ledger| {
            output_resources(ledger);
            for raster in rasters {
                ledger.merge(raster.texture_resource_ledger());
            }
        });
        self.check_health()?;
        let mut stats = RenderStats {
            cpu_prepare_us: started.elapsed().as_micros() as u64,
            draw_calls: draws,
            texture_bytes: self.texture_bytes
                + self.video_plane_bytes()
                + self.effect_gpu.state.bytes()
                + self.layer_gpu.bytes()
                + self.compositing_gpu.as_ref().map_or(0, |gpu| gpu.bytes())
                + active,
            parameter_upload_bytes: (bytes + geometry.vertices.len() * 20) as u64,
            parameter_resource_upload_bytes: std::mem::take(
                &mut self.effect_gpu.state.parameter_resource_upload_bytes,
            ),
            ..Default::default()
        };
        let total_us = stats.cpu_prepare_us;
        stats.add(preparation);
        stats.cpu_prepare_us = total_us;
        Ok(stats)
    }
}
