//! Center-time dependency preparation and phase-time adjustment backdrops.
use super::*;

impl Renderer {
    fn preflight_exposure_context(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        preview: bool,
    ) -> Result<()> {
        for node in &scene.nested {
            let (w, h) = nested_size(scene, &node.scene, width, height);
            self.preflight_exposure_context(&node.scene, w, h, preview)
                .map_err(|error| {
                    let message = node.scene.diagnostic(&error.to_string());
                    error.with_diagnostic(message)
                })?;
        }
        self.effect_gpu.builder.build_exposure_context_checked(
            scene,
            &self.asset_order,
            width,
            height,
            preview,
        )?;
        self.prepare_effect_images();
        self.effect_gpu.state.prepare(
            &self.device,
            &self.queue,
            &self.image_layout,
            &self.effect_gpu.builder,
            scene,
            self.texture_bytes + self.video_plane_bytes(),
            self.effect_gpu.builder.source_texture_budget,
        )
    }

    pub(super) fn prepare_phase_adjustments(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        preview: bool,
    ) -> Result<RenderStats> {
        let context = format!(
            "exposure composition={} sample_frame={} phase=adjustment_prepare",
            scene.composition_id, scene.frame
        );
        let result = (|| {
            self.check_exposure_media(scene, &context)?;
            // Compile/validate children too, so the ordinary nested draw cannot
            // silently bypass a failed effect while preparing a phase source.
            self.preflight_exposure_context(scene, width, height, preview)?;
            let (children, diagnostics) = self.prepare_nested(scene, width, height, preview)?;
            let mut encoder = self.allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("center-time adjustment dependencies"),
                })?;
            let mut stats = self.encode_effects(
                scene,
                None,
                width,
                height,
                &mut encoder,
                None,
                preview,
                None,
                true,
            )?;
            // Submit center geometry before the phase compositor uploads its
            // independent planes into the shared vertex/uniform buffers.
            self.submit_allocated(encoder.finish(), |_| {})?;
            self.check_health()?;
            stats.add(children);
            self.effect_diagnostics.extend(diagnostics);
            Ok(stats)
        })();
        result.map_err(|error: RenderError| {
            let message = format!("{context}: {error}");
            error.with_diagnostic(message)
        })
    }

    pub(super) fn encode_adjusted_backdrop(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        uniform_index: usize,
        accumulator: usize,
    ) -> Result<usize> {
        let next = 1 - accumulator;
        let base = self
            .layer_gpu
            .accumulators
            .get(accumulator)
            .ok_or_else(|| {
                RenderError::Invalid("adjustment phase=backdrop: base accumulator missing".into())
            })?;
        let output = self.layer_gpu.accumulators.get(next).ok_or_else(|| {
            RenderError::Invalid("adjustment phase=backdrop: output accumulator missing".into())
        })?;
        let filtered = self.effect_gpu.state.pool[0].as_ref().ok_or_else(|| {
            RenderError::Invalid("adjustment phase=backdrop: filtered image missing".into())
        })?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            multiview_mask: None,
            label: Some("adjust lower composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: &output.view,
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
        pass.set_pipeline(&self.layer_gpu.adjustment);
        pass.set_bind_group(
            0,
            &self.uniform_group,
            &[(uniform_index * self.uniform_stride) as u32],
        );
        pass.set_bind_group(1, &base.composite, &[]);
        pass.set_bind_group(2, &filtered.composite, &[]);
        pass.draw(0..3, 0..1);
        Ok(next)
    }
}
