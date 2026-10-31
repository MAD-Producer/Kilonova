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
    FrameMeasurement, FrameRecorder, GpuTimer, Presenter, PreviewMode, PreviewPolicy,
    SdrPreviewFrameTargets as PreviewFrameTargets,
    Renderer,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicI64, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, ThreadId},
    time::Instant,
};

pub type Result<T> = std::result::Result<T, crate::Error>;

// A root path and revision can recur after reopening a document. Clipboard and
// other asynchronous edits need an identity that cannot alias that replacement.
fn next_document_generation() -> Result<u64> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| "document generation space exhausted".into())
}

mod registry_observation;
mod last_error_observation;
mod resource_observation;
mod sampling;
pub mod viewer;
#[cfg(feature = "ai-depth")]
mod depth;
#[cfg(test)]
mod output_precision_tests;
mod preview_pressure;
#[cfg(all(test, target_os = "windows"))]
mod surface_admission_tests;
#[cfg(all(test, target_os = "windows"))]
mod precision_preview_tests;
pub(crate) mod metadata_observation;
#[cfg(test)]
mod metadata_observation_tests;
#[cfg(test)]
mod resource_observation_tests;
#[cfg(test)]
mod last_error_observation_tests;
#[cfg(test)]
mod registry_scope_tests;

static NEXT: AtomicI64 = AtomicI64::new(1);
type SessionRegistry = HashMap<i64, Arc<SessionSlot>>;
static SESSIONS: OnceLock<Mutex<SessionRegistry>> = OnceLock::new();

struct SessionSlot {
    // Reject foreign callers before waiting behind an owning thread's handler.
    owner: ThreadId,
    // Owner-thread close takes the value even if a resource report retained the
    // slot. The last Arc may then be dropped anywhere without moving GPU Drop.
    session: Mutex<Option<Session>>,
}

/// Monotonic counter shared by session handles and plugin editor tokens.
pub fn next_token() -> i64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn sessions() -> &'static Mutex<SessionRegistry> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Live GPU resources bound to one window.
pub struct Graphics {
    pub surface: wgpu::Surface<'static>,
    pub renderer: Renderer,
    pub scratch: PreviewFrameTargets,
    pub(crate) views: viewer::State,
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
    #[cfg(feature = "ai-depth")]
    depth: Option<depth::State>,
    pub audio_jobs: AudioJobs,
    pub(crate) asset_relink: Option<crate::ops::media::relink::Batch>,
    pub package_jobs: PackageJobs,
    pub audio_mixer: Option<(u64, AudioMixer)>,
    pub audio_pcm: Vec<f32>,
    pub font_store: Option<motion_media::fonts::FontStore>,
    pub engine: Engine,
    pub scene: Scene,
    pub geometry: motion_render::PlaneCompositor,
    pub observer: Observer,
    pub(crate) observer_history: crate::ops::preview::CameraHistory,
    pub observing: bool,
    pub frame: f64,
    pub root: PathBuf,
    pub document_generation: u64,
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
    pub editor_target: Option<motion_render::SdrCaptureTarget>,
    allocation_budget: motion_render::AllocationBudget,
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
        let engine = Engine::new(project).map_err(crate::Error::from)?;
        let project = engine.project();
        let mut scene = Scene::new(project)?;
        scene.set_model_source_catalog(motion_render::ModelSourceCatalog::prepare(project, &root, &Default::default())?);
        // A newly opened project has no compiled expressions on this worker.
        // Prepare source bytecode before the first frame's unchanged deadline,
        // just as SetExpression already does before evaluating an edit.
        let initial_sample = motion_core::prepare_expression_sources(project)
            .and_then(|()| scene.sample(project, 0.0, None, &motion_core::ExpressionEvaluator));
        let initial_error = match initial_sample {
            Ok(()) => None,
            Err(error @ motion_core::Error::Expression { .. }) => {
                let mut base = project.clone();
                for expression in &mut base.expressions {
                    expression.enabled = false;
                }
                scene.sample(&base, 0.0, None, &motion_core::ExpressionEvaluator).map_err(crate::Error::from)?;
                Some(error.to_string())
            }
            Err(error) => return Err(error.into()),
        };
        let observer = Observer::new(project.width, project.height);
        let plugin_root = root
            .parent()
            .ok_or("project directory has no parent")?
            .join("plugins");
        let mut registry=registry_observation::load(&plugin_root).map_err(crate::Error::from)?;
        crate::node_effects::restore(project,&root,&mut registry)?;
        let mut effects = motion_render::effect_plan::PlanBuilder::new(registry)?;
        effects.set_precision(project.working_precision);
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
            #[cfg(feature = "ai-depth")]
            depth: None,
            audio_jobs,
            asset_relink: None,
            package_jobs: PackageJobs::default(),
            audio_mixer: None,
            audio_pcm: Vec::new(),
            font_store: None,
            engine,
            scene,
            geometry: motion_render::PlaneCompositor::new(),
            observer,
            observer_history: Default::default(),
            observing: false,
            frame: 0.0,
            root,
            document_generation: next_document_generation()?,
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
            allocation_budget: motion_render::AllocationBudget::default(),
            platform,
        })
    }

    /// The host platform that owns this session's media and surface backends.
    pub fn platform(&self) -> &Arc<dyn Platform> {
        &self.platform
    }

    /// Shared logical admission for all renderers created for this session.
    /// Clone before native construction, including host-created surface renderers.
    /// Retained resources and frozen exports keep the same admission identity.
    pub fn allocation_budget(&self) -> &motion_render::AllocationBudget {
        &self.allocation_budget
    }

    /// Bounded metadata; peak means enrolled capacity since Session creation,
    /// including concurrent renderers and pending leases, not physical residency.
    pub fn allocation_budget_info(&self) -> Result<motion_render::AllocationBudgetInfo> {
        self.allocation_budget.info().map_err(crate::Error::from)
    }

    /// Read the active surface renderer, or the existing headless editor renderer.
    /// Does not create a GPU, modify admission, or change preview/output quality.
    pub fn memory_pressure_info(&mut self) -> motion_render::MemoryPressureSnapshot {
        if let Some(graphics) = &mut self.graphics {
            graphics.renderer.memory_pressure()
        } else if let Some(renderer) = &mut self.editor_renderer {
            renderer.memory_pressure()
        } else {
            motion_render::MemoryPressureSnapshot::without_renderer()
        }
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
            // This can fail on a broken admission invariant. Do it before any
            // category/policy mutation. Existing owners remain booked on a decrease.
            self.allocation_budget.set_limit(policy.session_allocation_budget_bytes)?;
            self.effects.set_memory_policy(policy)?;
            if let Some(graphics) = &mut self.graphics {
                graphics
                    .renderer
                    .set_memory_policy(policy)
                    .map_err(crate::Error::from)?;
            }
            if let Some(renderer) = &mut self.editor_renderer {
                renderer
                    .set_memory_policy(policy)
                    .map_err(crate::Error::from)?;
            }
            self.preview_memory.changed();
            self.memory_context = None;
            self.view_revision += 1; // Retained presentation must recheck the new limits.
        }
        Ok(json!({"policyVersion":motion_render::resource_policy::MEMORY_POLICY_VERSION,"memoryPolicy":policy}))
    }

    pub fn replace_project(&mut self, engine: Engine, root: PathBuf) -> Result<()> {
        if let Some(batch) = &mut self.asset_relink {
            // Keep the bounded terminal task queryable until its caller releases
            // it, so a document replacement is observed as cancellation.
            batch.cancel()?;
        }
        let document_generation = next_document_generation()?;
        let mut next_scene = Scene::new(engine.project())?;
        next_scene.set_model_source_catalog(motion_render::ModelSourceCatalog::prepare(
            engine.project(), &root, &self.scene.model_source_catalog())?);
        let mut registry=self.effects.registry.clone();
        crate::node_effects::restore(engine.project(),&root,&mut registry)?;
        let platform = self.platform.clone();
        let audio_jobs =
            AudioJobs::with_decoder(root.clone(), Limits::default(), audio_decoder(&platform))?;
        let video_jobs = VideoJobs::with_audio_decoder(root.clone(), audio_decoder(&platform))?;
        if let Some(g) = &mut self.graphics {
            g.renderer
                .configure_assets_with_model_catalog(engine.project(), &root, &next_scene.model_source_catalog())
                .map_err(crate::Error::from)?;
        }
        if let Some(mut editor) = self.editor.take() {
            editor
                .close(&mut self.engine, false)
                .map_err(crate::Error::from)?;
        }
        self.editor_token.clear();
        self.editor_activity = None;
        self.editor_recovery = None;
        self.editor_renderer = None;
        self.editor_target = None;
        self.effects.set_registry(registry.clone());
        if let Some(graphics)=&mut self.graphics{graphics.renderer.set_effect_registry(registry);}
        self.effects.alpha_images.clear();
        self.scene = next_scene;
        self.observer = Observer::new(engine.project().width, engine.project().height);
        self.composition_contexts.clear();
        self.media_compositions.clear();
        self.engine = engine;
        self.document_generation = document_generation;
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
        if let Some(g) = &mut self.graphics {
            g.scratch.invalidate(); g.actual_preview_quality = None;
            g.views = Default::default();
        }
        self.audio_mixer = None;
        self.root = root;
        self.font_store = None;
        self.frame = 0.0;
        self.observing = false;
        self.observer_history = Default::default();
        self.last_presented_frame = None;
        self.last_error = None;
        self.view_revision += 1;
        // Imported expression source stays editable when frame zero fails.
        if let Err(error) = self.sample() {
            self.last_error = Some(error.to_string());
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
        if !self.allocation_budget.same_budget(target.renderer.allocation_budget()) {
            return Err("surface renderer was not constructed with the session allocation budget".into());
        }
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
            .map_err(crate::Error::from)?;
        surface.configure(&renderer.device, &config);
        renderer
            .configure_assets_with_model_catalog(self.engine.project(), &self.root, &self.scene.model_source_catalog())
            .map_err(crate::Error::from)?;
        if let Some(change) = renderer.prepare_working_precision(self.engine.project().working_precision)? {
            renderer.commit_working_precision(change)?;
        }
        // The invalid placeholder is never displayed. One pixel leaves room
        // for a candidate even at the smallest supported two-target allowance;
        // prepare the real dimensions under the bounded preview policy.
        let scratch = PreviewFrameTargets::new(&renderer, 1, 1)
            .map_err(crate::Error::from)?;
        let presenter = Presenter::new_admitted(&renderer, &scratch.view, config.format)?;
        // Auto quality needs GPU cost even without diagnostic recording; only
        // four reusable 32-byte timing buffers are read, never video pixels.
        let timer = GpuTimer::new(&renderer)?;
        // Keep the last valid front until every replacement allocation succeeds.
        self.detach();
        self.graphics = Some(Graphics {
            surface,
            renderer,
            scratch,
            views: Default::default(),
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
        let Some(g) = &self.graphics else { return Ok(false); };
        let [x, y, w, h] = viewport;
        if !viewport.iter().all(|v| v.is_finite()) || x < 0. || y < 0.
            || w <= 0. || h <= 0. || x + w > g.config.width as f32 + 0.01
            || y + h > g.config.height as f32 + 0.01
        { return Err(format!("invalid composition viewport viewport={viewport:?} surface={}x{} phase=retained", g.config.width, g.config.height).into()); }
        if g.renderer.working_precision() != self.engine.project().working_precision {
            return self.render_with_view(self.frame, Some(viewport), image, overlay);
        }
        if self.observe_preview_pressure()? {
            return self.render_with_view(self.frame, Some(viewport), image, overlay);
        }
        let Some(g) = &mut self.graphics else { return Ok(false); };
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
        if !self.prepare_render_depth(None)? { return Ok(false); }
        self.synchronize_working_precision()?;
        let tier = self.preview.tier();
        let Some(g) = &self.graphics else {
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
            return Err(format!("invalid composition viewport viewport={viewport:?} surface={}x{} phase=render", g.config.width, g.config.height).into());
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
        self.observe_preview_pressure()?;
        let Some(g) = &mut self.graphics else { return Ok(false); };
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
        let mut upcoming = motion_render::image_resources::upcoming_assets(
            self.engine.project(),
            frame,
        ).map_err(crate::Error::from)?;
        if !self.preview_memory.allows_image_prefetch() { upcoming.clear(); }
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
        g.renderer.device.poll(wgpu::PollType::Poll).map_err(|e| crate::Error::with_context("preview GPU poll", e))?;
        g.renderer.check_health().map_err(crate::Error::from)?;
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
                    .allocation_command_encoder_for(motion_render::gpu_commands::CommandStage::CompositionAndPresentation, &wgpu::CommandEncoderDescriptor {
                        label: Some("Motion Studio bounded composition and presentation"),
                    })?;
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
            let encoded = encoded.and_then(|_| g.scratch.encode_output(&g.renderer, &mut encoder));
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
            let display = if g.presenter.view_channel() == motion_render::ViewChannel::Color {
                g.scratch.prepared_output()
            } else {
                g.scratch.prepared()
            }
                .ok_or("converted preview target missing")?;
            g.presenter.set_admitted_source(&g.renderer.device, &display.view);
            let view = output.texture.create_view(&Default::default());
            if let Err(error) = g.presenter.encode_view(
                &g.renderer.queue, &mut encoder, &view,
                slot.map(|i| g.timer.as_ref().unwrap().writes(i, 1)),
                if image.is_some() { viewport } else { preview_viewport(viewport, [self.scene.width, self.scene.height]) },
                image, self.scene.background,
            ) {
                if let (Some(timer), Some(i)) = (&mut g.timer, slot) {
                    timer.cancel_unsubmitted(i);
                }
                return Err(error.into());
            }
            overlay.take().unwrap()(&g.renderer, &mut encoder, &view);
            if let Some(i) = slot {
                g.timer.as_ref().unwrap().resolve(i, &mut encoder);
            }
            let submitting = Instant::now();
            let presentation_command = encoder.command_identity();
            g.renderer.submit_allocated(encoder.finish(), |ledger| {
                g.scratch.collect_texture_resources(ledger);
                g.presenter.collect_inspection_resources(ledger);
            })?;
            for upload in video_uploads {
                upload.mark_submitted();
            }
            let submit_us = submitting.elapsed().as_micros() as u64;
            if let Some(i) = slot {
                g.timer.as_ref().unwrap().map(i);
            }
            if let Err(error) = g.renderer.check_health() {
                g.presenter.set_admitted_source(&g.renderer.device, &g.scratch.view);
                return Err(error.into());
            }
            g.scratch.commit_submitted()?;
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
                    presentation_command,
                    exposure: None,
                    resources: Some(motion_render::FrameResources {
                        textures: g.renderer.texture_memory_breakdown(),
                        vector_upload: g.renderer.vector_upload_info()?,
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
                        image_readback: Some(g.renderer.image_readback_info()),
                        timestamp_readback: g.timer.as_ref().map(GpuTimer::readback_info),
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
                    .map_err(crate::Error::from)?;
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
        let model_sources = self.synchronize_model_sources();
        #[cfg(feature = "ai-depth")]
        self.synchronize_depth()?;
        self.effects.set_precision(self.engine.project().working_precision);
        let result = model_sources.and_then(|_| self.sampling.synchronize(self.engine.project(),self.engine.revision(),&mut self.video_frames))
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
            .map_err(crate::Error::from));
        if let Err(error) = &result {
            self.last_error = Some(error.to_string());
        } else if self
            .last_error
            .as_ref()
            .is_some_and(|e| e.starts_with("expression "))
        {
            self.last_error = None;
        }
        result
    }
    /// Rendering admission is separate from semantic sampling: pending depth is
    /// normal. An explicit indexed video track may stage its saved source sample;
    /// admission never invents a distance or substitutes flat data.
    pub(crate) fn prepare_render_depth(&mut self, scene: Option<&mut Scene>) -> Result<bool> {
        #[cfg(feature = "ai-depth")]
        {
            if let Some(scene) = scene { return self.render_depth_sample(scene); }
            if !self.scene.has_calibrated_depth() { return Ok(true); }
            let mut prepared = self.scene.clone();
            let ready = self.render_depth_sample(&mut prepared)?;
            if ready { self.scene = prepared; }
            Ok(ready)
        }
        #[cfg(not(feature = "ai-depth"))]
        {
            if scene.unwrap_or(&mut self.scene).has_calibrated_depth() {
                return Err("calibrated depth rendering requires the ai-depth feature".into());
            }
            Ok(true)
        }
    }
    /// Revision checks are metadata-only; unchanged frame sampling never opens
    /// source files or rebuilds the catalog. Root replacement prepares explicitly.
    pub(crate) fn synchronize_model_sources(&mut self) -> Result<()> {
        let previous = self.scene.model_source_catalog();
        if previous.matches_assets(self.engine.project()) { return Ok(()); }
        let ready = self.graphics.as_ref().map(|g| g.renderer.model_source_catalog())
            .filter(|catalog| catalog.matches_assets(self.engine.project()));
        let catalog = match ready {
            Some(catalog) => catalog,
            None => motion_render::ModelSourceCatalog::prepare(self.engine.project(), &self.root, &previous)?,
        };
        if let Some(g) = &mut self.graphics {
            if !g.renderer.model_source_catalog().shares_sources_with(&catalog) {
                g.renderer.configure_assets_with_model_catalog(self.engine.project(), &self.root, &catalog)?;
            }
        }
        self.scene.set_model_source_catalog(catalog);
        Ok(())
    }

    /// Persisted precision changes are applied before any content work on the
    /// existing device. A refusal returns its cause; the previous SDR front is
    /// retained and never presented as a successfully rendered new mode.
    pub(crate) fn synchronize_working_precision(&mut self) -> Result<()> {
        let precision = self.engine.project().working_precision;
        let Some(g) = &mut self.graphics else { return Ok(()); };
        if g.renderer.working_precision() == precision { return Ok(()); }
        let context = || format!("project working precision={precision:?} phase=preview-synchronize");
        let change = g.renderer.prepare_working_precision(precision)
            .map_err(|error| crate::Error::with_context(context(), error))?;
        self.sampling.cancel(&mut self.video_frames)?;
        if let Some(change) = change {
            g.renderer.commit_working_precision(change)
                .map_err(|error| crate::Error::with_context(context(), error))?;
        }
        g.scratch.discard_candidate();
        g.actual_preview_quality = None;
        self.preview_memory.changed();
        self.memory_context = None;
        self.view_revision += 1;
        Ok(())
    }

    pub fn composition_error(&self, code: &str, message: &str, details: Value) -> crate::Error {
        motion_core::composition::CompositionError {
            code: code.into(),
            composition: self.engine.project().composition_id.clone(),
            message: message.into(),
            details,
        }
        .into()
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
    let Some(rejection) = error.resource_rejection().copied() else { return Err(error.into()); };
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
    let mut encoder = g.renderer.allocation_command_encoder_for(motion_render::gpu_commands::CommandStage::CompositionAndPresentation, &wgpu::CommandEncoderDescriptor { label: Some("Motion Studio retained preview") })?;
    if let Some(viewport) = retained_viewport {
        g.presenter.encode_view(&g.renderer.queue, &mut encoder, &view, None,
            if image.is_some() { viewport } else { preview_viewport(viewport, g.last_canvas) }, image, g.last_background)?;
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
    g.renderer.submit_allocated(encoder.finish(), |ledger| {
        g.scratch.collect_texture_resources(ledger);
        g.presenter.collect_inspection_resources(ledger);
    })?;
    g.renderer.check_health().map_err(crate::Error::from)?;
    g.renderer.queue.present(output);
    Ok(true)
}

/// Adapt the host's audio fallback to the callback shape `motion-media` expects.
pub(crate) fn audio_decoder(platform: &Arc<dyn Platform>) -> motion_media::DecodeAudio {
    let platform = platform.clone();
    Arc::new(
        move |path: &std::path::Path,
              pcm: &std::path::Path,
              selected: Option<u32>,
              limit: u64,
              check: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>| {
            platform.decode_audio(path, pcm, selected, limit, check)
        },
    )
}

/// Run `operation` against a live session on its owning thread.
pub fn with_session<T>(id: i64, operation: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
    let began = Instant::now();
    let _pointwise_scope = motion_render::PointwiseLogScope::enter(id);
    let _allocation_scope = motion_render::AllocationLogScope::enter(id);
    let _preview_pressure_scope = preview_pressure::Scope::enter(id);
    let _pressure_scope = motion_render::MemoryPressureLogScope::enter(id);
    let _metadata_scope = metadata_observation::Scope::enter();
    let _registry_scope = registry_observation::Scope::enter(id);
    let _relink_scope = crate::ops::media::relink::observation::Scope::enter();
    let mut command_logs = motion_render::gpu_commands::GpuCommandLogScope::new(Some(id));
    let slot = {
        let registry = sessions().lock().unwrap_or_else(|e| e.into_inner());
        registry.get(&id).cloned().ok_or("native session is closed")?
    };
    if slot.owner != thread::current().id() {
        return Err("native session must run on its owning worker thread".into());
    }
    let mut guard = slot.session.lock().unwrap_or_else(|e| e.into_inner());
    let session = guard.as_mut().ok_or("native session is closed")?;
    session.check_thread()?;
    command_logs.watch(session.graphics.as_ref().and_then(|g| g.renderer.command_observer()));
    command_logs.watch(session.editor_renderer.as_ref().and_then(Renderer::command_observer));
    let result = session.recover_plugin_editor(Instant::now()).and_then(|()| operation(session));
    command_logs.watch(session.graphics.as_ref().and_then(|g| g.renderer.command_observer()));
    command_logs.watch(session.editor_renderer.as_ref().and_then(Renderer::command_observer));
    let event = session.last_error_observation.observe(
        session.last_error.as_deref(), session.frame,
        &session.engine.project().composition_id, began,
    );
    // Logging can call host-owned sinks. Release the session before any IO.
    drop(guard);
    command_logs.flush();
    if let Some(event) = event { event.emit(id); }
    result
}

/// Aggregate resource report for every session rooted under `directory`.
/// Entries are sampled individually; concurrent edits or closes may change the
/// report while it is collected. No registry lock spans filesystem or GPU work.
pub fn resource_info(directory: &std::path::Path) -> Result<Value> {
    let began = Instant::now();
    let root = directory.canonicalize().map_err(crate::Error::from)?;
    let entries: Vec<_> = {
        let registry = sessions().lock().unwrap_or_else(|e| e.into_inner());
        registry
            .iter()
            .map(|(&id, slot)| (id, Arc::clone(slot)))
            .collect()
    };
    let mut count = 0u64;
    let mut graphics = 0u64;
    let mut assets = 0u64;
    let mut targets = 0u64;
    let mut unresolved = 0usize;
    let mut first = None;
    for (id, slot) in entries {
        let guard = slot.session.lock().unwrap_or_else(|e| e.into_inner());
        let Some(session) = guard.as_ref() else { continue; };
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
            targets += g.scratch.texture_bytes() + g.views.texture_bytes();
        }
    }
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
    let slot = Arc::new(SessionSlot {
        owner: session.owner,
        session: Mutex::new(Some(session)),
    });
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    sessions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, slot);
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
    let _allocation_scope = motion_render::AllocationLogScope::enter(id);
    let mut command_logs = motion_render::gpu_commands::GpuCommandLogScope::new(Some(id));
    let slot = {
        let mut registry = sessions().lock().unwrap_or_else(|e| e.into_inner());
        if !registry
            .get(&id)
            .is_some_and(|slot| slot.owner == thread::current().id())
        {
            return;
        }
        registry.remove(&id)
    };
    if let Some(slot) = slot {
        let session = slot.session.lock().unwrap_or_else(|e| e.into_inner()).take();
        // Detach, decoder cleanup and GPU destruction run on the owner thread
        // after both guards have gone, even when another reader retains a slot.
        if let Some(mut session) = session {
            command_logs.watch(session.graphics.as_ref().and_then(|g| g.renderer.command_observer()));
            command_logs.watch(session.editor_renderer.as_ref().and_then(Renderer::command_observer));
            session.detach();
        }
    }
}
