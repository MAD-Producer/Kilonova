//! Engine bridge: the desktop worker thread that owns the host session.
//!
//! This mirrors the Android arrangement exactly. One dedicated thread owns the
//! session, every host call is queued to it, and the UI thread only ever reads
//! an immutable snapshot. That keeps a session's `ThreadId` guarantee, its
//! undo history and its GPU resources on one core, and it means the desktop UI
//! cannot touch engine state mid-edit.

use motion_host::{
    ops::{composition, editing, effects, export, geometry, images, media, preview, project},
    video_frame::DecodedFrame,
    Platform,
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        mpsc::{channel, sync_channel, Sender, SyncSender, TrySendError},
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
mod diagnostic_names;
mod failure;
mod ui_paint;
mod frame_capture;
#[cfg(test)]
mod fallback_tests;
#[cfg(test)]
mod queue_tests;
#[cfg(all(test, target_os = "windows"))]
mod scope_tests;
#[cfg(all(test, target_os = "windows"))]
mod pending_video_tests;
#[cfg(all(test, target_os = "windows"))]
mod sampling_playback_tests;
use failure::{ErrorReply, Failure, SessionLease};

/// Maximum JSON payload accepted from the UI. Matches the Android bridge limit
/// so a request that works on one platform works on both.
pub const REQUEST_LIMIT: usize = 256 * 1024;
pub const COMMAND_QUEUE_CAPACITY: usize = 128;

/// CPU draw commands only. All GPU objects stay with the session worker.
pub struct UiFrame {
    pub paint: motion_ui::paint::PaintList,
    pub atlas: Option<AtlasUpload>,
    pub logical_size: [f32; 2],
    pub composition: Option<[f32; 4]>,
    pub composition_image: Option<[f32; 4]>,
    pub window: u64,
    pub node_preview: Option<crate::node_preview::Viewport>,
}
pub struct AtlasUpload {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, PartialEq)]
struct CompositionStamp {
    revision: u64,
    view: u64,
    surface: u64,
    frame: u64,
    window: u64,
    viewport_size: [u32; 2],
    mode: motion_render::PreviewMode,
    tier: motion_render::PreviewTier,
    quality: motion_render::preview_memory::PreviewMemoryQuality,
}
impl CompositionStamp {
    fn new(s: &motion_host::session::Session, ui: &UiFrame, viewport: [f32; 4]) -> Self {
        Self {
            revision: s.engine.revision(),
            view: s.view_revision,
            surface: s.surface_epoch,
            frame: s.frame.to_bits(),
            window: ui.window,
            viewport_size: [viewport[2].ceil() as u32, viewport[3].ceil() as u32],
            mode: s.preview.mode,
            tier: s.preview.tier(),
            quality: s
                .preview_memory
                .quality(s.preview.mode == motion_render::PreviewMode::High),
        }
    }
}
#[derive(Default)]
struct PresentationState {
    retained: Option<CompositionStamp>,
    composition_renders: u64,
    composition_reuses: u64,
    last_present_us: u64,
}

/// Everything the worker can be asked to do.
///
// Poison still discards the cache entry, matching the original policy.
fn recycle_paint_pool(pool: &std::sync::Mutex<Vec<motion_ui::paint::PaintList>>, paint: motion_ui::paint::PaintList, worker: bool) {
    static UI: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
    static WORKER: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
    let poisoned = match pool.lock() {
        Ok(mut pool) => { if pool.len() < 8 { pool.push(paint); } false }
        Err(_) => true,
    };
    if poisoned {
        let (counter, site) = if worker { (&WORKER, "engine.paint.worker_recycle") } else { (&UI, "engine.paint.recycle") };
        counter.failure(site, format_args!("reason=mutex_poisoned fallback=discard_cache_entry"));
    }
}

/// Commands are a closed enum rather than a string channel so a typo in the UI
/// cannot become a runtime error, and so adding a host operation is a
/// compile-time change on both platforms.
#[allow(dead_code)] // Typed SDK bridge; remaining panels will use the other operations.
pub enum Command {
    #[cfg(test)]
    TestPanic {
        during_prepare: bool,
        reply: Sender<Result<Value, String>>,
    },
    #[cfg(test)]
    TestRecoveryPanic {
        root: PathBuf,
        reply: Sender<Result<Value, String>>,
    },
    #[cfg(test)]
    TestDelay {
        delay: std::time::Duration,
        reply: Sender<Result<Value, String>>,
    },
    #[cfg(test)]
    TestWait {entered:Sender<()>,release:std::sync::mpsc::Receiver<()>,reply:Sender<Result<Value,String>>},
    ProjectTicket(Sender<Result<crate::project_lifecycle::Ticket, String>>),
    ChangeProject {
        target: crate::project_lifecycle::Target,
        ticket: crate::project_lifecycle::Ticket,
        save: bool,
        reply: Sender<Result<Value, String>>,
    },
    NodeSource {snapshot:crate::node_editor::EditorSnapshot,reply:Sender<Result<Value,String>>},
    NodeInstall {
        snapshot: crate::node_editor::EditorSnapshot,
        compiled: motion_graph::CompiledGraph,
        canonical: Option<motion_graph::CompiledGraph>,
        reply: Sender<Result<bool, String>>,
    },
    NodeCapture {
        seconds: f64,
        path: PathBuf,
        reply: Sender<Result<Value, String>>,
    },
    EditChecked {
        text: String,
        revision: u64,
        reply: Sender<Result<Value, String>>,
    },
    CaptureUi {
        ui: UiFrame,
        path: PathBuf,
        reply: Sender<Result<Value, String>>,
    },
    Attach {
        target: motion_host::SurfaceTarget,
        reply: Sender<Result<Value, String>>,
    },
    Resize {
        window: u64,
        width: u32,
        height: u32,
        reply: Sender<Result<Value, String>>,
    },
    Present {
        ui: UiFrame,
        reply: Sender<Result<bool, String>>,
    },
    OpenDirectory {
        directory: PathBuf,
        reply: Sender<Result<Value, String>>,
    },
    FloatWindow {
        key: u64,
        target: FloatingSurface,
        reply: Sender<Result<Value, String>>,
    },
    CloseWindow {
        key: u64,
        reply: Sender<Result<Value, String>>,
    },
    State(Sender<Result<Value, String>>),
    Save(Sender<Result<Value, String>>),
    NewProject {
        text: String,
        reply: Sender<Result<Value, String>>,
    },
    OpenProject {
        name: String,
        reply: Sender<Result<Value, String>>,
    },
    ImportProject {
        file: PathBuf,
        reply: Sender<Result<Value, String>>,
    },
    ImportImage {
        file: PathBuf,
        reply: Sender<Result<Value, String>>,
    },
    CreateText {
        text: String,
        size: f32,
        width: u32,
        font: Option<(PathBuf, u32)>,
        reply: Sender<Result<Value, String>>,
    },
    FreezeMovie {
        control: motion_desktop_media::movie::ExportControl,
        reply: Sender<Result<motion_desktop_media::movie::FrozenMovie, String>>,
    },
    RenderPreview {
        frame: Option<f64>,
        reply: Sender<Result<Value, String>>,
    },
    Edit {
        text: String,
        reply: Sender<Result<Value, String>>,
    },
    AlignLayers {
        objects: Vec<u64>,
        edge: motion_host::ops::alignment::Edge,
        target: motion_host::ops::alignment::Target,
        reply: Sender<Result<Value, String>>,
    },
    DistributeLayers {
        objects: Vec<u64>,
        distribution: motion_host::ops::alignment::Distribution,
        reply: Sender<Result<Value, String>>,
    },
    History {
        op: i32,
        reply: Sender<Result<Value, String>>,
    },
    Drag {
        object: i64,
        dx: f64,
        dy: f64,
        width: i32,
        height: i32,
        reply: Sender<Result<Value, String>>,
    },
    Seek {
        frame: f64,
        reply: Sender<Result<Value, String>>,
    },
    View {
        kind: i32,
        reply: Sender<Result<Value, String>>,
    },
    Observe {
        enabled: bool,
        azimuth: f64,
        elevation: f64,
        reply: Sender<Result<Value, String>>,
    },
    Navigate {
        dx: f64,
        dy: f64,
        zoom: f64,
        multi: bool,
        width: i32,
        height: i32,
        reply: Sender<Result<Value, String>>,
    },
    Render {
        frame: f64,
        reply: Sender<Result<bool, String>>,
    },
    ConfigureMemory {
        total: i64,
        guarded: bool,
        reply: Sender<Result<Value, String>>,
    },
    PreviewMode {
        mode: i32,
        thermal: i32,
        reply: Sender<Result<Value, String>>,
    },
    PreviewInfo(Sender<Result<Value, String>>),
    CurveGraph {
        text: String,
        reply: Sender<Result<Value, String>>,
    },
    HitCandidates {
        x: f64,
        y: f64,
        reply: Sender<Result<Value, String>>,
    },
    Plugin {
        request: String,
        reply: Sender<Result<Value, String>>,
    },
    ColorCurveGraph {
        text: String,
        reply: Sender<Result<Value, String>>,
    },
    Composition {
        text: String,
        reply: Sender<Result<Value, String>>,
    },
    Media {
        text: String,
        reply: Sender<Result<Value, String>>,
    },
    MediaPackageLimits(Sender<Result<Value, String>>),
    Capture {
        output: Option<PathBuf>,
        control: motion_desktop_media::movie::ExportControl,
        reply: Sender<Result<Value, String>>,
    },
    Pack(Sender<Result<Value, String>>),
    ImageInfo {
        path: String,
        reply: Sender<Result<Value, String>>,
    },
    PrepareImage {
        root: PathBuf,
        path: String,
        reply: Sender<Result<Value, String>>,
    },
    /// Media capabilities do not need a session, so they answer immediately.
    MediaCapabilities {
        text: String,
        reply: Sender<Result<Value, String>>,
    },
    ResourceInfo {
        directory: PathBuf,
        reply: Sender<Result<Value, String>>,
    },
    /// A decoded video frame requested through the media request pipeline.
    VideoFrame {
        object: u64,
        sequence: u64,
        reply: Sender<Result<ArcFrame, String>>,
    },
    Shutdown,
}

impl Command {
    fn panic_reply(&self) -> Option<ErrorReply> {
        use Command::*;
        let reply = match self {
            ProjectTicket(reply) => ErrorReply::Ticket(reply.clone()),
            NodeInstall { reply, .. } | Present { reply, .. } | Render { reply, .. } => {
                ErrorReply::Bool(reply.clone())
            }
            FreezeMovie { reply, .. } => ErrorReply::Movie(reply.clone()),
            VideoFrame { reply, .. } => ErrorReply::Frame(reply.clone()),
            State(reply) | Save(reply) | PreviewInfo(reply) | MediaPackageLimits(reply)
            | Pack(reply) | Capture { reply, .. }
            | ChangeProject { reply, .. } | NodeCapture { reply, .. }
            | EditChecked { reply, .. } | CaptureUi { reply, .. }
            | Attach { reply, .. } | Resize { reply, .. } | OpenDirectory { reply, .. }
            | FloatWindow { reply, .. } | CloseWindow { reply, .. }
            | NewProject { reply, .. } | OpenProject { reply, .. } | ImportProject { reply, .. }
            | ImportImage { reply, .. } | CreateText { reply, .. } | RenderPreview { reply, .. }
            | Edit { reply, .. } | AlignLayers { reply, .. } | DistributeLayers { reply, .. } | History { reply, .. } | Drag { reply, .. } | Seek { reply, .. }
            | View { reply, .. } | Observe { reply, .. } | Navigate { reply, .. }
            | ConfigureMemory { reply, .. } | PreviewMode { reply, .. } | CurveGraph { reply, .. }
            | HitCandidates { reply, .. } | Plugin { reply, .. } | NodeSource {reply,..} | ColorCurveGraph { reply, .. }
            | Composition { reply, .. } | Media { reply, .. } | ImageInfo { reply, .. }
            | PrepareImage { reply, .. } | MediaCapabilities { reply, .. }
            | ResourceInfo { reply, .. } => ErrorReply::Value(reply.clone()),
            #[cfg(test)]
            TestPanic { reply, .. } | TestRecoveryPanic { reply, .. } | TestDelay { reply, .. } | TestWait {reply,..} => {
                ErrorReply::Value(reply.clone())
            }
            Shutdown => return None,
        };
        Some(reply)
    }
}

/// A decoded frame plus its timing report, as both hosts return it.
#[allow(dead_code)] // Video frame reports are part of the desktop SDK bridge.
pub struct ArcFrame {
    pub frame: std::sync::Arc<DecodedFrame>,
    pub report: Value,
}

impl std::ops::Deref for ArcFrame {
    type Target = DecodedFrame;
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

/// Handle used by the UI thread to talk to the engine.
pub struct Engine {
    sender: SyncSender<QueuedCommand>,
    worker: Option<JoinHandle<()>>,
    platform: Arc<dyn Platform>,
    factory: Arc<std::sync::Mutex<Option<SurfaceFactory>>>,
    failure: Failure,
    paint_pool: Arc<std::sync::Mutex<Vec<motion_ui::paint::PaintList>>>,
    diagnostics: Arc<crate::diagnostics::Diagnostics>,
    in_flight: Arc<Mutex<Option<Flight>>>,
    frame_flight: Arc<Mutex<Option<Flight>>>,
    queue_depth: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
    pending_node_expect: Arc<Mutex<Option<(u64, u64)>>>,
}
#[derive(Clone, Copy)]
struct Flight {
    request: u64,
    operation: &'static str,
    started: Instant,
}
thread_local! {
    static CURRENT_COMMAND: std::cell::Cell<Option<Flight>> = const { std::cell::Cell::new(None) };
}
pub(crate) fn current_command() -> Option<(u64, &'static str, Duration)> {
    CURRENT_COMMAND.get().map(|flight| (flight.request, flight.operation, flight.started.elapsed()))
}
static PRESENT_SURFACE_FAILURES: motion_render::observability::FailureCounter =
    motion_render::observability::FailureCounter::new();
struct QueuedCommand {
    command: Command,
    queued: std::time::Instant,
    request: u64,
    context: crate::diagnostics::Context,
}

#[allow(dead_code)] // SDK bridge methods are intentionally available to future panels.
impl Engine {
    /// Inspect executing work without asking the (possibly blocked) worker.
    pub fn in_flight(&self) -> Option<(String, Duration)> {
        self.in_flight.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
            .map(|flight| (flight.operation.to_owned(), flight.started.elapsed()))
    }
    /// Independent export work remains inspectable while the serial worker is free.
    pub fn frame_capture_in_flight(&self) -> Option<(u64, Duration)> {
        static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(&self.frame_flight, &FAILURES, "frame.flight.read", |flight|
            flight.map(|flight| (flight.request, flight.started.elapsed())))
    }
    pub fn queue_depth(&self) -> usize {
        self.queue_depth.load(Ordering::Acquire)
    }
    pub fn diagnostics(&self) -> &Arc<crate::diagnostics::Diagnostics> {
        &self.diagnostics
    }
    pub fn take_paint(&self) -> motion_ui::paint::PaintList {
        static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        let (paint, poisoned) = match self.paint_pool.lock() {
            Ok(mut pool) => (pool.pop().unwrap_or_default(), false),
            Err(_) => (motion_ui::paint::PaintList::default(), true),
        };
        if poisoned { FAILURES.failure("engine.paint.take", format_args!("reason=mutex_poisoned fallback=empty_paint")); }
        paint
    }
    pub fn recycle_paint(&self, mut paint: motion_ui::paint::PaintList) {
        paint.clear();
        recycle_paint_pool(&self.paint_pool, paint, false);
    }
    pub fn node_preview_available(&self) -> Result<(), String> {
        self.check_running()?;
        let factory = self
            .factory
            .lock()
            .map_err(|_| "surface factory lock poisoned")?;
        factory.as_ref().ok_or("node preview GPU is not attached")?;
        Ok(())
    }
    pub fn node_expect(&self, stamp: (u64, u64)) -> Result<(), String> {
        self.check_running()?;
        *self.pending_node_expect.lock().map_err(|_| "node expectation lock poisoned")? = Some(stamp);
        Ok(())
    }
    /// Stop accepting work and drain accepted commands. This also works while
    /// an MCP reader still owns an Arc; it does not interrupt a native GPU call.
    pub fn request_shutdown(&self) -> Result<(), String> {
        let context = self.diagnostics.context();
        let mut gate_error = None;
        let queued = {
            let _gate = self.in_flight.lock().unwrap_or_else(|e| {
                gate_error = Some("engine dispatch lock poisoned during shutdown".to_owned());
                e.into_inner()
            });
            if self.shutdown.swap(true, Ordering::AcqRel) {
                return gate_error.map_or(Ok(()), Err);
            }
            // Wake an idle worker. A full queue already wakes it; after draining
            // it observes the flag without requiring another free queue slot.
            let queued = self.sender.try_send(QueuedCommand {
                command: Command::Shutdown,
                queued: Instant::now(),
                request: self.diagnostics.next_request(),
                context,
            });
            if queued.is_ok() { self.queue_depth.fetch_add(1, Ordering::AcqRel); }
            queued
        };
        match queued {
            Ok(()) => {},
            Err(TrySendError::Full(_)) => {
                log::debug!("shutdown will follow the accepted full command queue");
            },
            Err(TrySendError::Disconnected(_)) => return Err(self.stopped_error()),
        }
        gate_error.map_or(Ok(()), Err)
    }
    pub fn node_install(
        &self,
        snapshot: crate::node_editor::EditorSnapshot,
        compiled: motion_graph::CompiledGraph,
    ) -> Result<std::sync::mpsc::Receiver<Result<bool, String>>, String> {
        self.request(|reply| Command::NodeInstall { snapshot, compiled, canonical:None,reply })
    }
    pub fn node_install_bound(&self,snapshot:crate::node_editor::EditorSnapshot,compiled:motion_graph::CompiledGraph,canonical:Option<motion_graph::CompiledGraph>)->Result<std::sync::mpsc::Receiver<Result<bool,String>>,String>{
        self.request(|reply|Command::NodeInstall{snapshot,compiled,canonical,reply})
    }
    pub fn node_source(&self,snapshot:crate::node_editor::EditorSnapshot)->Result<std::sync::mpsc::Receiver<Result<Value,String>>,String>{
        self.request(|reply|Command::NodeSource{snapshot,reply})
    }

    pub fn node_capture(&self, seconds: f64, path: PathBuf) -> Result<Value, String> {
        self.call(|reply| Command::NodeCapture {
            seconds,
            path,
            reply,
        })
    }
    /// Start the worker thread and open a project at `root`.
    pub fn start(
        root: PathBuf,
        platform: std::sync::Arc<dyn Platform>,
        total_memory: u64,
    ) -> Result<Self, String> {
        let total_memory = i64::try_from(total_memory).map_err(|_| {
            let error = "physical memory exceeds the signed 64-bit preview budget".to_string();
            log::error!("worker startup rejected: root={} reason={error}", root.display());
            error
        })?;
        let startup_root = root.clone();
        let startup_began = Instant::now();
        log::info!("worker startup phase=lock root={}", startup_root.display());
        crate::logging::flush();
        let desktop_platform = platform.clone();
        let diagnostics = crate::diagnostics::Diagnostics::new(
            if cfg!(test) {
                None
            } else {
                Some(
                    std::env::var_os("MOTION_DIAGNOSTICS_PATH")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| {
                            crate::directories().join("diagnostics/last-stalls.json")
                        }),
                )
            },
            if cfg!(test) {
                None
            } else {
                std::env::var("MOTION_SENTRY_DSN")
                    .ok()
                    .or_else(|| option_env!("MOTION_SENTRY_DSN").map(str::to_owned))
            },
        );
        let worker_diagnostics = diagnostics.clone();
        let paint_pool = Arc::new(std::sync::Mutex::new(Vec::new()));
        let worker_paints = paint_pool.clone();
        let failure: Failure = Arc::new(std::sync::Mutex::new(None));
        let worker_failure = failure.clone();
        let factory = Arc::new(std::sync::Mutex::new(None));
        let worker_factory = factory.clone();
        let in_flight = Arc::new(Mutex::new(None));
        let frame_flight = Arc::new(Mutex::new(None));
        let worker_frame_flight = frame_flight.clone();
        let worker_flight = in_flight.clone();
        let queue_depth = Arc::new(AtomicUsize::new(0));
        let worker_depth = queue_depth.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = shutdown.clone();
        let pending_node_expect = Arc::new(Mutex::new(None));
        let worker_node_expect = pending_node_expect.clone();
        let (sender, receiver) = sync_channel::<QueuedCommand>(COMMAND_QUEUE_CAPACITY);
        let (ready, opened) = channel::<Result<i64, String>>();
        let worker = std::thread::Builder::new()
            .name("motion-engine".into())
            .spawn(move || {
                let mut guard = match ProjectGuard::acquire(&root) {
                    Ok(guard) => guard,
                    Err(e) => {
                        log::error!("worker startup lock failed: root={} reason={e}", root.display());
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                let initial = if root.join("project.json").exists() {
                    String::new()
                } else {
                    let mut project = motion_core::Project::new(1920, 1080, 30, 300)
                        .expect("valid default composition");
                    project.name = "Untitled Project".into();
                    serde_json::to_string(&project).expect("serializable default composition")
                };
                log::info!("worker startup phase=session root={}", root.display());
                crate::logging::flush();
                let id = match project::create(root, &initial, platform.clone()) {
                    Ok(id) => id,
                    Err(error) => {
                        log::error!("worker startup session failed: {error}");
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                let lease = SessionLease(id);
                guard.initial = match motion_host::session::with_session(id, |s| Ok(s.engine.snapshot())) {
                    Ok(snapshot) => Some(snapshot),
                    Err(error) => {
                        log::error!("worker initial snapshot failed: {error}");
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                // The scratch budget is a device property, so apply it before
                // the first render rather than after a visible hitch.
                log::info!("worker startup phase=memory session={id}");
                crate::logging::flush();
                if let Err(error) = preview::configure_memory(id, total_memory, false) {
                    log::error!("worker startup memory failed: session={id} reason={error}");
                    let _ = ready.send(Err(error));
                    return;
                }
                if ready.send(Ok(id)).is_err() {
                    log::warn!("worker startup receiver closed; releasing session={id}");
                    return;
                }
                let mut painter = None;
                let mut node_preview = NodePreviewState::default();
                let mut presentation = PresentationState::default();
                let mut frame_capture = frame_capture::Worker::new(worker_frame_flight);
                let mut windows = std::collections::HashMap::new();
                let mut failed_reply = None;
                let mut channel_closed = true;
                loop {
                    let drained = {
                        let _gate = match worker_flight.lock() {
                            Ok(gate) => gate,
                            Err(poisoned) => {
                                drop(poisoned);
                                let error = "engine dispatch lock poisoned while checking shutdown";
                                log::error!("{error}");
                                static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
                                crate::fallback_observation::recovered(&worker_failure, &FAILURES, "engine.failure.shutdown", |value| *value = Some(error.into()));
                                channel_closed = false;
                                break;
                            }
                        };
                        worker_shutdown.load(Ordering::Acquire) && worker_depth.load(Ordering::Acquire) == 0
                    };
                    if drained {
                        channel_closed = false;
                        break;
                    }
                    let Ok(QueuedCommand {command,queued,request,context}) = receiver.recv() else { break; };
                    let began=std::time::Instant::now();
                    let operation=command.diagnostic_name();
                    let flight = Flight { request, operation, started: began };
                    {
                        // dispatch holds this same short gate across send + increment.
                        // recv can wake before send returns; never decrement first.
                        let mut current = worker_flight.lock().unwrap_or_else(|e| e.into_inner());
                        worker_depth.fetch_sub(1, Ordering::AcqRel);
                        *current = Some(flight);
                    }
                    CURRENT_COMMAND.set(Some(flight));
                    if operation != "present" {
                        log::info!("start #{} {}", request, operation);
                        crate::logging::flush();
                    }
                    let on_panic = command.panic_reply();
                    let handled=motion_host::ops::catch_panic(|| {
                        // Keep this inside the existing panic/reply boundary.
                        let pending = worker_node_expect.lock().map_err(|_| "node expectation lock poisoned")?;
                        let mut pending = pending;
                        if let Some(stamp) = pending.take() { node_preview.desired = stamp; }
                        drop(pending);
                        Ok(handle(
                        id,
                        &platform,
                        &mut painter,
                        &mut windows,
                        &mut guard,
                        &mut node_preview,
                        &mut presentation,
                        &mut frame_capture,
                        &worker_paints,
                        &worker_diagnostics,
                        command,
                    ))}).and_then(|result| result);
                    worker_diagnostics.record(request,"worker",operation,queued.elapsed(),Some(began.duration_since(queued)),Some(began.elapsed()),context);
                    *worker_flight.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    CURRENT_COMMAND.set(None);
                    if operation != "present" || began.elapsed() >= Duration::from_millis(100) {
                        log::debug!("done #{} {} {}ms", request, operation, began.elapsed().as_millis());
                    }
                    match handled {
                        Ok(true) => {
                            log::info!("worker shutdown requested: session={id}");
                            channel_closed = false;
                            break;
                        },
                        Ok(false) => {},
                        Err(reason) => {
                            channel_closed = false;
                            log::error!("panic #{} {} {}ms: {}", request, operation, began.elapsed().as_millis(), reason);
                            let recovery = match motion_host::ops::catch_panic(|| failure::recovery(id)) {
                                Ok(Ok(path)) => format!("recovery JSON: {}", path.display()),
                                Ok(Err(error)) | Err(error) => format!("recovery unavailable: {error}"),
                            };
                            let message = format!("engine session closed after native panic: {reason}; {recovery}; reopen the project");
                            log::error!("[motion-engine] {message}");
                            {
                                static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
                                crate::fallback_observation::recovered(&worker_failure, &FAILURES, "engine.failure.publish", |value| *value = Some(message.clone()));
                            }
                            failed_reply = on_panic.map(|reply| (reply, message));
                            break;
                        }
                    }
                }
                if channel_closed { log::error!("worker channel closed"); }
                worker_depth.store(0, Ordering::Release);
                drop(windows);
                drop(painter);
                drop(node_preview);
                drop(frame_capture);
                {
                    static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
                    crate::fallback_observation::recovered(&worker_factory, &FAILURES, "engine.surface.shutdown", |factory| { factory.take(); });
                }
                drop(lease);
                drop(guard);
                log::info!("worker resources released: session={id}");
                if let Some((reply, error)) = failed_reply { reply.send(error); }
            })
            .map_err(|error| {
                log::error!("worker spawn failed: root={} reason={error}", startup_root.display());
                error.to_string()
            })?;
        let _id = match opened.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(Ok(id)) => id,
            result => {
                let error = match result {
                    Ok(Err(error)) => error,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) =>
                        "engine startup timed out; background initialization may still hold the project lock".into(),
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) =>
                        "engine thread stopped during startup".into(),
                    Ok(Ok(_)) => unreachable!(),
                };
                log::error!("worker startup failed: root={} {}ms reason={error}", startup_root.display(), startup_began.elapsed().as_millis());
                return Err(error);
            }
        };
        log::info!("worker ready: session={_id} root={} {}ms", startup_root.display(), startup_began.elapsed().as_millis());
        Ok(Self {
            sender,
            worker: Some(worker),
            platform: desktop_platform,
            factory,
            failure,
            paint_pool,
            diagnostics,
            in_flight,
            frame_flight,
            queue_depth,
            shutdown,
            pending_node_expect,
        })
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.dispatch(command).map(|_| ())
    }
    fn dispatch(&self, command: Command) -> Result<u64, String> {
        self.check_running()?;
        let request = self.diagnostics.next_request();
        let operation = command.diagnostic_name();
        let context = self.diagnostics.context();
        enum Rejected {
            Stopped(String),
            Finished,
            Send(TrySendError<QueuedCommand>),
        }
        let mut failure_poisoned = false;
        static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        let queued = crate::fallback_observation::recovered(&self.in_flight, &FAILURES, "engine.flight.dispatch", |_| {
            // Repeat the running check under the shutdown gate, but defer all
            // logging (including poisoned failure-state evidence) until unlock.
            let failure = self.failure.lock().unwrap_or_else(|error| {
                failure_poisoned = true;
                error.into_inner()
            }).clone();
            if let Some(error) = failure { return Err(Rejected::Stopped(error)); }
            if self.shutdown.load(Ordering::Acquire) {
                return Err(Rejected::Stopped("engine shutdown requested; new commands are not accepted".into()));
            }
            if self.worker.as_ref().is_some_and(|worker| worker.is_finished()) {
                return Err(Rejected::Finished);
            }
            self.sender.try_send(QueuedCommand {
                command, queued: Instant::now(), request, context,
            }).map(|()| self.queue_depth.fetch_add(1, Ordering::AcqRel) + 1).map_err(Rejected::Send)
        });
        if failure_poisoned {
            static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
            FAILURES.failure("engine.failure.dispatch", format_args!("reason=mutex_poisoned policy=into_inner"));
        }
        let depth = queued.map_err(|error| match error {
            Rejected::Stopped(error) => error,
            Rejected::Finished | Rejected::Send(TrySendError::Disconnected(_)) => self.stopped_error(),
            Rejected::Send(TrySendError::Full(_)) => format!("engine command queue is full (capacity {COMMAND_QUEUE_CAPACITY}); command was not queued"),
        })?;
        // Present shares this queue; keep its healthy per-frame path silent.
        if operation != "present" {
            log::debug!("dispatch #{} {} queue_depth={}", request, operation, depth);
        }
        Ok(request)
    }

    /// Queue work without blocking the caller.
    pub fn request<T>(
        &self,
        make: impl FnOnce(Sender<Result<T, String>>) -> Command,
    ) -> Result<std::sync::mpsc::Receiver<Result<T, String>>, String> {
        let (reply, receive) = channel();
        self.send(make(reply))?;
        Ok(receive)
    }
    pub fn call<T>(
        &self,
        make: impl FnOnce(Sender<Result<T, String>>) -> Command,
    ) -> Result<T, String> {
        self.call_with_timeout(make, std::time::Duration::from_secs(60))
    }
    fn call_with_timeout<T>(
        &self,
        make: impl FnOnce(Sender<Result<T, String>>) -> Command,
        timeout: std::time::Duration,
    ) -> Result<T, String> {
        let (reply, receive) = channel();
        let began = std::time::Instant::now();
        let command = make(reply);
        let operation = command.diagnostic_name();
        let request = self.dispatch(command)?;
        let received = receive.recv_timeout(timeout);
        self.diagnostics.record(
            request,
            "blocking_wait",
            operation,
            began.elapsed(),
            None,
            None,
            self.diagnostics.context(),
        );
        let result = received
            .map_err(|error| match error {
                std::sync::mpsc::RecvTimeoutError::Disconnected => self.stopped_error(),
                std::sync::mpsc::RecvTimeoutError::Timeout => {
                    log::warn!("timeout #{} {} waited={}ms", request, operation, began.elapsed().as_millis());
                    format!("{operation} timed out; its result is still pending; check project state before retrying")
                },
            })?;
        #[cfg(debug_assertions)]
        if began.elapsed().as_millis() >= 16 {
            log::debug!(
                "[motion-timing] blocking {operation}: {} us",
                began.elapsed().as_micros()
            );
        }
        result
    }

    pub fn state(&self) -> Result<Value, String> {
        self.call(Command::State)
    }
    pub fn save(&self) -> Result<Value, String> {
        self.call(Command::Save)
    }
    pub fn new_project(&self, text: String) -> Result<Value, String> {
        self.call(|reply| Command::NewProject { text, reply })
    }
    pub fn open_project(&self, name: String) -> Result<Value, String> {
        self.call(|reply| Command::OpenProject { name, reply })
    }
    pub fn import_project(&self, file: PathBuf) -> Result<Value, String> {
        self.call(|reply| Command::ImportProject { file, reply })
    }
    pub fn edit(&self, text: String) -> Result<Value, String> {
        if text.len() > REQUEST_LIMIT {
            return Err("edit request exceeds 256 KiB".into());
        }
        self.call(|reply| Command::Edit { text, reply })
    }
    pub fn align_layers_async(&self, objects:Vec<u64>, edge:motion_host::ops::alignment::Edge,
        target:motion_host::ops::alignment::Target) -> Result<std::sync::mpsc::Receiver<Result<Value,String>>,String> {
        if objects.len()>motion_core::MAX_LAYERS {return Err("alignment selection exceeds layer limit".into());}
        self.request(|reply|Command::AlignLayers{objects,edge,target,reply})
    }
    pub fn distribute_layers_async(&self,objects:Vec<u64>,distribution:motion_host::ops::alignment::Distribution)
        -> Result<std::sync::mpsc::Receiver<Result<Value,String>>,String> {
        if objects.len()>motion_core::MAX_LAYERS {return Err("distribution selection exceeds layer limit".into());}
        self.request(|reply|Command::DistributeLayers{objects,distribution,reply})
    }
    pub fn edit_async(
        &self,
        text: String,
    ) -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        if text.len() > REQUEST_LIMIT {
            return Err("edit request exceeds 256 KiB".into());
        }
        self.request(|reply| Command::Edit { text, reply })
    }
    pub fn edit_at_revision(&self, text: String, revision: u64) -> Result<Value, String> {
        if text.len() > REQUEST_LIMIT {
            return Err("edit request exceeds 256 KiB".into());
        }
        self.call(|reply| Command::EditChecked {
            text,
            revision,
            reply,
        })
    }
    pub fn edit_async_at_revision(&self, text: String, revision: u64)
        -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        if text.len() > REQUEST_LIMIT { return Err("edit request exceeds 256 KiB".into()); }
        self.request(|reply| Command::EditChecked { text, revision, reply })
    }
    pub fn capture_ui(&self, ui: UiFrame, path: PathBuf) -> Result<Value, String> {
        self.call(|reply| Command::CaptureUi { ui, path, reply })
    }
    pub fn history(&self, op: i32) -> Result<Value, String> {
        self.call(|reply| Command::History { op, reply })
    }
    pub fn history_async(
        &self,
        op: i32,
    ) -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        self.request(|reply| Command::History { op, reply })
    }
    pub fn drag(
        &self,
        object: i64,
        dx: f64,
        dy: f64,
        width: i32,
        height: i32,
    ) -> Result<Value, String> {
        self.call(|reply| Command::Drag {
            object,
            dx,
            dy,
            width,
            height,
            reply,
        })
    }
    pub fn seek(&self, frame: f64) -> Result<Value, String> {
        self.call(|reply| Command::Seek { frame, reply })
    }
    pub fn drag_async(
        &self,
        object: i64,
        delta: [f32; 2],
        size: [i32; 2],
    ) -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        self.request(|reply| Command::Drag {
            object,
            dx: delta[0] as f64,
            dy: delta[1] as f64,
            width: size[0],
            height: size[1],
            reply,
        })
    }
    pub fn seek_async(
        &self,
        frame: f64,
    ) -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        self.request(|reply| Command::Seek { frame, reply })
    }
    pub fn view(&self, kind: i32) -> Result<Value, String> {
        self.call(|reply| Command::View { kind, reply })
    }
    pub fn observe(&self, enabled: bool, azimuth: f64, elevation: f64) -> Result<Value, String> {
        self.call(|reply| Command::Observe {
            enabled,
            azimuth,
            elevation,
            reply,
        })
    }
    pub fn navigate(
        &self,
        dx: f64,
        dy: f64,
        zoom: f64,
        multi: bool,
        width: i32,
        height: i32,
    ) -> Result<Value, String> {
        self.call(|reply| Command::Navigate {
            dx,
            dy,
            zoom,
            multi,
            width,
            height,
            reply,
        })
    }
    pub fn view_async(
        &self,
        kind: i32,
    ) -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        self.request(|reply| Command::View { kind, reply })
    }
    pub fn observe_async(
        &self,
        enabled: bool,
        azimuth: f64,
        elevation: f64,
    ) -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        self.request(|reply| Command::Observe {
            enabled,
            azimuth,
            elevation,
            reply,
        })
    }
    pub fn navigate_async(
        &self,
        dx: f64,
        dy: f64,
        zoom: f64,
        multi: bool,
        width: i32,
        height: i32,
    ) -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        self.request(|reply| Command::Navigate {
            dx,
            dy,
            zoom,
            multi,
            width,
            height,
            reply,
        })
    }
    pub fn render(&self, frame: f64) -> Result<bool, String> {
        self.call(|reply| Command::Render { frame, reply })
    }
    pub fn configure_memory(&self, total: i64, guarded: bool) -> Result<Value, String> {
        self.call(|reply| Command::ConfigureMemory {
            total,
            guarded,
            reply,
        })
    }
    pub fn preview_mode(&self, mode: i32, thermal: i32) -> Result<Value, String> {
        self.call(|reply| Command::PreviewMode {
            mode,
            thermal,
            reply,
        })
    }
    pub fn preview_info(&self) -> Result<Value, String> {
        self.call(Command::PreviewInfo)
    }
    pub fn curve_graph(&self, text: String) -> Result<Value, String> {
        self.call(|reply| Command::CurveGraph { text, reply })
    }
    pub fn hit_candidates(&self, x: f64, y: f64) -> Result<Value, String> {
        self.call(|reply| Command::HitCandidates { x, y, reply })
    }
    pub fn plugin_async(&self, request: String) -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        if request.len() > REQUEST_LIMIT { return Err("plugin request exceeds 256 KiB".into()); }
        self.request(|reply| Command::Plugin { request, reply })
    }
    pub fn plugin(&self, request: String) -> Result<Value, String> {
        self.call(|reply| Command::Plugin { request, reply })
    }
    pub fn color_curve_graph(&self, text: String) -> Result<Value, String> {
        self.call(|reply| Command::ColorCurveGraph { text, reply })
    }
    pub fn composition(&self, text: String) -> Result<Value, String> {
        self.call(|reply| Command::Composition { text, reply })
    }
    pub fn media(&self, text: String) -> Result<Value, String> {
        self.call(|reply| Command::Media { text, reply })
    }
    pub fn media_package_limits(&self) -> Result<Value, String> {
        self.call(Command::MediaPackageLimits)
    }
    pub fn media_capabilities(&self, text: String) -> Result<Value, String> {
        self.call(|reply| Command::MediaCapabilities { text, reply })
    }
    pub fn capture(&self) -> Result<Value, String> {
        self.call(|reply| Command::Capture { output: None,
            control: motion_desktop_media::movie::ExportControl::default(), reply })
    }
    pub fn capture_async(&self, output: PathBuf, control: motion_desktop_media::movie::ExportControl)
        -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        self.request(|reply| Command::Capture { output: Some(output), control, reply })
    }
    pub fn render_preview(&self, frame: Option<f64>) -> Result<Value, String> {
        self.call(|reply| Command::RenderPreview { frame, reply })
    }
    pub fn pack(&self) -> Result<Value, String> {
        self.call(Command::Pack)
    }
    pub fn image_info(&self, path: String) -> Result<Value, String> {
        self.call(|reply| Command::ImageInfo { path, reply })
    }
    pub fn import_image(&self, file: PathBuf) -> Result<Value, String> {
        self.call(|reply| Command::ImportImage { file, reply })
    }
    pub fn create_text(
        &self,
        text: String,
        size: f32,
        width: u32,
        font: Option<(PathBuf, u32)>,
    ) -> Result<Value, String> {
        self.call(|reply| Command::CreateText {
            text,
            size,
            width,
            font,
            reply,
        })
    }
    pub fn freeze_movie(
        &self,
        control: motion_desktop_media::movie::ExportControl,
    ) -> Result<
        std::sync::mpsc::Receiver<Result<motion_desktop_media::movie::FrozenMovie, String>>,
        String,
    > {
        if !cfg!(feature = "ffmpeg") {
            return Err("this build does not support MP4 export".into());
        }
        self.request(|reply| Command::FreezeMovie { control, reply })
    }
    pub fn prepare_image(&self, root: PathBuf, path: String) -> Result<Value, String> {
        self.call(|reply| Command::PrepareImage { root, path, reply })
    }
    pub fn resource_info(&self, directory: PathBuf) -> Result<Value, String> {
        self.call(|reply| Command::ResourceInfo { directory, reply })
    }
    pub fn video_frame(&self, object: u64, sequence: u64) -> Result<ArcFrame, String> {
        self.call(|reply| Command::VideoFrame {
            object,
            sequence,
            reply,
        })
    }

    pub fn open_directory(&self, directory: PathBuf) -> Result<Value, String> {
        self.call(|reply| Command::OpenDirectory { directory, reply })
    }
    pub fn attach_async(
        &self,
        window: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        width: u32,
        height: u32,
    ) -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        self.check_running()?;
        // Winit exposes Win32 handles only on the GUI thread. Build a surface
        // here, then transfer rendering ownership to the session worker.
        let target = self.platform.attach_surface(window, width, height)?;
        self.diagnostics.set_gpu(target.renderer.adapter.get_info());
        *self
            .factory
            .lock()
            .map_err(|_| "surface factory lock poisoned")? = Some(SurfaceFactory {
            instance: target.instance.clone(),
            adapter: target.renderer.adapter.clone(),
            device: target.renderer.device.clone(),
            queue: target.renderer.queue.clone(),
            config: target.config.clone(),
        });
        self.request(|reply| Command::Attach { target, reply })
    }
    pub fn resize_async(
        &self,
        window: u64,
        width: u32,
        height: u32,
    ) -> Result<std::sync::mpsc::Receiver<Result<Value, String>>, String> {
        self.request(|reply| Command::Resize {
            window,
            width,
            height,
            reply,
        })
    }
    pub fn float_window(
        &self,
        key: u64,
        window: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        width: u32,
        height: u32,
    ) -> Result<Value, String> {
        self.check_running()?;
        let target = {
            let factory = self
                .factory
                .lock()
                .map_err(|_| "surface factory lock poisoned")?;
            let factory = factory.as_ref().ok_or("desktop surface is not attached")?;
            let surface = factory
                .instance
                .create_surface(window)
                .map_err(|e| e.to_string())?;
            if !surface
                .get_capabilities(&factory.adapter)
                .formats
                .contains(&factory.config.format)
            {
                return Err("floating surface format is incompatible".into());
            }
            let mut config = factory.config.clone();
            config.width = width;
            config.height = height;
            surface.configure(&factory.device, &config);
            let target = FloatingSurface { surface, config };
            target
        };
        self.call(|reply| Command::FloatWindow { key, target, reply })
    }
    pub fn close_window(&self, key: u64) -> Result<Value, String> {
        self.call(|reply| Command::CloseWindow { key, reply })
    }
    pub fn present_async(
        &self,
        ui: UiFrame,
    ) -> Result<std::sync::mpsc::Receiver<Result<bool, String>>, String> {
        self.request(|reply| Command::Present { ui, reply })
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Err(error) = self.request_shutdown() {
            log::error!("engine shutdown: {error}");
        }
        if let Some(worker) = self.worker.take() {
            if let Err(payload) = worker.join() {
                let error = format!("engine shutdown failed: worker panicked outside its command boundary: {}", motion_media::panic_reason(payload));
                *self.failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(error.clone());
                log::error!("{error}");
            }
        }
    }
}

fn clear_target(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        multiview_mask: None,
        label: Some("floating panel background"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            depth_slice: None,
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.01,
                    g: 0.01,
                    b: 0.012,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
}


/// Carry the originating command into the detached freeze worker. The engine's
/// in_flight still describes only its serial worker, not this background task.
fn spawn_movie_freeze<T: Send + 'static>(
    reply: Sender<Result<T, String>>,
    freeze: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<(), String> {
    let flight = CURRENT_COMMAND.get();
    std::thread::Builder::new().name("motion-freeze-movie".into()).spawn(move || {
        CURRENT_COMMAND.set(flight);
        let began = Instant::now();
        if let Some(flight) = flight {
            log::info!("async start #{} {}", flight.request, flight.operation);
            crate::logging::flush();
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(freeze))
            .unwrap_or_else(|payload| Err(format!("movie freeze worker failed: {}", motion_media::panic_reason(payload))));
        respond(reply, result);
        if let Some(flight) = flight {
            log::debug!("async done #{} {} {}ms", flight.request, flight.operation, began.elapsed().as_millis());
        }
        CURRENT_COMMAND.set(None);
    }).map(|_| ()).map_err(|error| error.to_string())
}

fn respond<T>(reply: Sender<Result<T, String>>, value: Result<T, String>) {
    if let Err(reason) = &value {
        if let Some(flight) = CURRENT_COMMAND.get() {
            log::error!("error #{} {} {}ms: {}", flight.request, flight.operation,
                flight.started.elapsed().as_millis(), reason);
        }
    }
    let _ = reply.send(value);
}

/// Execute one command against the session.
///
/// Returns true when the worker should stop. Reply channels are best effort:
/// a UI that has already gone away must not keep the engine blocked.
struct ProjectGuard {
    root: PathBuf,
    _file: std::fs::File,
    initial: Option<motion_core::Project>,
}
impl ProjectGuard {
    fn acquire(root: &std::path::Path) -> Result<Self, String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(".motion-editor.lock"))
            .map_err(|e| e.to_string())?;
        file.try_lock()
            .map_err(|e| format!("project is already open or cannot be locked: {e}"))?;
        Ok(Self {
            root,
            _file: file,
            initial: None,
        })
    }
    fn ticket(&self, id: i64) -> Result<crate::project_lifecycle::Ticket, String> {
        let saved = if self
            .root
            .join("project.json")
            .try_exists()
            .map_err(|e| e.to_string())?
        {
            Some(motion_core::storage::load_document(&self.root).map_err(|e| e.to_string())?)
        } else {
            self.initial.clone()
        };
        motion_host::session::with_session(id, |s| {
            Ok(crate::project_lifecycle::Ticket {
                root: self.root.clone(),
                revision: s.engine.revision(),
                dirty: saved.as_ref() != Some(s.engine.project()),
                name: s.engine.project().name.clone(),
            })
        })
    }
    fn changed(&mut self, result: Result<Value, String>) -> Result<Value, String> {
        let state = result?;
        let root = PathBuf::from(state["root"].as_str().ok_or("missing project root")?)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if root != self.root {
            *self = Self::acquire(&root)?;
        }
        Ok(state)
    }
}

struct SurfaceFactory {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

pub struct FloatingSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}
#[derive(Default)]
struct NodePreviewState {
    desired: (u64, u64),
    active: Option<crate::node_preview::NodePreview>,
    installs: u64,
}

fn handle(
    id: i64,
    platform: &Arc<dyn Platform>,
    painter: &mut Option<motion_ui::Painter>,
    windows: &mut std::collections::HashMap<u64, FloatingSurface>,
    guard: &mut ProjectGuard,
    node_preview: &mut NodePreviewState,
    presentation: &mut PresentationState,
    frame_capture: &mut frame_capture::Worker,
    paints: &Arc<std::sync::Mutex<Vec<motion_ui::paint::PaintList>>>,
    diagnostics: &crate::diagnostics::Diagnostics,
    command: Command,
) -> bool {
    // Capture, project operations and view changes can overwrite the shared
    // scratch texture. A read-only state request leaves it reusable.
    if !matches!(
        &command,
        Command::Present { .. } | Command::State(_) | Command::PreviewInfo(_) | Command::Capture { .. }
    ) {
        presentation.retained = None;
    }
    match command {
        #[cfg(test)]
        Command::TestPanic { during_prepare, reply } => respond(reply, motion_host::session::with_session(id, |session| {
            if during_prepare {
                session.engine.apply_batch_with_prepare(vec![motion_core::Command::SetColor { object: 0, value: [1.,0.,0.,1.] }], |_| panic!("injected preparation panic")).map_err(|error| error.to_string())?;
            }
            panic!("injected worker panic");
        })),
        #[cfg(test)]
        Command::TestDelay { delay, reply } => {
            std::thread::sleep(delay);
            respond(reply, project::state(id));
        },
        #[cfg(test)]
        Command::TestWait {entered,release,reply}=> {
            let _=entered.send(());
            respond(reply,release.recv_timeout(std::time::Duration::from_secs(5)).map(|_|serde_json::json!({"ready":true})).map_err(|e|e.to_string()));
        },
        #[cfg(test)]
        Command::TestRecoveryPanic { root, reply } => respond(reply, motion_host::session::with_session(id, |session| {
            session.root = root;
            panic!("injected panic with unavailable recovery directory");
        })),
        Command::ProjectTicket(reply) => respond(reply, guard.ticket(id)),
        Command::ChangeProject { target, ticket, save, reply } => respond(reply, (|| {
            motion_host::session::with_session(id, |s| {
                if guard.root != ticket.root || s.engine.revision() != ticket.revision {
                    return Err("project changed while confirming; review the current changes again".into());
                }
                Ok(())
            })?;
            let choice = if save { project::PreviousProject::Save } else { project::PreviousProject::Discard };
            match target {
                crate::project_lifecycle::Target::Close => {
                    if save { project::save(id) } else { project::state(id) }
                }
                crate::project_lifecycle::Target::New(text) => {
                    guard.changed(project::new_project_with_choice(id, &text, choice))
                }
                crate::project_lifecycle::Target::Import(file) => {
                    guard.changed(project::import_project_with_choice(id, &file, choice))
                }
                crate::project_lifecycle::Target::Open(directory) => {
                    let directory = directory.canonicalize().map_err(|e| {
                        format!("open project {}: {e}", directory.display())
                    })?;
                    if directory == guard.root {
                        return if save { project::save(id) } else {
                            project::open_directory_with_choice(id, &directory, choice)
                        };
                    }
                    let next = ProjectGuard::acquire(&directory)?;
                    let result = project::open_directory_with_choice(id, &directory, choice)?;
                    *guard = next;
                    Ok(result)
                }
            }
        })()),
        Command::NodeSource{snapshot,reply}=>respond(reply,motion_host::session::with_session(id,|s|crate::node_layer::persist(s,&snapshot))),
        Command::NodeInstall { snapshot, compiled, canonical,reply } => respond(reply, (|| {
            // All pipeline creation and scope push/pop stay on this worker,
            // alongside effects and presentation. Reject stale CPU work first.
            if snapshot.preview_stamp() != node_preview.desired {
                log::debug!("node install superseded received={:?} desired={:?} revision={}", snapshot.preview_stamp(), node_preview.desired, snapshot.revision);
                return Ok(false);
            }
            let preview = motion_host::session::with_session(id, |session| {
                let graphics = session.graphics.as_ref().ok_or("node GPU not attached")?;
                let context = crate::node_preview::GpuContext {
                    device: graphics.renderer.device.clone(),
                    queue: graphics.renderer.queue.clone(),
                    format: graphics.config.format,
                };
                if let Some(canonical)=&canonical{crate::node_preview::NodePreview::build(&context,&snapshot,canonical).map_err(|e|e.to_string())?;}
                crate::node_preview::NodePreview::build(&context, &snapshot, &compiled)
                    .map_err(|error| format!("node graph {} {:?}: {error}", snapshot.document.id, snapshot.preview_stamp()))
            })?;
            if snapshot.layer_target.is_some(){motion_host::session::with_session(id,|s|crate::node_layer::apply(s,&snapshot,canonical.as_ref().unwrap_or(&compiled)))?;}
            node_preview.active = Some(preview);
            node_preview.installs += 1;
            Ok(true)
        })()),
        Command::NodeCapture{seconds,path,reply}=>respond(reply,motion_host::session::with_session(id,|s|{
            let g=s.graphics.as_ref().ok_or("node GPU not attached")?;
            let preview=node_preview.active.as_mut().ok_or("no valid node preview")?;
            let dimension=crate::node_preview::RASTER_SIZE;
            let target=g.renderer.capture_target(dimension,dimension).map_err(|e|e.to_string())?;
            let mut encoder=g.renderer.device.create_command_encoder(&Default::default());clear_target(&mut encoder,&target.view);
            preview.encode(&g.renderer.device,&g.renderer.queue,&mut encoder,&target.view,crate::node_preview::Viewport{rect:[0.,0.,dimension as f32,dimension as f32],seconds},wgpu::TextureFormat::Rgba8UnormSrgb);
            g.renderer.queue.submit(Some(encoder.finish()));
            let pixels=g.renderer.read_target(&target).map_err(|e|e.to_string())?;
            let fingerprint=pixels.iter().fold(0xcbf29ce484222325u64,|h,b|(h^u64::from(*b)).wrapping_mul(0x100000001b3));
            image::save_buffer(&path,&pixels,dimension,dimension,image::ColorType::Rgba8).map_err(|e|e.to_string())?;
            Ok(serde_json::json!({"path":path,"seconds":seconds,"fingerprint":format!("{fingerprint:016x}"),"stamp":preview.stamp,"renders":preview.renders,"frames":preview.frames}))
        })),
        Command::EditChecked {text,revision,reply}=>respond(reply,motion_host::session::with_session(id, |s| {
            if s.engine.gesture_active() { return Err("finish the active editor gesture before an agent edit".into()); }
            if s.engine.revision() != revision { return Err("project revision changed; fetch motion_state before editing".into()); }
            s.apply_commands(&text)
        })),
        Command::CaptureUi { ui, path, reply } => respond(reply,motion_host::session::with_session(id, |s| {
            let g = s.graphics.as_ref().ok_or("desktop surface is not attached")?;
            let (width,height)=if ui.window==0{(g.config.width,g.config.height)}else{let window=windows.get(&ui.window).ok_or("floating screenshot window closed")?;(window.config.width,window.config.height)};
            let target = g.renderer.capture_target(width, height).map_err(|e| e.to_string())?;
            let mut chrome = motion_ui::Painter::new(&g.renderer.device, wgpu::TextureFormat::Rgba8UnormSrgb);
            let upload = ui.atlas.ok_or("screenshot requires the complete glyph atlas")?;
            chrome.upload_atlas(&g.renderer.device, &g.renderer.queue, &upload.pixels, upload.width, upload.height);
            let presenter = motion_render::Presenter::new(&g.renderer, &g.scratch.view, wgpu::TextureFormat::Rgba8UnormSrgb);
            let mut encoder = g.renderer.device.create_command_encoder(&Default::default());
            if let Some([x,y,w,h])=ui.composition {
                let scale = (w / s.scene.width as f32).min(h / s.scene.height as f32);
                let vw = s.scene.width as f32 * scale; let vh = s.scene.height as f32 * scale;
                presenter.encode_view(&g.renderer.queue, &mut encoder, &target.view, None,
                    if ui.composition_image.is_some() { [x,y,w,h] } else { [x+(w-vw)*0.5,y+(h-vh)*0.5,vw,vh] }, ui.composition_image, s.scene.background);
            } else {clear_target(&mut encoder,&target.view);}
            chrome.render_scaled(&g.renderer.device,&g.renderer.queue,&mut encoder,&target.view,&ui.paint,ui.logical_size,[width as f32/ui.logical_size[0],height as f32/ui.logical_size[1]]).map_err(|e|format!("capture_ui window={}: {e}",ui.window))?;
            if let (Some(preview),Some(viewport))=(&mut node_preview.active,ui.node_preview) {preview.encode(&g.renderer.device,&g.renderer.queue,&mut encoder,&target.view,viewport,wgpu::TextureFormat::Rgba8UnormSrgb);}
            g.renderer.queue.submit(Some(encoder.finish()));
            let pixels = g.renderer.read_target(&target).map_err(|e| e.to_string())?;
            image::save_buffer(&path, &pixels, width, height, image::ColorType::Rgba8).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({"path":path,"width":width,"height":height}))
        })),
        Command::Attach { target, reply } => respond(reply,(|| {
            motion_host::session::with_session(id, |s| {
                let next = motion_ui::Painter::new(&target.renderer.device, target.config.format);
                s.attach(target)?;
                *painter = Some(next);
                Ok(s.snapshot())
            })
        })()),
        Command::FloatWindow {key,target,reply} => respond(reply,motion_host::session::with_session(id,|s|{
            if key==0 || windows.contains_key(&key){return Err("invalid floating window id".into());}
            windows.insert(key,target);Ok(s.snapshot())
        })),
        Command::CloseWindow {key,reply} => {windows.remove(&key);respond(reply,project::state(id));},
        Command::Resize { window, width, height, reply } => respond(reply,motion_host::session::with_session(id, |s| {
            if width == 0 || height == 0 {
                log::debug!("surface resize deferred width={width} height={height} reason=zero_extent");
                return Ok(s.snapshot());
            }
            if let Some(g) = &mut s.graphics {
                if window==0 {g.config.width=width;g.config.height=height;g.surface.configure(&g.renderer.device,&g.config);}
                else if let Some(w)=windows.get_mut(&window){w.config.width=width;w.config.height=height;w.surface.configure(&g.renderer.device,&w.config);}
                else{return Err("floating window is closed".into());}
                s.view_revision += 1;
            }
            Ok(s.snapshot())
        })),
        Command::Present { ui, reply } => {let result=motion_host::session::with_session(id, |s| {
            let began = std::time::Instant::now();
            let painter=painter.as_mut().ok_or("desktop surface is not attached")?;
            let floating=if ui.window==0 {None}else{Some(windows.get_mut(&ui.window).ok_or("floating window is closed")?)};
            let mut floating=floating;
            if let (Some(w),Some(g))=(floating.as_mut(),s.graphics.as_mut()) {
                std::mem::swap(&mut w.surface,&mut g.surface);std::mem::swap(&mut w.config,&mut g.config);
            }
            let result=(||{
                let g=s.graphics.as_ref().ok_or("desktop surface is not attached")?;
                if let Some(upload)=&ui.atlas {painter.upload_atlas(&g.renderer.device,&g.renderer.queue,&upload.pixels,upload.width,upload.height);}
                if let Some(viewport)=ui.composition {
                    let frame=s.frame; // Shared clock, never a stale UI frame.
                    let stamp=CompositionStamp::new(s,&ui,viewport);
                    let reuse=presentation.retained.as_ref()==Some(&stamp)
                        && !s.preview_memory_held && s.last_error.is_none();
                    let pixel_ratio=[g.config.width as f32/ui.logical_size[0],g.config.height as f32/ui.logical_size[1]];
                    // The overlay targets the swapchain, not the composition scratch texture.
                    let surface_format=g.config.format;
                    let mut paint_error=None;
                    let overlay=|r:&motion_render::Renderer,encoder:&mut wgpu::CommandEncoder,target:&wgpu::TextureView|{
                        if let Err(error)=painter.render_scaled(&r.device,&r.queue,encoder,target,&ui.paint,ui.logical_size,pixel_ratio) {
                            paint_error=Some(error);
                            return;
                        }
                        if let (Some(preview),Some(viewport))=(&mut node_preview.active,ui.node_preview){preview.encode(&r.device,&r.queue,encoder,target,viewport,surface_format);}
                    };
                    let result=if reuse {
                        presentation.composition_reuses+=1;
                        s.present_retained_with_view(viewport,ui.composition_image,overlay)
                    }else {
                        presentation.composition_renders+=1;
                        s.render_with_view(frame,Some(viewport),ui.composition_image,overlay)
                    };
                    let result=ui_paint::finish(result,paint_error,ui.window);
                    presentation.retained=if result.as_ref().is_ok_and(|v|*v)
                        && !s.preview_memory_held && s.last_error.is_none()
                        && stamp==CompositionStamp::new(s,&ui,viewport)
                    {Some(stamp)}else{None};
                    result
                }else{
                    let output=match g.surface.get_current_texture(){
                        wgpu::CurrentSurfaceTexture::Success(output)|wgpu::CurrentSurfaceTexture::Suboptimal(output)=>{PRESENT_SURFACE_FAILURES.success();output},
                        error @ (wgpu::CurrentSurfaceTexture::Lost|wgpu::CurrentSurfaceTexture::Outdated)=>{
                            PRESENT_SURFACE_FAILURES.failure("desktop.present.surface",format_args!("state={error:?} window={}",ui.window));
                            g.surface.configure(&g.renderer.device,&g.config);return Ok(false)
                        },
                        wgpu::CurrentSurfaceTexture::Timeout=>{
                            PRESENT_SURFACE_FAILURES.failure("desktop.present.surface",format_args!("state=Timeout window={}",ui.window));
                            return Ok(false)
                        },wgpu::CurrentSurfaceTexture::Occluded=>{PRESENT_SURFACE_FAILURES.success();return Ok(false)},
                        wgpu::CurrentSurfaceTexture::Validation=>return Err("surface acquisition validation failed".into()),
                    };
                    let view=output.texture.create_view(&Default::default());
                    let mut encoder=g.renderer.device.create_command_encoder(&Default::default());
                    clear_target(&mut encoder,&view);
                    painter.render_scaled(&g.renderer.device,&g.renderer.queue,&mut encoder,&view,&ui.paint,ui.logical_size,[g.config.width as f32/ui.logical_size[0],g.config.height as f32/ui.logical_size[1]]).map_err(|e|format!("present window={}: {e}",ui.window))?;
                    if let (Some(preview),Some(viewport))=(&mut node_preview.active,ui.node_preview){preview.encode(&g.renderer.device,&g.renderer.queue,&mut encoder,&view,viewport,g.config.format);}
                    g.renderer.queue.submit(Some(encoder.finish()));g.renderer.queue.present(output);Ok(true)
                }
            })();
            if let (Some(w),Some(g))=(floating.as_mut(),s.graphics.as_mut()) {
                std::mem::swap(&mut w.surface,&mut g.surface);std::mem::swap(&mut w.config,&mut g.config);
            }
            presentation.last_present_us=began.elapsed().as_micros() as u64;
            #[cfg(debug_assertions)]
            if presentation.last_present_us>=30_000 {
                log::debug!("[motion-timing] surface {} present: {} us, composition renders={}, reuses={}",ui.window,presentation.last_present_us,presentation.composition_renders,presentation.composition_reuses);
            }
            result.map_err(|error| format!("{error}; window={} logical_size={:?}", ui.window, ui.logical_size))
        });
        let mut paint=ui.paint;paint.clear();
        recycle_paint_pool(paints, paint, true);
        respond(reply,result);
        },
        Command::OpenDirectory { directory, reply } => respond(reply,(||{
            motion_core::storage::load(&directory).map_err(|e|e.to_string())?;
            let directory=directory.canonicalize().map_err(|e|e.to_string())?;
            if directory==guard.root{return project::save(id);}
            let next=ProjectGuard::acquire(&directory)?;
            let result=project::open_directory(id,&directory)?;*guard=next;Ok(result)
        })()),
        Command::State(reply) => respond(reply,project::state(id).map(|mut state|{
            state["capabilities"]["layer_sampling"]["desktop_png"]=serde_json::json!({"exposure_renderer":true});
            state["capabilities"]["layer_sampling"]["desktop_movie"]=serde_json::json!({
                "exposure_renderer":cfg!(feature="ffmpeg"),"codec_check":"export_start","requires":["h264","aac"]});
            state["desktop"]=serde_json::json!({"headless":painter.is_none(),"panelWindows":if painter.is_some(){windows.len()+1}else{0},"sharedGpuDevice":painter.is_some(),"agentToolProtocol":1,
                "uiPerformance":{"compositionRenders":presentation.composition_renders,"compositionReuses":presentation.composition_reuses,"lastPresentUs":presentation.last_present_us},
                "diagnostics":diagnostics.status(),"frameExport":frame_capture.status(),
                "nodePreview":{"active":node_preview.active.as_ref().map(|p|p.stamp),"frames":node_preview.active.as_ref().map_or(0,|p|p.frames),"shaderRenders":node_preview.active.as_ref().map_or(0,|p|p.renders),"installs":node_preview.installs,"logicalSize":crate::node_preview::LOGICAL_SIZE,"rasterSize":crate::node_preview::RASTER_SIZE,"perFrameCpuReadbacks":0}});state
        })),
        Command::Save(reply) => respond(reply,project::save(id)),
        Command::NewProject { text, reply } => respond(reply,guard.changed(project::new_project_with_choice(id, &text, project::PreviousProject::Save))),
        Command::OpenProject { name, reply } => respond(reply,(||{
            let directory=guard.root.parent().ok_or("project parent is missing")?.join(&name).canonicalize().map_err(|e|e.to_string())?;
            if directory==guard.root{return project::save(id);}
            let next=ProjectGuard::acquire(&directory)?;
            let result=project::open_project(id,&name)?;*guard=next;Ok(result)
        })()),
        Command::ImportProject { file, reply } => respond(reply,guard.changed(project::import_project(id, &file))),
        Command::ImportImage {file,reply} => respond(reply,motion_host::session::with_session(id,|s|crate::workflow::import_image(s,&file))),
        Command::CreateText {text,size,width,font,reply}=>respond(reply,motion_host::session::with_session(id,|s|crate::workflow::create_text(s,text,size,width,font))),
        Command::FreezeMovie {control,reply}=> {
            let plan=motion_host::session::with_session(id,|s|motion_desktop_media::movie::FrozenMovie::capture_plan(s,&control));
            match plan {
                Err(error)=>respond(reply,Err(error)),
                Ok(plan) => {
                    if let Err(error) = spawn_movie_freeze(reply.clone(), move || plan.freeze(&control)) {
                        respond(reply, Err(error));
                    }
                }
            }
        },
        Command::RenderPreview {frame,reply}=>respond(reply,(||{if let Some(frame)=frame {preview::seek(id,frame)?;}export::capture(id)})()),
        Command::Edit { text, reply } => respond(reply,editing::command(id, &text)),
        Command::AlignLayers { objects, edge, target, reply } => respond(reply,motion_host::ops::alignment::align(id,objects,edge,target)),
        Command::DistributeLayers { objects, distribution, reply } => respond(reply,motion_host::ops::alignment::distribute(id,objects,distribution)),
        Command::History { op, reply } => respond(reply,editing::history(id, op)),
        Command::Drag {
            object,
            dx,
            dy,
            width,
            height,
            reply,
        } => respond(reply,editing::drag(id, object, dx, dy, width, height)),
        Command::Seek { frame, reply } => respond(reply,preview::seek(id, frame)),
        Command::View { kind, reply } => respond(reply,preview::view(id, kind)),
        Command::Observe {
            enabled,
            azimuth,
            elevation,
            reply,
        } => respond(reply,preview::observe(id, enabled, azimuth, elevation)),
        Command::Navigate {
            dx,
            dy,
            zoom,
            multi,
            width,
            height,
            reply,
        } => respond(reply,preview::navigate(id, dx, dy, zoom, multi, width, height)),
        Command::Render { frame, reply } => respond(reply,preview::render(id, frame)),
        Command::ConfigureMemory {
            total,
            guarded,
            reply,
        } => respond(reply,preview::configure_memory(id, total, guarded)),
        Command::PreviewMode {
            mode,
            thermal,
            reply,
        } => respond(reply,preview::preview_mode(id, mode, thermal)),
        Command::PreviewInfo(reply) => respond(reply,preview::preview_info(id)),
        Command::CurveGraph { text, reply } => respond(reply,editing::curve_graph(&text)),
        Command::HitCandidates { x, y, reply } => respond(reply,geometry::hit_candidates(id, x, y)),
        Command::Plugin { request, reply } => respond(reply,effects::plugin(id, &request)),
        Command::ColorCurveGraph { text, reply } => respond(reply,effects::color_curve_graph(&text)),
        Command::Composition { text, reply } => respond(reply,composition::request(id, &text)),
        Command::Media { text, reply } => respond(reply,media_request(id, &text)),
        Command::MediaPackageLimits(reply) => respond(reply,Ok(media::package_limits())),
        Command::Capture { output, control, reply } => {
            if let Err(error) = frame_capture.start(id, output, control, reply.clone()) {
                frame_capture::respond_frame(reply, Err(error));
            }
        },
        Command::Pack(reply) => respond(reply,export::pack(id)),
        Command::ImageInfo { path, reply } => respond(reply,images::image_info(&path)),
        Command::PrepareImage { root, path, reply } => respond(reply,images::prepare_image(root, path)),
        Command::MediaCapabilities { text, reply } => {
            respond(reply,capabilities_request(platform, &text))
        }
        Command::ResourceInfo { directory, reply } => {
            respond(reply,motion_host::session::resource_info(&directory))
        }
        Command::VideoFrame {
            object,
            sequence,
            reply,
        } => respond(reply,media::read_video_frame_into(id, object, sequence).map(|frame| ArcFrame {
            report: media::frame_report(&frame, object, sequence),
            frame,
        })),
        Command::Shutdown => return true,
    };
    false
}

/// Desktop media requests carry real paths, so the shared opener applies.
fn media_request(id: i64, text: &str) -> Result<Value, String> {
    let value = motion_host::ops::parse_request(text, 16 * 1024, "media request")?;
    let opener = value["path"]
        .as_str()
        .map(|path| media::file_opener(PathBuf::from(path)));
    media::request(id, value, opener)
}

/// Capability queries need no session, so they answer from the platform alone.
fn capabilities_request(platform: &Arc<dyn Platform>, text: &str) -> Result<Value, String> {
    let value = motion_host::ops::parse_request(text, 16 * 1024, "media request")?;
    let request = serde_json::from_value::<media::Request>(value)
        .map_err(|error| format!("media request: {error}"))?;
    let media::Request::MediaCapabilities { video_query: query } = request else {
        return Err("expected media_capabilities request".into());
    };
    platform.media_capabilities(query.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capability_queries_reject_bad_requests_instead_of_answering_defaults() {
        let platform: Arc<dyn Platform> = motion_desktop_media::platform();
        for text in [
            "{",
            r#"{"op":"unknown"}"#,
            r#"{"op":"media_status","request_id":"wrong-command"}"#,
            r#"{"op":"media_capabilities","video_query":{"width":"not-a-number"}}"#,
            r#"{"op":"media_capabilities","unexpected":1}"#,
        ] {
            assert!(capabilities_request(&platform, text).is_err(), "accepted {text}");
        }
        let oversized = format!("{}{}", " ".repeat(16 * 1024), r#"{"op":"media_capabilities"}"#);
        assert!(capabilities_request(&platform, &oversized).is_err());
        assert!(capabilities_request(&platform, r#"{"op":"media_capabilities"}"#).unwrap().is_object());
    }

    #[test]
    fn asynchronous_edit_replies_preserve_one_transaction_and_undo() {
        let folder = tempfile::tempdir().unwrap();
        let engine = Engine::start(
            folder.path().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine.state().unwrap()["project"].clone();
        let begin = engine.history_async(2).unwrap();
        let edit = engine
            .edit_async(
                serde_json::to_string(&motion_core::Command::Add {
                    layer: motion_core::Layer::solid(
                        1,
                        "Async layer",
                        [100.; 2],
                        [100., 100., 0.],
                        [1.; 4],
                    ),
                })
                .unwrap(),
            )
            .unwrap();
        let end = engine.history_async(3).unwrap();
        for reply in [begin, edit, end] {
            reply
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap();
        }
        assert_ne!(engine.state().unwrap()["project"], before);
        assert_eq!(
            engine
                .history_async(0)
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap()["project"],
            before
        );
        assert_eq!(
            engine
                .seek_async(21.5)
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap()["frame"],
            21.5
        );
    }
    #[test]
    fn project_lock_releases_on_worker_shutdown() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("default");
        let engine =
            Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        assert!(Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).is_err());
        engine.save().unwrap();
        drop(engine);
        let reopened = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
        assert!(reopened.state().is_ok());
    }
}
