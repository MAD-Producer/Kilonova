//! Persistent exposure requests for the live preview. A pending poll owns its
//! images until completion or an explicit request change; failed requests do
//! not implicitly restart when the next Present arrives.
use super::*;
use crate::render_sampling::{needs_exposure, ExposureMedia, ExposureMediaPolicy};
use motion_render::{
    exposure_scene::{ExposureDocument, ExposureScenes},
    preview_memory::PreviewMemoryQuality,
    ExposureFrame, ExposurePoll, RenderError, ResourceRejection, TextureResourceLedger,
};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RequestKey {
    pub frame: f64,
    pub size: [u32; 2],
    pub preview: bool,
    pub quality: PreviewMemoryQuality,
    pub view: u64,
    pub surface: u64,
    pub surface_size: [u32; 2],
    pub viewport: [f32; 4],
    pub image: Option<[f32; 4]>,
    pub observer: Option<[f32; 16]>,
}

struct Failure {
    cause: String,
    rejection: Option<ResourceRejection>,
}
impl Failure {
    fn error(&self) -> RenderError {
        match self.rejection {
            Some(rejection) => RenderError::Resource {
                rejection,
                message: self.cause.clone(),
            },
            None => RenderError::Invalid(self.cause.clone()),
        }
    }
}
struct Request {
    key: RequestKey,
    task: Option<ExposureFrame<motion_core::ExpressionEvaluator>>,
    failure: Option<Failure>,
    cpu_us: u64,
    media_us: u64,
}
#[derive(Default)]
pub(crate) struct SamplingPreview {
    document_key: Option<(u64, String)>,
    document: Option<ExposureDocument>,
    request: Option<Request>,
}
impl SamplingPreview {
    pub(crate) fn required(&self) -> bool {
        self.document.is_some()
    }
    pub(super) fn synchronize(
        &mut self,
        project: &Project,
        revision: u64,
        videos: &mut VideoFrames,
    ) -> Result<bool> {
        if self
            .document_key
            .as_ref()
            .is_some_and(|(old, composition)| {
                *old == revision && composition == &project.composition_id
            })
        {
            return Ok(self.document.is_some());
        }
        self.cancel(videos)?;
        let document = if needs_exposure(project)? {
            Some(ExposureDocument::new(Arc::new(project.clone())).map_err(crate::Error::from)?)
        } else {
            None
        };
        self.document = document;
        self.document_key = Some((revision, project.composition_id.clone()));
        Ok(self.document.is_some())
    }
    pub(super) fn cancel(&mut self, videos: &mut VideoFrames) -> Result<()> {
        let had_request = self.request.take().is_some();
        if had_request {
            videos.cancel_pending_sampling()?;
        }
        Ok(())
    }
    /// Surface detach or the existing memory policy explicitly abandons the
    /// GPU request. Ordinary Present errors retain their terminal request.
    pub(super) fn discard_request(&mut self) {
        self.request = None;
    }
    pub(crate) fn ledger(&self) -> TextureResourceLedger {
        self.request
            .as_ref()
            .and_then(|r| r.task.as_ref())
            .map_or_else(
                TextureResourceLedger::default,
                ExposureFrame::texture_resource_ledger,
            )
    }
    pub(crate) fn status(&self) -> Value {
        let Some(request) = &self.request else {
            return Value::Null;
        };
        let progress = request.task.as_ref().map(ExposureFrame::progress);
        json!({"frame":request.key.frame,"width":request.key.size[0],"height":request.key.size[1],
            "state":if request.failure.is_some(){"failed"}else if request.task.as_ref().is_some_and(|t|t.target().complete()){"ready"}else{"pending"},
            "phase":progress.map(|p|format!("{:?}",p.stage)),"phaseIndex":progress.map(|p|p.phase),
            "phaseCount":progress.map(|p|p.phases),"object":progress.and_then(|p|p.receiver),
            "sampleFrame":progress.and_then(|p|p.sample_frame),"workUnits":progress.map(|p|p.work_units),
            "cpuPollUs":request.cpu_us,"mediaPrepareAndUploadUs":request.media_us,
            "error":request.failure.as_ref().map(|e|&e.cause)})
    }
    pub(super) fn cpu_us(&self) -> u64 {
        self.request.as_ref().map_or(0, |r| r.cpu_us)
    }
    fn media_us(&self) -> u64 {
        self.request.as_ref().map_or(0, |r| r.media_us)
    }

    pub(super) fn poll_into(
        &mut self,
        renderer: &mut Renderer,
        scratch: &mut PreviewFrameTargets,
        videos: &mut VideoFrames,
        proxies: &mut crate::preview_video::PreviewVideoCache,
        root: &std::path::Path,
        key: RequestKey,
        observer: Option<Observer>,
        slice: Duration,
    ) -> std::result::Result<ExposurePoll, RenderError> {
        let cancellation_error = if self.request.as_ref().is_none_or(|r| r.key != key) {
            let error = self.cancel(videos).err();
            self.request = Some(Request {
                key,
                task: None,
                failure: None,
                cpu_us: 0,
                media_us: 0,
            });
            error
        } else {
            None
        };
        let request = self
            .request
            .as_mut()
            .ok_or_else(|| RenderError::Invalid("preview exposure request missing".into()))?;
        if let Some(error) = &request.failure {
            return Err(error.error());
        }
        let began = Instant::now();
        let result = (|| {
            if let Some(error) = cancellation_error {
                return Err(RenderError::Invalid(format!(
                    "phase=cancel_previous: {error}"
                )));
            }
            let document = self
                .document
                .as_ref()
                .ok_or_else(|| RenderError::Invalid("preview exposure document missing".into()))?;
            if request.task.is_none() {
                let scenes = ExposureScenes::new(
                    document.clone(),
                    &document.composition_id,
                    request.key.frame,
                    request.key.preview,
                    observer,
                    motion_core::ExpressionEvaluator,
                )
                .map_err(|e| RenderError::Invalid(e.to_string()))?;
                request.task = Some(ExposureFrame::new(
                    renderer,
                    scenes,
                    request.key.size[0],
                    request.key.size[1],
                )?);
            }
            let task = request
                .task
                .as_mut()
                .ok_or_else(|| RenderError::Invalid("preview exposure task missing".into()))?;
            let mut media = ExposureMedia {
                project: document,
                root,
                videos,
                proxies,
                policy: ExposureMediaPolicy {
                    image_resolution: request.key.quality.image_resolution(),
                    asynchronous_images: request.key.quality.level == 0,
                    video_edge: request.key.quality.video_edge,
                },
            };
            let result = task.poll(renderer, slice, |renderer, scene| {
                let began = Instant::now();
                let result = media.prepare(renderer, scene);
                request.media_us = request.media_us.saturating_add(micros(began.elapsed()));
                result
            })?;
            if matches!(result, ExposurePoll::Ready(_)) {
                let target = scratch.prepare(renderer, request.key.size[0], request.key.size[1])?;
                renderer.resolve_exposure(task.target(), target)?;
            }
            Ok(result)
        })();
        request.cpu_us = request.cpu_us.saturating_add(micros(began.elapsed()));
        result.map_err(|error: RenderError| {
            let mut cause = format!(
                "preview exposure composition={} revision={:?} frame={} size={}x{} view={} surface={} phase=request: {error}",
                self.document_key.as_ref().map_or("unknown", |(_, composition)| composition.as_str()),
                self.document_key.as_ref().map(|(revision, _)| revision),
                request.key.frame,
                request.key.size[0],
                request.key.size[1],
                request.key.view,
                request.key.surface
            );
            if let Err(cleanup) = videos.cancel_pending_sampling() {
                cause.push_str(&format!("; cancel pending exposure reads also failed: {cleanup}"));
            }
            request.failure = Some(Failure {
                cause: cause.chars().take(2048).collect(),
                rejection: error.resource_rejection().copied(),
            });
            request.task = None;
            scratch.discard_candidate();
            match error.resource_rejection().copied() {
                Some(rejection) => RenderError::Resource {
                    rejection,
                    message: cause,
                },
                None => RenderError::Invalid(cause),
            }
        })
    }
}
fn micros(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

impl Session {
    pub(crate) fn cancel_pending_exposure(&mut self) -> Result<()> {
        self.sampling.cancel(&mut self.video_frames)
    }

    pub(super) fn render_exposure_with_view(
        &mut self,
        frame: f64,
        viewport: [f32; 4],
        image: Option<[f32; 4]>,
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<bool> {
        let began = Instant::now();
        let Some(g) = &mut self.graphics else {
            return Ok(false);
        };
        g.renderer
            .device
            .poll(wgpu::PollType::Poll)
            .map_err(|e| crate::Error::with_context("exposure preview GPU poll", e))?;
        g.renderer.check_health().map_err(crate::Error::from)?;
        if let Some(timer) = &mut g.timer {
            for timing in timer.collect().into_iter().flatten() {
                if let Some(recorder) = &mut self.recorder {
                    recorder.timing(timing);
                }
            }
        }
        let base = self.preview.render_dimensions(
            self.scene.width,
            self.scene.height,
            viewport[2].ceil() as u32,
            viewport[3].ceil() as u32,
        );
        let mut overlay = Some(overlay);
        for _ in 0..motion_render::preview_memory::PREVIEW_MEMORY_LEVELS {
            let quality = self
                .preview_memory
                .quality(self.preview.mode == PreviewMode::High);
            let (rw, rh) = quality.dimensions(base.0, base.1);
            let observer = self.observing.then(|| self.observer.clone());
            let key = RequestKey {
                frame,
                size: [rw, rh],
                preview: self.preview.mode != PreviewMode::High,
                quality,
                view: self.view_revision,
                surface: self.surface_epoch,
                surface_size: [g.config.width, g.config.height],
                viewport,
                image,
                observer: observer.as_ref().map(|o| {
                    o.pose(self.scene.width, self.scene.height)
                        .view_projection
                        .to_cols_array()
                }),
            };
            let poll_began = Instant::now();
            let result = self.sampling.poll_into(
                &mut g.renderer,
                &mut g.scratch,
                &mut self.video_frames,
                &mut self.preview_video,
                &self.root,
                key,
                observer,
                Duration::from_millis(8),
            );
            let poll_us = micros(poll_began.elapsed());
            match result {
                Ok(ExposurePoll::Pending(_)) => {
                    let retained = g.scratch.valid().then_some(viewport);
                    let painted = present_chrome(g, retained, image, overlay.take().unwrap())?;
                    self.presented += u64::from(painted);
                    return Ok(false);
                }
                Err(error) => {
                    if error.resource_rejection().is_none() {
                        return Err(error.into());
                    }
                    self.sampling.discard_request();
                    if preview_refused(
                        g,
                        &mut self.preview_memory,
                        error,
                        self.preview_memory_clock.elapsed().as_millis() as u64,
                    )? {
                        continue;
                    }
                    break;
                }
                Ok(ExposurePoll::Ready(_)) => {}
            }
            let acquiring = Instant::now();
            let Some(output) = acquire_preview_surface(g)? else {
                return Ok(false);
            };
            let acquire_us = micros(acquiring.elapsed());
            let candidate = g
                .scratch
                .prepared()
                .ok_or("completed exposure preview target missing")?
                .view
                .clone();
            g.presenter.set_admitted_source(&g.renderer.device, &candidate);
            let view = output.texture.create_view(&Default::default());
            let mut encoder =
                g.renderer
                    .allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Motion Studio exposure presentation"),
                    })?;
            g.presenter.encode_view(
                &g.renderer.queue,
                &mut encoder,
                &view,
                None,
                if image.is_some() {
                    viewport
                } else {
                    preview_viewport(viewport, [self.scene.width, self.scene.height])
                },
                image,
                self.scene.background,
            );
            overlay.take().unwrap()(&g.renderer, &mut encoder, &view);
            let submitting = Instant::now();
            g.renderer.submit_allocated(encoder.finish(), |ledger| {
                g.scratch.collect_texture_resources(ledger)
            })?;
            let submit_us = micros(submitting.elapsed());
            if let Err(error) = g.renderer.check_health() {
                g.presenter.set_admitted_source(&g.renderer.device, &g.scratch.view);
                return Err(error.into());
            }
            g.scratch.commit_submitted();
            g.actual_preview_quality = Some(quality);
            g.last_canvas = [self.scene.width, self.scene.height];
            g.last_background = self.scene.background;
            let presenting = Instant::now();
            g.renderer.queue.present(output);
            let present_call_us = micros(presenting.elapsed());
            self.presented += 1;
            let render_wall_us = micros(began.elapsed());
            self.last_cpu_us = self
                .sampling
                .cpu_us()
                .saturating_add(render_wall_us.saturating_sub(poll_us))
                .saturating_sub(acquire_us)
                .saturating_sub(present_call_us);
            self.video_prepare_us = 0;
            self.video_upload_us = 0;
            if let Some(recorder) = &mut self.recorder {
                recorder.record(FrameMeasurement {
                    sequence: self.presented,
                    frame,
                    elapsed_us: recorder.elapsed_us(),
                    cpu_prepare_us: self.last_cpu_us,
                    acquire_us,
                    submit_us,
                    present_call_us,
                    render_wall_us,
                    render_width: rw,
                    render_height: rh,
                    preview_fps: self.preview.tier().fps(),
                    gpu: None,
                    resources: Some(motion_render::FrameResources {
                        textures: g.renderer.texture_memory_breakdown(),
                        vector_upload: g.renderer.vector_upload_info()?,
                        preview_target_bytes: g.scratch.texture_bytes(),
                        exposure_target_bytes: g.renderer.exposure_texture_bytes(),
                        exposure_media_us: self.sampling.media_us(),
                        cpu_video_cache_bytes: self.video_frames.cache_bytes()? as u64,
                        cpu_video_proxy_bytes: self.preview_video.bytes() as u64,
                        video_prepare_us: 0,
                        video_upload_us: 0,
                        image_upload_bytes_total: g.renderer.image_upload_bytes,
                        video_upload_bytes_total: g.renderer.video_upload_bytes,
                        pipeline_builds_total: g.renderer.effect_pipeline_builds(),
                        memory_quality_level: quality.level,
                        resource_refusals_total: self.preview_memory.refusal_count,
                    }),
                });
            }
            self.last_presented_frame = Some(frame);
            self.last_presented_revision = self.engine.revision();
            self.last_presented_view_revision = self.view_revision;
            let previous_tier = self.preview.tier();
            self.preview.observe_render(
                self.last_cpu_us,
                None,
                acquire_us.saturating_add(present_call_us),
            );
            if previous_tier != self.preview.tier() {
                self.view_revision += 1;
            }
            self.preview_memory.succeeded();
            self.last_error = None;
            return Ok(true);
        }
        self.preview_memory_held = g.scratch.valid();
        let held = present_held(g, viewport, image, overlay.take().unwrap())?;
        self.presented += u64::from(held);
        Ok(held)
    }
}

#[cfg(test)]
mod tests;
