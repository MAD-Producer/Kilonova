//! Frozen full-resolution PNG export, independent of the preview renderer.
use super::*;
use motion_render::{image_resources::Resolution, Renderer, Scene};
use std::time::{Duration, Instant};

pub struct FrameFreezePlan {
    sources: MovieFreezePlan,
    frame: f64,
    output: PathBuf,
    platform: Arc<dyn motion_host::Platform>,
}

impl FrameFreezePlan {
    /// Attach the engine request before this immutable plan leaves the worker.
    pub fn with_observation_request(mut self, request: u64) -> Self {
        self.sources = self.sources.with_observation_request(request);
        self
    }

    /// Capture immutable model/effect state and source handles on the session
    /// worker. Copy, decode, render and publication run on the export worker.
    pub fn capture(
        session: &Session,
        output: Option<PathBuf>,
        control: &ExportControl,
    ) -> Result<Self> {
        session.check_thread()?;
        let frame = session.frame;
        if !frame.is_finite() || frame < 0. || frame >= f64::from(session.engine.project().frames) {
            return Err("PNG capture frame is invalid".into());
        }
        let mut sources = MovieFreezePlan::new_with_audio(
            session.engine.snapshot(),
            &session.root,
            session.effects.registry.clone(),
            session.effects.memory_policy(),
            session.engine.revision(),
            control,
            false,
        )?;
        sources.allocation_budget = session.allocation_budget().clone();
        let output = output.unwrap_or_else(|| {
            session
                .root
                .join("exports")
                .join(format!("frame-{:06}.png", frame.floor() as u32))
        });
        Ok(Self {
            sources,
            frame,
            output,
            platform: session.platform().clone(),
        })
    }

    pub fn export(self, control: &ExportControl) -> Result<serde_json::Value> {
        let frame = self.frame;
        let revision = self.sources.revision;
        let output = self.output.clone();
        let budget = self.sources.allocation_budget.clone();
        let request = self.sources.observation_request;
        allocation_observation::run(
            &budget, request, motion_render::FrameOperation::Export, Some(frame),
            || self.export_inner(control),
        ).map_err(|error| {
            if error == "movie export cancelled" {
                "PNG export cancelled".into()
            } else {
                format!(
                    "PNG export revision={revision} frame={frame} path={}: {error}",
                    output.display()
                )
            }
        })
    }

    fn export_inner(self, control: &ExportControl) -> Result<serde_json::Value> {
        control.check()?;
        let frozen = self.sources.freeze(control)?;
        let output = export_destination_for(&self.output, &frozen.source_paths, "PNG")?;
        let temporary = tempfile::Builder::new()
            .prefix(".motion-frame-")
            .suffix(".png")
            .tempfile_in(output.parent().ok_or("PNG output has no parent")?)
            .map_err(|e| format!("create PNG temporary: {e}"))?;
        let p = &frozen.project;
        control.check()?;
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(p.working_precision, frozen.allocation_budget.clone()))
            .map_err(|e| format!("PNG GPU initialization: {e}"))?;
        renderer.set_effect_registry(frozen.registry.clone());
        renderer
            .set_memory_policy(frozen.memory_policy)
            .map_err(|e| e.to_string())?;
        renderer
            .configure_assets(p, frozen.assets.path())
            .map_err(|e| e.to_string())?;
        let mut videos = motion_host::video_frames::VideoFrames::new(self.platform);
        let mut proxies = motion_host::preview_video::PreviewVideoCache::default();
        let pixels =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Vec<u8>> {
                let target = renderer.sdr_capture_target(p.width, p.height, motion_render::OutputTransform::SDR_CLIP)
                    .map_err(|e| format!("PNG SDR output allocation: {e}"))?;
                if motion_host::render_sampling::needs_exposure(p)? {
                    let document =
                        motion_render::exposure_scene::ExposureDocument::new(Arc::new(p.clone()))
                            .map_err(|e| e.to_string())?;
                    super::render_exposure_frame(
                        &mut renderer,
                        &mut videos,
                        &mut proxies,
                        p,
                        document,
                        frozen.assets.path(),
                        self.frame,
                        control,
                        &target,
                    )
                } else {
                    let mut scene = Scene::new(p).map_err(|error| error.to_string())?;
                    scene
                        .sample(p, self.frame, None, &motion_core::ExpressionEvaluator)
                        .map_err(|e| e.to_string())?;
                    renderer
                        .prepare_scene_assets(&scene, Resolution::Full, false)
                        .map_err(|e| e.to_string())?;
                    let began = Instant::now();
                    let decoded = loop {
                        control.check()?;
                        if let Some(frames) =
                            videos.prepare_scene_exact(p, frozen.assets.path(), &scene)?
                        {
                            break frames;
                        }
                        if began.elapsed() > Duration::from_secs(30) {
                            return Err("phase=video_decode: PNG frame decode timed out".into());
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    };
                    for (object, decoded) in decoded {
                        let source = scene
                            .video_layers()
                            .into_iter()
                            .find(|l| l.id == object)
                            .and_then(|l| l.video.as_ref())
                            .ok_or("PNG video instance missing")?
                            .asset;
                        decoded.upload(&mut renderer, object, source)?;
                    }
                    renderer
                        .capture_sdr(&scene, &target)
                        .map(|value| value.0.pixels)
                        .map_err(|e| format!("PNG render: {e}"))
                }
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "PNG render panic: {}",
                    motion_media::panic_reason(payload)
                ))
            });
        let cleanup = videos.finish();
        let pixels = match (pixels, cleanup) {
            (Ok(pixels), Ok(())) => pixels,
            (Err(primary), Ok(())) => return Err(primary),
            (Ok(_), Err(error)) => return Err(format!("PNG decoder cleanup: {error}")),
            (Err(primary), Err(error)) => {
                return Err(format!("{primary}; PNG decoder cleanup: {error}"))
            }
        };
        control.check()?;
        control.set_progress(0.9);
        image::save_buffer_with_format(
            temporary.path(),
            &pixels,
            p.width,
            p.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|e| format!("encode PNG: {e}"))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|e| format!("sync PNG: {e}"))?;
        let decoded = image::open(temporary.path())
            .map_err(|e| format!("verify PNG: {e}"))?
            .into_rgba8();
        if decoded.dimensions() != (p.width, p.height) || decoded.as_raw() != &pixels {
            return Err("PNG verification differs from completed render".into());
        }
        // Match movie publication: successful cancellation cannot race rename.
        {
            let mut state = control
                .0
                .lock()
                .map_err(|_| "export cancellation state poisoned")?;
            if state.cancelled {
                return Err("movie export cancelled".into());
            }
            temporary
                .persist(&output)
                .map_err(|e| format!("publish PNG: {}", e.error))?;
            state.published = true;
        }
        control.set_progress(1.);
        Ok(
            serde_json::json!({"path":output,"width":p.width,"height":p.height,
            "frame":self.frame,"revision":frozen.revision}),
        )
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod precision_tests;
