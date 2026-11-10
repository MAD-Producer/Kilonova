//! Composition-only native frames on an independent, worker-owned renderer.
//!
//! This is the full-resolution authored-camera producer, not the live preview
//! scheduler or its cache consumer. It never acquires a window surface, draws
//! chrome, converts to SDR, or moves a Session/GPU renderer between threads.
use super::*;
use motion_effects::image_contract::{ImageContract, ImageMeaning};
use motion_render::{CapturedImage, CompositionFrameContext, CompositionFrameKey, Renderer, Scene};
use std::{
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

pub mod coordinator;
pub mod disk;
mod key;
pub mod source_watch;
mod sources;
pub mod worker;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Freeze,
    ContentKey,
    Initialize,
    Sample,
    Images,
    Video,
    Draw,
    Readback,
    DiskOpen,
    DiskRead,
    DiskWrite,
}
impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Freeze => "freeze",
            Self::ContentKey => "content_key",
            Self::Initialize => "initialize",
            Self::Sample => "sample",
            Self::Images => "images",
            Self::Video => "video",
            Self::Draw => "draw",
            Self::Readback => "readback",
            Self::DiskOpen => "disk_open",
            Self::DiskRead => "disk_read",
            Self::DiskWrite => "disk_write",
        }
    }
}
fn enter(
    phase: &mut Phase,
    next: Phase,
    control: &ExportControl,
    progress: &mut impl FnMut(Phase),
) -> Result<()> {
    *phase = next;
    progress(next);
    control.check()
}

/// Transfer this immutable source plan to the background worker. It contains
/// metadata only, not open source handles, a Session or a GPU renderer.
pub struct PrerenderPlan {
    sources: sources::CapturedSources,
    adapter: Option<wgpu::Adapter>,
    document_generation: u64,
    device_generation: u64,
    max_frame_bytes: u64,
    disk: Option<disk::Config>,
}
impl PrerenderPlan {
    /// Optional storage for a background Worker. Selecting the policy does no
    /// IO; standalone snapshot preparation/export does not open a disk cache.
    pub fn with_disk_cache(mut self, config: Option<disk::Config>) -> Self {
        self.disk = config;
        self
    }
    pub fn capture(
        session: &Session,
        document_generation: u64,
        device_generation: u64,
        request: u64,
        max_frame_bytes: u64,
        control: &ExportControl,
    ) -> Result<Self> {
        session.check_thread()?;
        log::info!("start #{request} prerender_capture document={document_generation} device={device_generation}");
        let began = Instant::now();
        let result = (|| {
            control.check()?;
            let sources = sources::CapturedSources::capture(session, request)?;
            Ok(Self {
                sources,
                adapter: session
                    .graphics
                    .as_ref()
                    .map(|g| g.renderer.adapter.clone()),
                document_generation,
                device_generation,
                max_frame_bytes,
                disk: None,
            })
        })();
        observe_result(request, "capture", began, &result);
        result
    }
    pub fn prepare(
        self,
        control: &ExportControl,
        progress: impl FnMut(Phase),
    ) -> Result<PreparedPrerender> {
        // Standalone prepared snapshots own frozen bytes independently of the
        // live project. Cache owners opt into live-source invalidation below.
        self.prepare_inner(control, progress, None)
    }
    fn prepare_observed(
        self,
        control: &ExportControl,
        progress: impl FnMut(Phase),
        mut observed: impl FnMut(source_watch::Observation) -> Result<()>,
    ) -> Result<PreparedPrerender> {
        self.prepare_inner(control, progress, Some(&mut observed))
    }
    fn prepare_inner(
        self,
        control: &ExportControl,
        mut progress: impl FnMut(Phase),
        mut observed: Option<&mut dyn FnMut(source_watch::Observation) -> Result<()>>,
    ) -> Result<PreparedPrerender> {
        let request = self.sources.observation_request;
        log::info!(
            "start #{request} prerender_prepare document={} device={}",
            self.document_generation,
            self.device_generation
        );
        let began = Instant::now();
        let mut phase = Phase::Freeze;
        let result = (|| {
            enter(&mut phase, Phase::Freeze, control, &mut progress)?;
            let watch = if let Some(observed) = &mut observed {
                let watch = self.sources.watch()?;
                observed(watch.observation())?;
                Some(watch)
            } else {
                None
            };
            let _lease = watch
                .as_ref()
                .map(|watch| source_lease(&watch.observation(), control))
                .transpose()?;
            let frozen = self.sources.open(control)?.freeze(control)?;
            enter(&mut phase, Phase::ContentKey, control, &mut progress)?;
            PreparedPrerender::new(
                frozen,
                self.document_generation,
                self.device_generation,
                self.max_frame_bytes,
                control,
            )
            .map(|mut prepared| {
                prepared.adapter = self.adapter;
                prepared.watch = watch;
                prepared
            })
        })();
        observe_result(request, phase.name(), began, &result);
        result
    }
}

/// Fully owned bytes and an authoritative native-output context. Generation
/// tokens remain the owner's responsibility; they are not content fingerprints.
pub struct PreparedPrerender {
    frozen: FrozenMovie,
    adapter: Option<wgpu::Adapter>,
    context: CompositionFrameContext,
    max_frame_bytes: u64,
    watch: Option<source_watch::SourceWatch>,
}
impl PreparedPrerender {
    fn new(
        frozen: FrozenMovie,
        document: u64,
        device: u64,
        max_frame_bytes: u64,
        control: &ExportControl,
    ) -> Result<Self> {
        let bytes = motion_render::image_formats::color_storage(frozen.project.working_precision)
            .capacity(frozen.project.width, frozen.project.height)
            .ok_or("prerender frame size overflow")?;
        if bytes > max_frame_bytes {
            return Err(format!(
                "prerender native frame needs {bytes} CPU bytes, limit={max_frame_bytes}"
            ));
        }
        let context = key::context(&frozen, document, device, control)?;
        Ok(Self {
            frozen,
            adapter: None,
            context,
            max_frame_bytes,
            watch: None,
        })
    }
    pub fn context(&self) -> CompositionFrameContext {
        self.context
    }
    pub fn frame_key(&self, frame: f64) -> Result<CompositionFrameKey> {
        if !frame.is_finite() || frame < 0. || frame >= f64::from(self.frozen.project.frames) {
            return Err("prerender frame is outside the frozen composition".into());
        }
        CompositionFrameKey::new(
            self.context,
            frame,
            self.frozen.project.width,
            self.frozen.project.height,
            motion_render::image_formats::native_format(
                motion_render::image_formats::color_storage(self.frozen.project.working_precision),
            ),
        )
        .map_err(|e| e.to_string())
    }
}

fn source_lease(
    observation: &source_watch::Observation,
    control: &ExportControl,
) -> Result<source_watch::Lease> {
    match observation.attach(control.clone())? {
        Some(lease) => Ok(lease),
        None => {
            control.cancel();
            Err("movie export cancelled".into())
        }
    }
}

/// Native bytes, not PNG/display values. The coordinator must bound retained
/// completions and reject obsolete generations before foreground installation.
pub struct CompletedFrame {
    key: CompositionFrameKey,
    image: CapturedImage,
}
impl CompletedFrame {
    pub fn key(&self) -> CompositionFrameKey {
        self.key
    }
    pub fn image(&self) -> &CapturedImage {
        &self.image
    }
    pub fn into_image(self) -> CapturedImage {
        self.image
    }
}

/// Create and use on one background thread. The Rc marker deliberately prevents
/// transferring a live GPU renderer; PreparedPrerender is the transferable input.
pub struct NativePrerender {
    input: PreparedPrerender,
    renderer: Renderer,
    videos: motion_host::video_frames::VideoFrames,
    proxies: motion_host::preview_video::PreviewVideoCache,
    _owner: PhantomData<Rc<()>>,
}
impl NativePrerender {
    pub fn open(input: PreparedPrerender, control: &ExportControl) -> Result<Self> {
        let request = input.frozen.observation_request;
        log::info!(
            "start #{request} prerender_initialize document={} device={}",
            input.context.document_generation,
            input.context.device_generation
        );
        let began = Instant::now();
        let result = (|| {
            control.check()?;
            let _source_lease = input
                .watch
                .as_ref()
                .map(|watch| source_lease(&watch.observation(), control))
                .transpose()?;
            let p = &input.frozen.project;
            input
                .frozen
                .depth
                .validate_project(p)
                .map_err(|e| e.to_string())?;
            if input.frozen.depth.layer_count() > 0
                && motion_host::render_sampling::needs_exposure(p)?
            {
                return Err(
                    "frozen calibrated depth with exposure sampling is not supported".into(),
                );
            }
            let mut renderer = match &input.adapter {
                Some(adapter) => pollster::block_on(Renderer::on_adapter_with_working_precision(
                    adapter.clone(),
                    p.working_precision,
                    input.frozen.allocation_budget.clone(),
                )),
                None => pollster::block_on(Renderer::headless_with_working_precision(
                    p.working_precision,
                    input.frozen.allocation_budget.clone(),
                )),
            }
            .map_err(|e| format!("prerender GPU initialization: {e}"))?;
            control.check()?;
            renderer.set_effect_registry(input.frozen.registry.clone());
            renderer
                .set_memory_policy(input.frozen.memory_policy)
                .map_err(|e| e.to_string())?;
            renderer
                .configure_assets(p, input.frozen.assets.path())
                .map_err(|e| e.to_string())?;
            let videos = motion_host::video_frames::VideoFrames::new(input.frozen.platform.clone());
            Ok(Self {
                input,
                renderer,
                videos,
                proxies: Default::default(),
                _owner: PhantomData,
            })
        })();
        observe_result(request, "initialize", began, &result);
        result
    }
    pub fn context(&self) -> CompositionFrameContext {
        self.input.context
    }
    pub fn readback_info(&self) -> motion_render::ImageReadbackInfo {
        self.renderer.image_readback_info()
    }
    /// Phase publication precedes each potentially blocking operation. The owner
    /// can cancel from another thread; no phase callback runs under an observer lock.
    pub fn render(
        &mut self,
        frame: f64,
        control: &ExportControl,
        mut progress: impl FnMut(Phase),
    ) -> Result<CompletedFrame> {
        let request = self.input.frozen.observation_request;
        let began = Instant::now();
        log::info!(
            "start #{request} prerender_frame frame={frame} document={} device={}",
            self.input.context.document_generation,
            self.input.context.device_generation
        );
        let mut phase = Phase::Sample;
        let result = self.render_inner(frame, control, &mut phase, &mut progress);
        if let Err(error) = &result {
            if error != "movie export cancelled" {
                log::error!(
                    "prerender #{request} frame={frame} phase={} elapsed_ms={} cause={error}",
                    phase.name(),
                    began.elapsed().as_millis()
                );
            }
        }
        log::debug!(
            "done #{request} prerender_frame frame={frame} phase={} elapsed_ms={} success={}",
            phase.name(),
            began.elapsed().as_millis(),
            result.is_ok()
        );
        result
    }
    fn render_inner(
        &mut self,
        frame: f64,
        control: &ExportControl,
        phase: &mut Phase,
        progress: &mut impl FnMut(Phase),
    ) -> Result<CompletedFrame> {
        let _source_lease = self
            .input
            .watch
            .as_ref()
            .map(|watch| source_lease(&watch.observation(), control))
            .transpose()?;
        let key = self.input.frame_key(frame)?;
        let frozen = &self.input.frozen;
        let p = &frozen.project;
        enter(phase, Phase::Sample, control, progress)?;
        let image = if motion_host::render_sampling::needs_exposure(p)? {
            let document =
                motion_render::exposure_scene::ExposureDocument::new(Arc::new(p.clone()))
                    .map_err(|e| e.to_string())?;
            let scenes = motion_render::exposure_scene::ExposureScenes::new(
                document,
                &p.composition_id,
                frame,
                false,
                None,
                motion_core::ExpressionEvaluator,
            )
            .map_err(|e| e.to_string())?;
            let mut task =
                motion_render::ExposureFrame::new(&mut self.renderer, scenes, p.width, p.height)
                    .map_err(|e| e.to_string())?;
            let mut media = motion_host::render_sampling::ExposureMedia {
                project: p,
                root: frozen.assets.path(),
                videos: &mut self.videos,
                proxies: &mut self.proxies,
                policy: motion_host::render_sampling::ExposureMediaPolicy::FULL,
            };
            let mut last = task.progress();
            let mut changed = Instant::now();
            loop {
                enter(phase, Phase::Draw, control, progress)?;
                match task
                    .poll(
                        &mut self.renderer,
                        Duration::from_millis(8),
                        |renderer, scene| media.prepare(renderer, scene),
                    )
                    .map_err(|e| e.to_string())?
                {
                    motion_render::ExposurePoll::Ready(_) => break,
                    motion_render::ExposurePoll::Pending(current) => {
                        if current != last {
                            last = current;
                            changed = Instant::now();
                        }
                        if changed.elapsed() > Duration::from_secs(30) {
                            return Err(format!(
                                "prerender exposure progress={last:?}: media preparation timed out"
                            ));
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
            }
            enter(phase, Phase::Readback, control, progress)?;
            self.renderer
                .capture_exposure_image(task.target())
                .map_err(|e| e.to_string())?
        } else {
            let mut scene = Scene::new(p).map_err(|e| e.to_string())?;
            scene.set_model_source_catalog(self.renderer.model_source_catalog());
            scene
                .sample(p, frame, None, &motion_core::ExpressionEvaluator)
                .map_err(|e| e.to_string())?;
            let depth = frozen
                .depth
                .verify_sources_with_platform(
                    frozen.assets.path(),
                    frozen.platform.clone(),
                    &|| control.check().map_err(Into::into),
                )
                .map_err(|e| e.to_string())?;
            depth
                .attach_checked(&mut scene, &|| control.check().map_err(Into::into))
                .map_err(|e| e.to_string())?;
            enter(phase, Phase::Images, control, progress)?;
            if !self
                .renderer
                .prepare_scene_assets(
                    &scene,
                    motion_render::image_resources::Resolution::Full,
                    false,
                )
                .map_err(|e| e.to_string())?
            {
                return Err("prerender full-resolution image preparation is incomplete; frame not published".into());
            }
            enter(phase, Phase::Video, control, progress)?;
            let began = Instant::now();
            let decoded = loop {
                control.check()?;
                if let Some(frames) =
                    self.videos
                        .prepare_scene_exact(p, frozen.assets.path(), &scene)?
                {
                    break frames;
                }
                if began.elapsed() > Duration::from_secs(30) {
                    return Err("prerender exact video decode timed out".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            };
            for (object, decoded) in decoded {
                control.check()?;
                let source = scene
                    .video_layers()
                    .into_iter()
                    .find(|l| l.id == object)
                    .and_then(|l| l.video.as_ref())
                    .ok_or("prerender video instance missing")?
                    .asset;
                decoded.upload(&mut self.renderer, object, source)?;
            }
            enter(phase, Phase::Draw, control, progress)?;
            let contract = ImageContract::new(
                ImageMeaning::Color {
                    space: motion_effects::WorkingSpace::Linear,
                    alpha: motion_effects::AlphaMode::Premultiplied,
                },
                p.working_precision,
            );
            let target = self
                .renderer
                .image_capture_target(p.width, p.height, contract)
                .map_err(|e| e.to_string())?;
            self.renderer
                .draw(&scene, target.view(), p.width, p.height)
                .map_err(|e| e.to_string())?;
            enter(phase, Phase::Readback, control, progress)?;
            self.renderer
                .read_image_target(&target)
                .map_err(|e| e.to_string())?
        };
        control.check()?;
        if image.pixels.len() as u64 > self.input.max_frame_bytes {
            return Err("prerender completion exceeds admitted CPU frame limit".into());
        }
        Ok(CompletedFrame { key, image })
    }
}
impl Drop for NativePrerender {
    fn drop(&mut self) {
        if let Err(error) = self.videos.finish() {
            log::error!(
                "prerender #{} phase=decoder_cleanup cause={error}",
                self.input.frozen.observation_request
            );
        }
    }
}
fn observe_result<T>(request: u64, phase: &str, began: Instant, result: &Result<T>) {
    if let Err(error) = result {
        if error != "movie export cancelled" {
            log::error!(
                "prerender #{request} phase={phase} elapsed_ms={} cause={error}",
                began.elapsed().as_millis()
            );
        }
    }
    log::debug!(
        "done #{request} prerender phase={phase} elapsed_ms={} success={}",
        began.elapsed().as_millis(),
        result.is_ok()
    );
}

#[cfg(test)]
mod tests;
