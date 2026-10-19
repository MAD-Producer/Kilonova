//! Editing session lifecycle, GPU resources and frame sampling.
//!
//! This is the portable replacement for the Android-only `Session` that used to
//! live in `motion-android`. It owns the engine, the sampled scene, the effect
//! plan builder, media jobs and the preview policy; the host supplies only
//! surface creation and media decoding through [`crate::platform::Platform`].
use crate::platform::{Platform, SurfaceTarget};
use crate::video_frames::VideoFrames;
use motion_core::{Engine, Observer, Project};
use motion_render::Scene;
use motion_media::{AudioJobs, AudioMixer, Limits, PackageJobs, VideoJobs};
use motion_render::{
    FrameMeasurement, FrameRecorder, GpuTimer, Presenter, PreviewMode, PreviewPolicy, PreviewFrameTargets,
    Renderer,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, ThreadId},
    time::Instant,
};

pub type Result<T> = std::result::Result<T, String>;

mod last_error_observation;
mod resource_observation;
mod sampling;
pub(crate) mod metadata_observation;
#[cfg(test)]
mod metadata_observation_tests;
#[cfg(test)]
mod resource_observation_tests;
#[cfg(test)]
mod last_error_observation_tests;

static NEXT: AtomicI64 = AtomicI64::new(1);
static SESSIONS: OnceLock<Mutex<HashMap<i64, Session>>> = OnceLock::new();

/// Monotonic counter shared by session handles and plugin editor tokens.
pub fn next_token() -> i64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn sessions() -> &'static Mutex<HashMap<i64, Session>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Live GPU resources bound to one window.
pub struct Graphics {
    pub surface: wgpu::Surface<'static>,
    pub renderer: Renderer,
    pub scratch: PreviewFrameTargets,
    pub actual_preview_quality: Option<motion_render::preview_memory::PreviewMemoryQuality>,
    last_canvas: [u32; 2],
    last_background: [f32; 4],
    pub presenter: Presenter,
    pub config: wgpu::SurfaceConfiguration,
    pub timer: Option<GpuTimer>,
    /// Keeps the wgpu instance alive for as long as its surface.
    _instance: wgpu::Instance,
}

pub struct Session {
    pub composition_contexts: HashMap<String, CompositionContext>,
    pub composition_bundle: Vec<u8>,
    pub media_compositions: HashMap<String, String>,
    pub video_jobs: VideoJobs,
    pub video_frames: VideoFrames,
    pub preview_inputs: crate::preview_inputs::PreviewInputReader,
    pub audio_jobs: AudioJobs,
    pub package_jobs: PackageJobs,
    pub audio_mixer: Option<(u64, AudioMixer)>,
    pub audio_pcm: Vec<f32>,
    pub font_store: Option<motion_media::fonts::FontStore>,
    pub engine: Engine,
    pub scene: Scene,
    pub geometry: motion_render::PlaneCompositor,
    pub observer: Observer,
    pub observing: bool,
    pub frame: f64,
    pub root: PathBuf,
    pub graphics: Option<Graphics>,
    pub owner: ThreadId,
    pub presented: u64,
    pub last_cpu_us: u64,
    pub render_attempts: u64,
    pub video_pending_attempts: u64,
    pub video_prepare_us: u64,
    pub video_upload_us: u64,
    pub last_error: Option<String>,
    last_error_observation: last_error_observation::Observation,
    pub(crate) audio_snapshot_error: std::cell::RefCell<Option<String>>,
    pub last_presented_frame: Option<f64>,
    pub last_presented_revision: u64,
    pub view_revision: u64,
    pub last_presented_view_revision: u64,
    pub surface_epoch: u64,
    pub preview: PreviewPolicy,
    pub preview_memory: motion_render::preview_memory::PreviewMemoryPolicy,
    pub preview_video: crate::preview_video::PreviewVideoCache,
    pub(crate) sampling: sampling::SamplingPreview,
    pub preview_memory_held: bool,
    pub(crate) preview_memory_clock: Instant,
    memory_context: Option<(u64, u64, u32, u32)>,
    pub recorder: Option<FrameRecorder>,
    pub(crate) resource_tracking: bool,
    pub effects: motion_render::effect_plan::PlanBuilder,
    pub plugin_root: PathBuf,
    pub editor: Option<motion_core::plugin_editor::PluginEditorSession>,
    pub editor_token: String,
    pub(crate) editor_activity: Option<Instant>,
    pub(crate) editor_recovery: Option<Value>,
    pub editor_renderer: Option<Renderer>,
    pub editor_target: Option<motion_render::CaptureTarget>,
    platform: Arc<dyn Platform>,
}

/// Per-composition UI context: playhead, selection and timeline view state.
#[derive(Clone, Default)]
pub struct CompositionContext {
    pub frame: f64,
    pub selection: Vec<u64>,
    pub timeline: Value,
    pub path: Vec<String>,
}

impl Session {
    pub fn new(project: Project, root: PathBuf, platform: Arc<dyn Platform>) -> Result<Self> {
        let engine = Engine::new(project).map_err(|e| e.to_string())?;
        let project = engine.project();
        let mut scene = Scene::new(project);
        let initial_error = match scene.sample(project, 0.0, None, &motion_core::ExpressionEvaluator) {
            Ok(()) => None,
            Err(error @ motion_core::Error::Expression { .. }) => {
                let mut base = project.clone();
                for expression in &mut base.expressions {
                    expression.enabled = false;
                }
                scene.sample(&base, 0.0, None, &motion_core::ExpressionEvaluator).map_err(|e| e.to_string())?;
                Some(error.to_string())
            }
            Err(error) => return Err(error.to_string()),
        };
        let observer = Observer::new(project.width, project.height);
        let plugin_root = root
            .parent()
            .ok_or("project directory has no parent")?
            .join("plugins");
        let mut registry=motion_effects::Registry::load(&plugin_root).map_err(|e|e.to_string())?;
        crate::node_effects::restore(project,&mut registry)?;
        let effects = motion_render::effect_plan::PlanBuilder::new(registry)?;
        let video_jobs = VideoJobs::with_audio_decoder(root.clone(), audio_decoder(&platform))?;
        let audio_jobs =
            AudioJobs::with_decoder(root.clone(), Limits::default(), audio_decoder(&platform))?;
        let mut video_frames = VideoFrames::new(platform.clone());
        let mut sampling = sampling::SamplingPreview::default();
        sampling.synchronize(project,engine.revision(),&mut video_frames)?;
        Ok(Self {
            composition_contexts: Default::default(),
            composition_bundle: Vec::new(),
            media_compositions: Default::default(),
            video_jobs,
            video_frames,
            preview_inputs: crate::preview_inputs::PreviewInputReader::new(platform.clone()),
            audio_jobs,
            package_jobs: PackageJobs::default(),
            audio_mixer: None,
            audio_pcm: Vec::new(),
            font_store: None,
            engine,
            scene,
            geometry: motion_render::PlaneCompositor::new(),
            observer,
            observing: false,
            frame: 0.0,
            root,
            graphics: None,
            owner: thread::current().id(),
            presented: 0,
            last_cpu_us: 0,
            render_attempts: 0,
            video_pending_attempts: 0,
            video_prepare_us: 0,
            video_upload_us: 0,
            last_error: initial_error,
            last_error_observation: Default::default(),
            audio_snapshot_error: std::cell::RefCell::new(None),
            last_presented_frame: None,
            last_presented_revision: 0,
            view_revision: 0,
            last_presented_view_revision: 0,
            surface_epoch: 0,
            preview: PreviewPolicy::default(),
            preview_memory: Default::default(),
            preview_video: Default::default(),
            sampling,
            preview_memory_held: false,
            preview_memory_clock: Instant::now(),
            memory_context: None,
            recorder: None,
            resource_tracking: false,
            effects,
            plugin_root,
            editor: None,
            editor_token: String::new(),
            editor_activity: None,
            editor_recovery: None,
            editor_renderer: None,
            editor_target: None,
            platform,
        })
    }

    /// The host platform that owns this session's media and surface backends.
    pub fn platform(&self) -> &Arc<dyn Platform> {
        &self.platform
    }

    pub(crate) fn update_submission_tracking(&self) {
        let enabled = self.resource_tracking || self.recorder.is_some();
        if let Some(g) = &self.graphics { g.renderer.set_submission_tracking(enabled); }
        if let Some(renderer) = &self.editor_renderer { renderer.set_submission_tracking(enabled); }
    }
    pub fn configure_resource_tracking(&mut self, enabled: bool) -> Value {
        self.resource_tracking = enabled;
        self.update_submission_tracking();
        json!({"version":1,"enabled":enabled,"profiling":self.recorder.is_some()})
    }
    pub fn configure_memory_policy(
        &mut self,
        policy: motion_render::resource_policy::MemoryPolicy,
    ) -> Result<Value> {
        policy.validate()?;
        if self.effects.memory_policy() != policy {
            self.effects.set_memory_policy(policy)?;
            if let Some(graphics) = &mut self.graphics {
                graphics
                    .renderer
                    .set_memory_policy(policy)
                    .map_err(|error| error.to_string())?;
            }
            if let Some(renderer) = &mut self.editor_renderer {
                renderer
                    .set_memory_policy(policy)
                    .map_err(|error| error.to_string())?;
            }
            self.preview_memory.changed();
            self.memory_context = None;
            self.view_revision += 1; // Retained presentation must recheck the new limits.
        }
        Ok(json!({"policyVersion":2,"memoryPolicy":policy}))
    }

    pub fn replace_project(&mut self, engine: Engine, root: PathBuf) -> Result<()> {
        let mut registry=self.effects.registry.clone();
        crate::node_effects::restore(engine.project(),&mut registry)?;
        let platform = self.platform.clone();
        let audio_jobs =
            AudioJobs::with_decoder(root.clone(), Limits::default(), audio_decoder(&platform))?;
        let video_jobs = VideoJobs::with_audio_decoder(root.clone(), audio_decoder(&platform))?;
        if let Some(g) = &mut self.graphics {
            g.renderer
                .configure_assets(engine.project(), &root)
                .map_err(|e| e.to_string())?;
        }
        if let Some(mut editor) = self.editor.take() {
            editor
                .close(&mut self.engine, false)
                .map_err(|e| e.to_string())?;
        }
        self.editor_token.clear();
        self.editor_activity = None;
        self.editor_recovery = None;
        self.editor_renderer = None;
        self.editor_target = None;
        self.effects.set_registry(registry.clone());
        if let Some(graphics)=&mut self.graphics{graphics.renderer.set_effect_registry(registry);}
        self.effects.alpha_images.clear();
        self.scene = Scene::new(engine.project());
        self.observer = Observer::new(engine.project().width, engine.project().height);
        self.composition_contexts.clear();
        self.media_compositions.clear();
        self.engine = engine;
        self.audio_jobs = audio_jobs;
        self.video_jobs = video_jobs;
        // Frozen package exports stay queryable across project replacement, but
        // their open source handles belong to the old snapshot.
        self.video_frames.clear();
        self.sampling = Default::default();
        self.preview_inputs.cancel();
        self.preview_video.clear();
        self.preview_memory = Default::default();
        self.memory_context = None;
        if let Some(g) = &mut self.graphics { g.scratch.invalidate(); g.actual_preview_quality = None; }
        self.audio_mixer = None;
        self.root = root;
        self.font_store = None;
        self.frame = 0.0;
        self.observing = false;
        self.last_presented_frame = None;
        self.last_error = None;
        self.view_revision += 1;
        // Imported expression source stays editable when frame zero fails.
        if let Err(error) = self.sample() {
            self.last_error = Some(error);
        }
        Ok(())
    }

    pub fn check_thread(&self) -> Result<()> {
        if self.owner != thread::current().id() {
            Err("native session must run on its owning worker thread".into())
        } else {
            Ok(())
        }
    }

    pub fn detach(&mut self) {
        self.sampling.discard_request();
        self.preview_video.clear();
        self.preview_memory = Default::default();
        self.memory_context = None;
        self.preview_inputs.cancel();
        self.video_frames.clear();
        if let Some(g) = self.graphics.take() {
            if let Err(error) = g.renderer.device.poll(wgpu::PollType::wait_indefinitely()) {
                log::error!("[motion-gpu] session detach phase=wait: {error}");
            }
            drop(g);
        }
    }

    /// Bind a host-created surface. Everything after surface creation is shared
    /// with the Android preview path.
    pub fn attach(&mut self, target: SurfaceTarget) -> Result<()> {
        self.detach();
        let SurfaceTarget {
            instance,
            surface,
            config,
            mut renderer,
        } = target;
        renderer.set_effect_registry(self.effects.registry.clone());
        renderer.set_preview_resource_guard(true);
        renderer.set_submission_tracking(self.resource_tracking || self.recorder.is_some());
        renderer
            .set_memory_policy(self.effects.memory_policy())
            .map_err(|e| e.to_string())?;
        surface.configure(&renderer.device, &config);
        renderer
            .configure_assets(self.engine.project(), &self.root)
            .map_err(|e| e.to_string())?;
        let (render_width, render_height) = self.preview.render_dimensions(
            self.engine.project().width,
            self.engine.project().height,
            config.width,
            config.height,
        );
        // Start with a small placeholder; the requested target is prepared under
        // the bounded policy, rather than failing before the first preview.
        let scratch = PreviewFrameTargets::new(&renderer, render_width.min(64), render_height.min(64))
            .map_err(|e| e.to_string())?;
        let presenter = Presenter::new(&renderer, &scratch.view, config.format);
        // Auto quality needs GPU cost even without diagnostic recording; only
        // four reusable 32-byte timing buffers are read, never video pixels.
        let timer = GpuTimer::new(&renderer.device, &renderer.queue);
        self.graphics = Some(Graphics {
            surface,
            renderer,
            scratch,
            actual_preview_quality: None,
            last_canvas: [self.scene.width, self.scene.height],
            last_background: self.scene.background,
            presenter,
            config,
            _instance: instance,
            timer,
        });
        self.last_error = None;
        self.surface_epoch += 1;
        self.last_presented_frame = None;
        Ok(())
    }

    pub fn render(&mut self, frame: f64) -> Result<bool> {
        self.render_with_overlay(frame, None, |_, _, _| {})
    }

    /// Repaint host chrome over the last completed composition without sampling
    /// or executing its effects. The caller must invalidate its reuse key when
    /// project, camera, quality, surface or viewport dimensions change.
    pub fn present_retained_with_overlay(
        &mut self,
        viewport: [f32; 4],
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<bool> {
        self.present_retained_with_view(viewport, None, overlay)
    }
    pub fn present_retained_with_view(&mut self, viewport: [f32; 4], image: Option<[f32; 4]>,
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView)) -> Result<bool> {
        validate_canvas_image(image)?;
        let Some(g) = &mut self.graphics else { return Ok(false); };
        let [x, y, w, h] = viewport;
        if !viewport.iter().all(|v| v.is_finite()) || x < 0. || y < 0.
            || w <= 0. || h <= 0. || x + w > g.config.width as f32 + 0.01
            || y + h > g.config.height as f32 + 0.01
        { return Err(format!("invalid composition viewport viewport={viewport:?} surface={}x{} phase=retained", g.config.width, g.config.height)); }
        let rendered = present_held(g, viewport, image, overlay)?;
        self.presented += u64::from(rendered);
        Ok(rendered)
    }

    /// Present once, with host chrome on the same GPU device and session thread.
    pub fn render_with_overlay(
        &mut self,
        frame: f64,
        viewport: Option<[f32; 4]>,
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<bool> {
        self.render_with_view(frame, viewport, None, overlay)
    }
    pub fn render_with_view(&mut self, frame: f64, viewport: Option<[f32; 4]>, image: Option<[f32; 4]>,
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView)) -> Result<bool> {
        validate_canvas_image(image)?;
        use motion_render::preview_memory::PREVIEW_MEMORY_LEVELS;
        let began = Instant::now();
        self.render_attempts += 1;
        self.frame = frame;
        self.sample()?;
        let tier = self.preview.tier();
        let Some(g) = &mut self.graphics else {
            return Ok(false);
        };
        let viewport = viewport.unwrap_or([0., 0., g.config.width as f32, g.config.height as f32]);
        let [vx, vy, vw, vh] = viewport;
        if !viewport.iter().all(|v| v.is_finite())
            || vx < 0.
            || vy < 0.
            || vw <= 0.
            || vh <= 0.
            || vx + vw > g.config.width as f32 + 0.01
            || vy + vh > g.config.height as f32 + 0.01
        {
            return Err(format!("invalid composition viewport viewport={viewport:?} surface={}x{} phase=render", g.config.width, g.config.height));
        }
        let context = (
            self.engine.revision(),
            preview_resource_signature(&self.scene),
            vw.ceil() as u32,
            vh.ceil() as u32,
        );
        if self.memory_context != Some(context) {
            self.preview_memory.changed();
            self.memory_context = Some(context);
        }
        let exposure = self.sampling.required();
        let now_ms = self.preview_memory_clock.elapsed().as_millis() as u64;
        self.preview_memory_held = false;
        if !self.preview_memory.begin(now_ms) {
            self.preview_memory_held = g.scratch.valid();
            let held = present_held(g, viewport, image, overlay)?;
            self.presented += u64::from(held);
            return Ok(held);
        }
        if exposure { return self.render_exposure_with_view(frame,viewport,image,overlay); }
        g.renderer.retain_video_instances(&self.scene);
        self.preview_video.retain(&self.scene);
        let upcoming = motion_render::image_resources::upcoming_assets(
            self.engine.project(),
            frame,
        ).map_err(|error| error.to_string())?;
        g.renderer.set_image_prefetch(upcoming);
        let preparing = Instant::now();
        let frames = self.video_frames.prepare_scene(
            self.engine.project(),
            &self.root,
            &self.scene,
            frame,
        )?;
        self.video_prepare_us = preparing.elapsed().as_micros() as u64;
        let Some(frames) = frames else {
            self.video_pending_attempts += 1;
            // Decoder work is asynchronous. Keep editor chrome responsive over
            // the last completed canvas (or an empty surface on first open),
            // but leave this composition pending so the caller keeps polling.
            let retained = g.scratch.valid().then_some(viewport);
            let painted = present_chrome(g, retained, image, overlay)?;
            self.presented += u64::from(painted);
            return Ok(false);
        };
        g.renderer.device.poll(wgpu::PollType::Poll).map_err(|e| format!("preview GPU poll: {e}"))?;
        g.renderer.check_health().map_err(|e| e.to_string())?;
        let mut gpu_work_us = None;
        if let Some(timer) = &mut g.timer {
            for timing in timer.collect().into_iter().flatten() {
                gpu_work_us = Some(timing.total_us);
                if let Some(recorder) = &mut self.recorder {
                    recorder.timing(timing);
                }
            }
        }
        let base = self.preview.render_dimensions(
            self.scene.width,
            self.scene.height,
            vw.ceil() as u32,
            vh.ceil() as u32,
        );
        let mut overlay = Some(overlay);
        for _ in 0..PREVIEW_MEMORY_LEVELS {
            let quality = self
                .preview_memory
                .quality(self.preview.mode == PreviewMode::High);
            let image_resolution = quality.image_resolution();
            let ready =
                g.renderer
                    .prepare_scene_assets(&self.scene, image_resolution, quality.level == 0);
            match ready {
                Ok(false) => return Ok(false),
                Ok(true) => {}
                Err(error) => {
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
            }
            let (rw, rh) = quality.dimensions(base.0, base.1);
            if let Err(error) = g.scratch.prepare(&g.renderer, rw, rh) {
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
            let sequence = self.presented + 1;
            let mut encoder =
                g.renderer
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Motion Studio bounded composition and presentation"),
                    });
            let uploading = Instant::now();
            let mut video_uploads = Vec::new();
            let mut upload_error = None;
            for (object, image) in &frames {
                let source = self
                    .scene
                    .video_layers()
                    .into_iter()
                    .find(|l| l.id == *object)
                    .unwrap()
                    .video
                    .as_ref()
                    .unwrap()
                    .asset;
                match self.preview_video.upload_prepared(
                    &mut g.renderer,
                    *object,
                    source,
                    image,
                    quality.video_edge,
                    !self.scene.nested.is_empty(),
                    &mut encoder,
                ) {
                    Ok(Some(upload)) => video_uploads.push(upload),
                    Ok(None) => {}
                    Err(error) => {
                        upload_error = Some(error);
                        break;
                    }
                }
            }
            self.video_upload_us = uploading.elapsed().as_micros() as u64;
            if let Some(error) = upload_error {
                drop(encoder);
                drop(video_uploads);
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
            let slot = g.timer.as_mut().and_then(|t| t.begin(sequence));
            let timestamps = slot.map(|i| g.timer.as_ref().unwrap().writes(i, 0));
            let candidate_view = g
                .scratch
                .prepared()
                .expect("prepared preview target")
                .view
                .clone();
            let encoded = if self.preview.mode == PreviewMode::High {
                g.renderer.encode(
                    &self.scene,
                    &candidate_view,
                    rw,
                    rh,
                    &mut encoder,
                    timestamps,
                )
            } else {
                g.renderer.encode_preview(
                    &self.scene,
                    &candidate_view,
                    rw,
                    rh,
                    &mut encoder,
                    timestamps,
                )
            };
            if let Err(error) = encoded {
                if let (Some(timer), Some(i)) = (&mut g.timer, slot) {
                    timer.cancel_unsubmitted(i);
                }
                drop(encoder);
                drop(video_uploads);
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
            // Do not acquire/drop swapchain images for refused candidates.
            let acquiring = Instant::now();
            let output = match acquire_preview_surface(g) {
                Ok(Some(output)) => output,
                result => {
                    if let (Some(timer), Some(i)) = (&mut g.timer, slot) {
                        timer.cancel_unsubmitted(i);
                    }
                    drop(encoder);
                    drop(video_uploads);
                    return result.map(|_| false);
                }
            };
            let acquire_us = acquiring.elapsed().as_micros() as u64;
            g.presenter.set_source(&g.renderer.device, &candidate_view);
            let view = output.texture.create_view(&Default::default());
            g.presenter.encode_view(
                &g.renderer.queue, &mut encoder, &view,
                slot.map(|i| g.timer.as_ref().unwrap().writes(i, 1)),
                if image.is_some() { viewport } else { preview_viewport(viewport, [self.scene.width, self.scene.height]) },
                image, self.scene.background,
            );
            overlay.take().unwrap()(&g.renderer, &mut encoder, &view);
            if let Some(i) = slot {
                g.timer.as_ref().unwrap().resolve(i, &mut encoder);
            }
            let submitting = Instant::now();
            g.renderer.submit_tracked(encoder.finish(), |ledger| g.scratch.collect_texture_resources(ledger));
            for upload in video_uploads {
                upload.mark_submitted();
            }
            let submit_us = submitting.elapsed().as_micros() as u64;
            if let Some(i) = slot {
                g.timer.as_ref().unwrap().map(i);
            }
            if let Err(error) = g.renderer.check_health() {
                g.presenter.set_source(&g.renderer.device, &g.scratch.view);
                return Err(error.to_string());
            }
            g.scratch.commit_submitted();
            g.actual_preview_quality = Some(quality);
            g.last_canvas = [self.scene.width, self.scene.height];
            g.last_background = self.scene.background;
            let presenting = Instant::now();
            g.renderer.queue.present(output);
            let present_call_us = presenting.elapsed().as_micros() as u64;
            self.presented += 1;
            let render_wall_us = began.elapsed().as_micros() as u64;
            self.last_cpu_us = render_wall_us
                .saturating_sub(acquire_us)
                .saturating_sub(present_call_us);
            if let Some(recorder) = &mut self.recorder {
                recorder.record(FrameMeasurement {
                    sequence,
                    frame,
                    elapsed_us: recorder.elapsed_us(),
                    cpu_prepare_us: self.last_cpu_us,
                    acquire_us,
                    submit_us,
                    present_call_us,
                    render_wall_us,
                    render_width: rw,
                    render_height: rh,
                    preview_fps: tier.fps(),
                    gpu: None,
                    resources: Some(motion_render::FrameResources {
                        textures: g.renderer.texture_memory_breakdown(),
                        preview_target_bytes: g.scratch.texture_bytes(),
                        exposure_target_bytes: 0,
                        exposure_media_us: 0,
                        cpu_video_cache_bytes: self.video_frames.cache_bytes()? as u64,
                        cpu_video_proxy_bytes: self.preview_video.bytes() as u64,
                        video_prepare_us: self.video_prepare_us,
                        video_upload_us: self.video_upload_us,
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
                gpu_work_us,
                acquire_us.saturating_add(present_call_us),
            );
            if previous_tier != self.preview.tier() {
                self.view_revision += 1;
            }
            self.preview_memory.succeeded();
            if quality.level == 0 {
                g.renderer
                    .prefetch_scene_assets(image_resolution)
                    .map_err(|e| e.to_string())?;
            }
            self.last_error = None;
            return Ok(true);
        }
        self.preview_memory_held = g.scratch.valid();
        let held = present_held(g, viewport, image, overlay.take().unwrap())?;
        self.presented += u64::from(held);
        Ok(held)
    }

    pub fn sample(&mut self) -> Result<()> {
        let result = self.sampling.synchronize(self.engine.project(),self.engine.revision(),&mut self.video_frames)
            .and_then(|_| self
            .scene
            .sample(
                self.engine.project(),
                self.frame,
                if self.observing {
                    Some(&self.observer)
                } else {
                    None
                }, &motion_core::ExpressionEvaluator)
            .map_err(|e| e.to_string()));
        if let Err(error) = &result {
            self.last_error = Some(error.clone());
        } else if self
            .last_error
            .as_ref()
            .is_some_and(|e| e.starts_with("expression "))
        {
            self.last_error = None;
        }
        result
    }

    pub fn composition_error(&self, code: &str, message: &str, details: Value) -> String {
        motion_core::composition::CompositionError {
            code: code.into(),
            composition: self.engine.project().composition_id.clone(),
            message: message.into(),
            details,
        }
        .to_string()
    }
}

fn preview_resource_signature(scene: &Scene) -> u64 {
    use std::hash::{Hash, Hasher};
    fn visit(scene: &Scene, hash: &mut impl Hasher) {
        scene.composition_id.hash(hash);
        for layer in &scene.layers {
            layer.id.hash(hash); layer.asset.hash(hash);
            layer.video.as_ref().map(|v| v.asset).hash(hash);
            layer.masks.len().hash(hash); layer.vector.is_some().hash(hash);
        }
        for effect in &scene.effects { effect.layer.hash(hash); effect.instance.hash(hash); }
        for child in &scene.nested { visit(&child.scene, hash); }
    }
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    visit(scene, &mut hash);
    hash.finish()
}
fn preview_refused(g: &mut Graphics, policy: &mut motion_render::preview_memory::PreviewMemoryPolicy,
    error: motion_render::RenderError, now_ms: u64) -> Result<bool> {
    let Some(rejection) = error.resource_rejection().copied() else { return Err(error.to_string()); };
    let retry = policy.refused(rejection, now_ms);
    g.scratch.discard_candidate();
    g.renderer.reclaim_preview_working_set().map_err(|e| format!("{error}; preview working-set cleanup also failed: {e}"))?;
    Ok(retry)
}
static PREVIEW_SURFACE_FAILURES: motion_render::observability::FailureCounter =
    motion_render::observability::FailureCounter::new();
fn acquire_preview_surface(g: &mut Graphics) -> Result<Option<wgpu::SurfaceTexture>> {
    match g.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(output) | wgpu::CurrentSurfaceTexture::Suboptimal(output) => { PREVIEW_SURFACE_FAILURES.success(); Ok(Some(output)) },
        error @ (wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated) => {
            PREVIEW_SURFACE_FAILURES.failure("host.acquire_preview_surface", format_args!("state={error:?}"));
            g.surface.configure(&g.renderer.device, &g.config); Ok(None)
        }
        wgpu::CurrentSurfaceTexture::Timeout => {
            PREVIEW_SURFACE_FAILURES.failure("host.acquire_preview_surface", format_args!("state=Timeout"));
            Ok(None)
        },
        wgpu::CurrentSurfaceTexture::Occluded => { PREVIEW_SURFACE_FAILURES.success(); Ok(None) },
        wgpu::CurrentSurfaceTexture::Validation => Err("surface acquisition validation failed".into()),
    }
}
fn validate_canvas_image(image: Option<[f32; 4]>) -> Result<()> {
    if image.is_some_and(|v| !v.iter().all(|v| v.is_finite()) || v[2] <= 0. || v[3] <= 0.) {
        return Err("invalid composition canvas image".into());
    }
    Ok(())
}
fn preview_viewport([x,y,w,h]: [f32; 4], [cw,ch]: [u32; 2]) -> [f32; 4] {
    motion_render::fit_viewport([x, y, w, h], [cw, ch])
}
fn present_held(g: &mut Graphics, viewport: [f32; 4], image: Option<[f32; 4]>,
    overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView)) -> Result<bool> {
    if !g.scratch.valid() { return Ok(false); }
    present_chrome(g, Some(viewport), image, overlay)
}
fn present_chrome(g: &mut Graphics, retained_viewport: Option<[f32; 4]>, image: Option<[f32; 4]>,
    overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView)) -> Result<bool> {
    let Some(output) = acquire_preview_surface(g)? else { return Ok(false); };
    let view = output.texture.create_view(&Default::default());
    let mut encoder = g.renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Motion Studio retained preview") });
    if let Some(viewport) = retained_viewport {
        g.presenter.encode_view(&g.renderer.queue, &mut encoder, &view, None,
            if image.is_some() { viewport } else { preview_viewport(viewport, g.last_canvas) }, image, g.last_background);
    } else {
        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Motion Studio pending preview"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view, depth_slice: None, resolve_target: None,
                ops: wgpu::Operations {load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store},
            })],
            ..Default::default()
        });
    }
    overlay(&g.renderer, &mut encoder, &view);
    g.renderer.submit_tracked(encoder.finish(), |ledger| g.scratch.collect_texture_resources(ledger));
    g.renderer.check_health().map_err(|e| e.to_string())?;
    g.renderer.queue.present(output);
    Ok(true)
}

/// Adapt the host's audio fallback to the callback shape `motion-media` expects.
fn audio_decoder(platform: &Arc<dyn Platform>) -> motion_media::DecodeAudio {
    let platform = platform.clone();
    Arc::new(
        move |path: &std::path::Path,
              pcm: &std::path::Path,
              selected: Option<u32>,
              limit: u64,
              check: &mut dyn FnMut(f64) -> motion_media::Result<()>| {
            platform.decode_audio(path, pcm, selected, limit, check)
        },
    )
}

/// Run `operation` against a live session on its owning thread.
pub fn with_session<T>(id: i64, operation: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
    let began = Instant::now();
    let _metadata_scope = metadata_observation::Scope::enter();
    let mut registry = sessions().lock().unwrap_or_else(|e| e.into_inner());
    let session = registry.get_mut(&id).ok_or("native session is closed")?;
    session.check_thread()?;
    let result = session.recover_plugin_editor(Instant::now()).and_then(|()| operation(session));
    let event = session.last_error_observation.observe(
        session.last_error.as_deref(), session.frame,
        &session.engine.project().composition_id, began,
    );
    // Logging can call host-owned sinks. Release the registry before any IO.
    drop(registry);
    if let Some(event) = event { event.emit(id); }
    result
}

/// Aggregate resource report for every session rooted under `directory`.
pub fn resource_info(directory: &std::path::Path) -> Result<Value> {
    let began = Instant::now();
    let root = directory.canonicalize().map_err(|e| e.to_string())?;
    let registry = sessions().lock().unwrap_or_else(|e| e.into_inner());
    let mut count = 0u64;
    let mut graphics = 0u64;
    let mut assets = 0u64;
    let mut targets = 0u64;
    let mut unresolved = 0usize;
    let mut first = None;
    for (&id, session) in registry.iter() {
        match session.root.canonicalize() {
            Ok(path) if path.starts_with(&root) => {},
            Ok(_) => continue,
            Err(error) => {
                unresolved = unresolved.saturating_add(1);
                if first.is_none() {
                    first = Some(resource_observation::UnresolvedRoot::new(id, &session.root, &error));
                }
                continue;
            }
        }
        count += 1;
        if let Some(g) = &session.graphics {
            graphics += 1;
            assets += g.renderer.texture_bytes();
            targets += g.scratch.texture_bytes();
        }
    }
    drop(registry);
    resource_observation::report(unresolved, first, began);
    Ok(
        json!({"sessions":count,"graphics":graphics,"assetTextureBytes":assets,"renderTargetBytes":targets,
        "scope":"Application-owned sessions and textures in the requested project directory; not driver/system allocations"}),
    )
}

/// Create a session and register it. Returns `0` on failure and stores the
/// reason for [`creation_error`], matching the Android bridge contract.
pub fn open(root: PathBuf, project: Project, platform: Arc<dyn Platform>) -> Result<i64> {
    let session = Session::new(project, root, platform)?;
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    sessions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, session);
    Ok(id)
}

thread_local! {
    static CREATION_ERROR: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Reason the most recent [`open`] on this thread failed.
pub fn creation_error() -> String {
    CREATION_ERROR.with(|e| e.borrow().clone())
}

pub fn set_creation_error(error: String) {
    CREATION_ERROR.with(|e| *e.borrow_mut() = error);
}

/// Close a session and release its GPU resources.
pub fn close(id: i64) {
    let mut registry = sessions().lock().unwrap_or_else(|e| e.into_inner());
    if registry
        .get(&id)
        .is_some_and(|s| s.owner == thread::current().id())
    {
        if let Some(mut s) = registry.remove(&id) {
            s.detach();
        }
    }
}
