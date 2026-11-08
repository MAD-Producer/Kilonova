//! Actual Session surface consumer of the frozen native colour pipeline.
//! A candidate group is published only after the complete presentation submits.
use super::*;
use crate::session::color::surface::refuse;

fn inspections(views: &[View<'_>], inspections: &[ViewInspection<'_>]) -> Result<()> {
    validate_inspections(views, inspections)?;
    if inspections.iter().any(|inspection| {
        inspection.channel != motion_render::ViewChannel::Color
            || inspection.region.is_some()
            || inspection.comparison.is_some()
    }) {
        return Err(refuse(
            "surface_inspection",
            "managed channel, ROI and comparison consumers require explicit integration",
        )
        .into());
    }
    Ok(())
}

impl Session {
    pub(super) fn present_color_views(
        &mut self,
        owner: u64,
        views: &[View<'_>],
        inspection: &[ViewInspection<'_>],
        frozen: bool,
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<bool> {
        self.check_thread()?;
        inspections(views, inspection)?;
        self.observe_preview_pressure()?;
        if !self.view_group_current(owner, views, frozen)? {
            return Ok(false);
        }
        let placements = placements(&self.scene, views);
        let g = self
            .graphics
            .as_mut()
            .ok_or("managed surface detached during retained presentation")?;
        let Some(output) = acquire_preview_surface(g)? else {
            return Ok(false);
        };
        let target = output.texture.create_view(&Default::default());
        let mut encoder = g.renderer.allocation_command_encoder_for(
            motion_render::gpu_commands::CommandStage::CompositionAndPresentation,
            &wgpu::CommandEncoderDescriptor {
                label: Some("retained managed views"),
            },
        )?;
        g.views
            .colors
            .as_ref()
            .ok_or_else(|| refuse("surface_present", "prepared display owner is missing"))?
            .encode(
                &g.renderer,
                &mut encoder,
                &output.texture,
                &placements,
                false,
            )?;
        overlay(&g.renderer, &mut encoder, &target);
        g.renderer
            .submit_allocated(encoder.finish(), |ledger| g.views.collect(ledger))?;
        g.renderer.check_health()?;
        g.renderer.queue.present(output);
        self.presented = self.presented.saturating_add(1);
        Ok(true)
    }

    pub(super) fn render_color_views(
        &mut self,
        owner: u64,
        frame: f64,
        views: &[View<'_>],
        inspection: &[ViewInspection<'_>],
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<bool> {
        self.check_thread()?;
        inspections(views, inspection)?;
        if !frame.is_finite() || frame < 0. || frame >= f64::from(self.engine.project().frames) {
            return Err(refuse("surface_request", "invalid frame").into());
        }
        let Some(g) = &self.graphics else {
            return Ok(false);
        };
        validate_surface(views, [g.config.width, g.config.height])?;
        if self.recorder.is_some() {
            return Err(refuse(
                "surface_recording",
                "managed frame recording requires its explicit measurement consumer",
            )
            .into());
        }
        let began = Instant::now();
        self.frame = frame;
        self.observe_preview_pressure()?;
        let mut key = self
            .views_key(owner, frame, views, None)?
            .ok_or("managed surface missing")?;
        let colors = self
            .colors
            .as_ref()
            .ok_or("managed project preparation missing")?;
        let state = self
            .graphics
            .as_ref()
            .and_then(|g| g.views.colors.as_ref())
            .ok_or_else(|| refuse("surface_prepare", "prepared display owner is missing"))?;
        if !state.program_matches(colors) {
            return Err(refuse(
                "surface_prepare",
                "display owner does not match the installed project programs",
            )
            .into());
        }
        state.check_failure(&key)?;
        if state.published() == Some(&key) {
            return self.present_color_views(owner, views, inspection, false, overlay);
        }
        if !state.ready(&key) {
            self.render_attempts = self.render_attempts.saturating_add(1);
            self.cancel_pending_exposure()?;
            self.sample()?;
            key.view_revision = self.view_revision;
            let cameras: Vec<_> = views.iter().map(|view| view.camera).collect();
            let frames = self
                .colors
                .as_ref()
                .and_then(|colors| colors.authored.current())
                .ok_or_else(|| refuse("surface_sample", "sampled working frame is missing"))?
                .fork_views(&cameras)
                .map_err(RenderError::from)?;
            let g = self
                .graphics
                .as_mut()
                .ok_or("managed surface detached during rendering")?;
            g.renderer.device.poll(wgpu::PollType::Poll)?;
            let reserved = g.scratch.texture_bytes() + g.views.targets.texture_bytes();
            g.views
                .colors
                .as_mut()
                .ok_or_else(|| refuse("surface_render", "prepared display owner is missing"))?
                .render(&mut g.renderer, key.clone(), &frames, &key.sizes, reserved)?;
        }
        let placements = placements(&self.scene, views);
        let g = self
            .graphics
            .as_mut()
            .ok_or("managed surface detached during submission")?;
        let Some(output) = acquire_preview_surface(g)? else {
            return Ok(false);
        };
        let target = output.texture.create_view(&Default::default());
        let mut encoder = g.renderer.allocation_command_encoder_for(
            motion_render::gpu_commands::CommandStage::CompositionAndPresentation,
            &wgpu::CommandEncoderDescriptor {
                label: Some("managed independent views"),
            },
        )?;
        g.views
            .colors
            .as_ref()
            .ok_or_else(|| refuse("surface_present", "prepared display owner is missing"))?
            .encode(
                &g.renderer,
                &mut encoder,
                &output.texture,
                &placements,
                true,
            )?;
        overlay(&g.renderer, &mut encoder, &target);
        g.renderer
            .submit_allocated(encoder.finish(), |ledger| g.views.collect(ledger))?;
        g.renderer.check_health()?;
        g.views
            .colors
            .as_mut()
            .ok_or_else(|| refuse("surface_commit", "prepared display owner is missing"))?
            .commit()?;
        g.renderer.queue.present(output);
        self.presented = self.presented.saturating_add(1);
        self.last_cpu_us = began.elapsed().as_micros() as u64;
        self.last_presented_frame = Some(frame);
        self.last_presented_revision = self.engine.revision();
        self.last_presented_view_revision = self.view_revision;
        g.actual_preview_quality = Some(key.quality);
        Ok(true)
    }
}
