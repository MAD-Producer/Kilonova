//! Native desktop editor. One worker owns the session, GPU and swapchain.
mod audio_output;
mod dialogs;
mod engine;
mod i18n;
mod mcp;
mod movie_export;
mod node_editor;
mod node_examples;
mod node_preview;
mod node_tasks;
mod panels;
mod shell;
mod shortcuts;
mod workflow;
mod workspace;
use i18n::{Locale, Text as T};

use engine::{AtlasUpload, Engine, UiFrame};
use motion_core::{Command, Layer, Project};
use motion_ui::{
    dock,
    input::{Event, Input, Key, Modifiers, MouseButton},
    paint::PaintList,
    text::TextAtlas,
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key as WinitKey, NamedKey},
    window::{Window, WindowId},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--command-list") {
        println!("{}", motion_core::CommandRegistry::catalogue());
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Motion Studio\n  --project <directory>  open an existing project or create one\n  --command-list         print the registered command catalogue\n  --commands <file.json> run/save an atomic command batch without a window\n  --mcp                  serve editor tools over MCP stdio (no window)\n  --mcp-ui               serve MCP and open the shared editor window\n  --locale <zh|en>        UI language override\n  --smoke <report.json>  hidden-window GPU acceptance with screenshot\n  --node-smoke <report.json>  node graph GPU/UI acceptance with screenshots");
        return Ok(());
    }
    let argument = |flag: &str| -> Result<Option<PathBuf>, String> {
        if let Some(i) = args.iter().position(|a| a == flag) {
            args.get(i + 1)
                .filter(|a| !a.starts_with("--"))
                .map(|a| Some(PathBuf::from(a)))
                .ok_or_else(|| format!("{flag} requires a path"))
        } else {
            Ok(None)
        }
    };
    let smoke = argument("--smoke")?
        .or(argument("--node-smoke")?)
        .or(argument("--panels-smoke")?);
    let locale = argument("--locale")?
        .map(|p| Locale::parse(&p.to_string_lossy()).ok_or("locale must be zh or en"))
        .transpose()?;
    let root = argument("--project")?
        .or_else(|| std::env::var_os("MOTION_PROJECT").map(PathBuf::from))
        .unwrap_or_else(|| {
            if smoke.is_some() {
                std::env::temp_dir()
                    .join(format!("motion-desktop-smoke-{}", std::process::id()))
                    .join("default")
            } else {
                directories().join("default")
            }
        });
    if args.iter().any(|a| a == "--mcp") {
        if args.iter().any(|a| a == "--commands") {
            return Err("--commands and --mcp are mutually exclusive".into());
        }
        return mcp::serve(
            Arc::new(Engine::start(
                root,
                motion_desktop_media::platform(),
                total_memory(),
            )?),
            None,
        )
        .map_err(Into::into);
    }
    if let Some(file) = argument("--commands")? {
        use std::io::Read;
        let mut bytes = vec![];
        std::fs::File::open(&file)?
            .take(engine::REQUEST_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > engine::REQUEST_LIMIT {
            return Err("command file exceeds 256 KiB".into());
        }
        let payload = String::from_utf8(bytes)?;
        let engine = Engine::start(root, motion_desktop_media::platform(), total_memory())?;
        let revision = engine.state()?["revision"]
            .as_u64()
            .ok_or("revision missing")?;
        let result = engine.edit_at_revision(payload, revision)?;
        engine.save()?;
        println!("{}", serde_json::json!({"ok":true,"data":result}));
        return Ok(());
    }
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let mut app = Shell::new(root, smoke, locale)?;
    app.smoke_panels = args.iter().any(|a| a == "--panels-smoke");
    app.smoke_nodes = args
        .iter()
        .any(|a| a == "--node-smoke" || a == "--smoke-nodes");
    if args.iter().any(|a| a == "--mcp-ui") {
        let engine = app.engine.clone();
        std::thread::Builder::new()
            .name("motion-mcp".into())
            .spawn(move || {
                if let Err(e) = mcp::serve(
                    engine,
                    Some(Box::new(move || {
                        let _ = proxy.send_event(UserEvent::EngineChanged);
                    })),
                ) {
                    eprintln!("MCP: {e}");
                }
            })?;
    }
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.fatal {
        return Err(error.into());
    }
    Ok(())
}
fn directories() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(base).join("MotionStudio");
    }
    #[cfg(target_os = "macos")]
    if let Some(base) = std::env::var_os("HOME") {
        return PathBuf::from(base).join("Library/Application Support/MotionStudio");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(base).join("motion-studio");
        }
        if let Some(base) = std::env::var_os("HOME") {
            return PathBuf::from(base).join(".local/share/motion-studio");
        }
    }
    std::env::temp_dir().join("motion-studio")
}
fn total_memory() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
                .map(|v| v * 1024)
        })
        .unwrap_or(8 * 1024 * 1024 * 1024)
}
enum UserEvent {
    EngineChanged,
}
struct Shell {
    window: Option<Arc<Window>>,
    engine: Arc<Engine>,
    atlas: TextAtlas,
    workspace: workspace::Workspace,
    layout_path: PathBuf,
    floating: std::collections::HashMap<WindowId, FloatWindow>,
    next_window: u64,
    panels: panels::Panels,
    node_tasks: node_tasks::NodeTasks,
    node_compile_due: Option<Instant>,
    node_installing: Vec<(
        node_editor::EditorSnapshot,
        std::sync::mpsc::Receiver<Result<bool, String>>,
    )>,
    node_close_prompt: Option<node_editor::EditorSnapshot>,
    node_close_save: Option<node_editor::EditorSnapshot>,
    node_examples: Option<node_examples::Browser>,
    node_example_after_save: Option<(node_editor::EditorSnapshot, &'static str)>,
    dialog: Option<dialogs::Dialog>,
    system_fonts: Option<Vec<dialogs::FontChoice>>,
    input: Input,
    state: Value,
    scale: f32,
    dirty: bool,
    pending: bool,
    frame: f64,
    playback: Option<(Instant, f64)>,
    playback_ready: bool,
    audio: Option<audio_output::AudioOutput>,
    freezing_movie: Option<(
        std::sync::mpsc::Receiver<Result<motion_desktop_media::movie::FrozenMovie, String>>,
        PathBuf,
        motion_desktop_media::movie::ExportControl,
    )>,
    exporting_movie: Option<movie_export::MovieExport>,
    export_notified: bool,
    importing_image: Option<std::sync::mpsc::Receiver<Result<Value, String>>>,
    importing_media: Option<(String, Instant)>,
    attaching: Option<std::sync::mpsc::Receiver<Result<Value, String>>>,
    attached: bool,
    rendering: std::collections::HashMap<u64, std::sync::mpsc::Receiver<Result<bool, String>>>,
    resizing: Option<std::sync::mpsc::Receiver<Result<Value, String>>>,
    resize_queue: std::collections::HashMap<u64, [u32; 2]>,
    actions: std::collections::VecDeque<panels::Action>,
    editing: Option<(Instant, std::sync::mpsc::Receiver<Result<Value, String>>)>,
    closing: bool,
    next_redraw: Instant,
    fatal: Option<String>,
    smoke: Option<PathBuf>,
    smoke_frames: u32,
    smoke_stage: u8,
    smoke_nodes: bool,
    smoke_panels: bool,
    smoke_panel_index: usize,
    smoke_panel_current: Option<dock::Panel>,
    smoke_panel_records: Vec<Value>,
    smoke_node_report: Value,
    smoke_float_frames: std::collections::HashMap<u32, u32>,
    started: Instant,
}
struct FloatWindow {
    window: Arc<Window>,
    key: u64,
    panel: dock::Panel,
    input: Input,
    scale: f32,
}

impl Shell {
    fn new(root: PathBuf, smoke: Option<PathBuf>, locale: Option<Locale>) -> Result<Self, String> {
        let engine = Arc::new(Engine::start(
            root,
            motion_desktop_media::platform(),
            total_memory(),
        )?);
        let state = engine.state()?;
        let layout_path = std::env::var_os("MOTION_UI_LAYOUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| directories().join("desktop-layout-v2.json"));
        let mut workspace = workspace::Workspace::load(&layout_path);
        if let Some(locale) = locale {
            workspace.locale = locale;
        }
        let chosen = workspace.locale;
        let mut shell = Self {
            window: None,
            engine,
            atlas: TextAtlas::from_family(
                motion_ui::style::default_stylesheet().font_family(),
                motion_ui::style::default_stylesheet().base_font_size(),
                chosen == Locale::Zh,
            )?,
            workspace,
            layout_path,
            floating: Default::default(),
            next_window: 1,
            panels: Default::default(),
            node_tasks: node_tasks::NodeTasks::start()?,
            node_compile_due: Some(Instant::now()),
            node_installing: vec![],
            node_close_prompt: None,
            node_close_save: None,
            node_examples: None,
            node_example_after_save: None,
            dialog: None,
            system_fonts: None,
            input: Default::default(),
            state,
            scale: 1.0,
            dirty: true,
            pending: false,
            frame: 0.0,
            playback: None,
            playback_ready: false,
            audio: None,
            freezing_movie: None,
            exporting_movie: None,
            export_notified: false,
            importing_image: None,
            importing_media: None,
            attaching: None,
            attached: false,
            rendering: Default::default(),
            resizing: None,
            resize_queue: Default::default(),
            actions: Default::default(),
            editing: None,
            closing: false,
            next_redraw: Instant::now(),
            fatal: None,
            smoke,
            smoke_frames: 0,
            smoke_stage: 0,
            smoke_nodes: false,
            smoke_panels: false,
            smoke_panel_index: 0,
            smoke_panel_current: None,
            smoke_panel_records: vec![],
            smoke_node_report: Value::Null,
            smoke_float_frames: Default::default(),
            started: Instant::now(),
        };
        shell.panels.locale = chosen;
        shell.panels.effect_controls.catalogue =
            shell.engine.plugin("{\"op\":\"catalogue\"}".into())?;
        if shell.smoke.is_some() {
            shell.workspace = workspace::Workspace::default();
            shell.workspace.locale = chosen;
            shell.prepare_smoke()?;
        }
        Ok(shell)
    }
}
impl ApplicationHandler<UserEvent> for Shell {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: UserEvent) {
        self.actions.push_back(panels::Action::RefreshState);
        self.dirty = true;
        self.redraw();
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Motion Studio")
            .with_inner_size(winit::dpi::LogicalSize::new(1600.0, 976.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(900.0, 600.0))
            .with_visible(self.smoke.is_none());
        let result = (|| {
            let window = Arc::new(
                event_loop
                    .create_window(attributes)
                    .map_err(|e| e.to_string())?,
            );
            self.scale = window.scale_factor() as f32;
            window.set_ime_allowed(true);
            let size = window.inner_size();
            self.attaching = Some(self.engine.attach_async(
                window.clone(),
                size.width,
                size.height,
            )?);
            self.window = Some(window);
            self.dirty = true;
            self.redraw();
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            self.fatal = Some(error);
            event_loop.exit();
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.floating.contains_key(&id) {
            self.floating_event(event_loop, id, event);
            return;
        }
        match event {
            WindowEvent::Ime(ime) => {
                if let Some(event) = translate_ime(ime) {
                    self.translate(event);
                }
            }
            WindowEvent::CloseRequested => {
                self.request_close();
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.resize_queue.insert(0, [size.width, size.height]);
                    self.dirty = true;
                    self.redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor as f32;
                self.dirty = true;
                self.redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let m = modifiers.state();
                self.input.modifiers = Modifiers {
                    control: m.control_key() || m.super_key(),
                    shift: m.shift_key(),
                    alt: m.alt_key(),
                };
            }
            WindowEvent::CursorMoved { position, .. } => self.translate(Event::MouseMoved {
                position: [
                    position.x as f32 / self.scale,
                    position.y as f32 / self.scale,
                ],
            }),
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    WinitMouseButton::Left => MouseButton::Left,
                    WinitMouseButton::Right => MouseButton::Right,
                    _ => MouseButton::Middle,
                };
                let position = self.input.mouse;
                self.translate(if state == ElementState::Pressed {
                    Event::MousePressed { position, button }
                } else {
                    Event::MouseReleased { position, button }
                });
            }
            WindowEvent::MouseWheel { delta, .. } => self.translate(Event::MouseWheel {
                delta: mouse_wheel_delta(delta, self.scale),
            }),
            WindowEvent::KeyboardInput { event, .. } => {
                for event in keyboard_events(&event, &self.input) {
                    self.translate(event);
                }
            }
            WindowEvent::Focused(false) => {
                self.workspace.cancel();
                self.stop();
                self.actions
                    .extend(panels::cancel_interaction(&mut self.panels));
                self.translate(Event::FocusLost);
            }
            WindowEvent::DroppedFile(path) => {
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                self.actions.push_back(
                    if path.is_dir()
                        || path.file_name().is_some_and(|n| n == "project.json")
                        || matches!(ext.as_str(), "aem" | "msproj")
                    {
                        panels::Action::OpenPath(path)
                    } else {
                        panels::Action::ImportPath(path)
                    },
                );
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.present(event_loop) {
                    self.pending = false;
                    self.stop();
                    self.panels.message = Some(self.workspace.locale.error(&error));
                    if self.smoke.is_some() {
                        self.fatal = Some(error);
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some((began, receive)) = &self.editing {
            let result = match receive.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("engine thread has stopped".into()))
                }
            };
            if let Some(result) = result {
                if began.elapsed().as_millis() >= 16 {
                    eprintln!(
                        "[motion-timing] asynchronous edit: {} us",
                        began.elapsed().as_micros()
                    );
                }
                self.editing = None;
                match result {
                    Ok(state) => self.state = state,
                    Err(error) => {
                        self.panels.message = Some(self.workspace.locale.error(&error));
                        self.stop();
                    }
                }
                self.dirty = true;
                self.redraw();
            }
        }
        if let Err(error) = self.poll_nodes() {
            self.panels.node_editor.report_error(error);
            self.dirty = true;
        }
        if let Some((receive, path, control)) = &self.freezing_movie {
            if let Ok(result) = receive.try_recv() {
                let output = path.clone();
                let control = control.clone();
                self.freezing_movie = None;
                match result.and_then(|snapshot| {
                    movie_export::MovieExport::start_with_control(snapshot, output, control)
                }) {
                    Ok(task) => self.exporting_movie = Some(task),
                    Err(error) => self.panels.message = Some(self.workspace.locale.error(&error)),
                };
                self.dirty = true;
            } else {
                self.panels.message = Some(format!(
                    "{} {:.0}%",
                    self.workspace.locale.t(T::Exporting),
                    control.progress() * 100.
                ));
                self.dirty = true;
            }
        }
        if let Some(export) = &self.exporting_movie {
            let status = export.status();
            if status.state == "running" {
                self.panels.message = Some(format!(
                    "{} {:.0}%",
                    self.workspace.locale.t(T::Exporting),
                    status.progress * 100.
                ));
                self.dirty = true;
            } else if !self.export_notified {
                self.export_notified = true;
                self.panels.message = Some(if let Some(result) = status.result {
                    format!(
                        "{}: {}",
                        self.workspace.locale.t(T::ExportFinished),
                        result.path.display()
                    )
                } else {
                    self.workspace
                        .locale
                        .error(status.error.as_deref().unwrap_or("export cancelled"))
                });
                self.dirty = true;
            }
        }
        if let Err(error) = self.poll_import() {
            self.panels.message = Some(self.workspace.locale.error(&error));
            self.dirty = true;
        }
        if self.smoke.is_some()
            && self.started.elapsed()
                > Duration::from_secs(if self.smoke_panels { 120 } else { 30 })
        {
            self.fatal = Some("desktop smoke timed out".into());
            event_loop.exit();
            return;
        }
        if let Some(receive) = &self.attaching {
            if let Ok(result) = receive.try_recv() {
                self.attaching = None;
                match result {
                    Ok(state) => {
                        self.state = state;
                        self.attached = true;
                        self.dirty = true;
                        let restore = self.workspace.floating.clone();
                        self.workspace.floating.clear();
                        self.workspace.hidden.clear();
                        for panel in restore {
                            self.workspace.redock(panel);
                            if let Err(e) = self.float_panel(panel, event_loop) {
                                self.panels.message = Some(self.workspace.locale.error(&e));
                            }
                        }
                    }
                    Err(error) => {
                        self.fatal = Some(error);
                        event_loop.exit();
                        return;
                    }
                }
            }
        }
        let mut completed = vec![];
        for (key, receive) in &self.rendering {
            if let Ok(result) = receive.try_recv() {
                completed.push((*key, result));
            }
        }
        for (key, result) in completed {
            self.rendering.remove(&key);
            match result {
                Ok(rendered) => {
                    self.pending = !rendered;
                    if rendered && self.smoke.is_some() {
                        if key == 0 {
                            self.smoke_frames += 1;
                            self.pending = true;
                        } else if let Some(f) = self.floating.values().find(|f| f.key == key) {
                            *self.smoke_float_frames.entry(f.panel.dock().0).or_default() += 1;
                        }
                    }
                }
                Err(e) => {
                    self.stop();
                    self.panels.message = Some(self.workspace.locale.error(&e));
                    if self.smoke.is_some() {
                        self.fatal = Some(e);
                        event_loop.exit();
                        return;
                    }
                }
            }
        }
        if let Some(receive) = &self.resizing {
            if let Ok(result) = receive.try_recv() {
                self.resizing = None;
                match result {
                    Ok(state) => self.state = state,
                    Err(e) => self.panels.message = Some(e),
                }
                self.dirty = true;
            }
        }
        if self.attached
            && self.rendering.is_empty()
            && self.resizing.is_none()
            && self.editing.is_none()
        {
            while let Some(action) = shell::actions::pop_action(&mut self.actions) {
                if let Err(e) = self.action(action, Some(event_loop)) {
                    self.panels.message = Some(self.workspace.locale.error(&e));
                    self.stop();
                }
                self.redraw();
                if self.editing.is_some() {
                    break;
                }
            }
            if let Some((&key, &size)) = self
                .resize_queue
                .iter()
                .next()
                .filter(|_| self.editing.is_none())
            {
                self.resize_queue.remove(&key);
                match self.engine.resize_async(key, size[0], size[1]) {
                    Ok(receive) => self.resizing = Some(receive),
                    Err(e) => self.panels.message = Some(e),
                }
            }
            if self.closing && self.editing.is_none() && self.actions.is_empty() {
                match self.engine.save() {
                    Ok(_) => {
                        event_loop.exit();
                        return;
                    }
                    Err(e) => {
                        self.closing = false;
                        self.panels.message = Some(self.workspace.locale.error(&e));
                        self.dirty = true;
                    }
                }
            }
            if self.smoke.is_some() && self.editing.is_none() {
                let step = (|| -> Result<(), String> {
                    if self.smoke_stage == 0 && self.smoke_frames >= 2 {
                        self.float_panel(dock::DockId(2), event_loop)?;
                        if let Some(f) = self
                            .floating
                            .values()
                            .find(|f| f.panel == dock::Panel::EffectControls)
                        {
                            let _ = f
                                .window
                                .request_inner_size(winit::dpi::LogicalSize::new(640.0, 560.0));
                        }
                        self.smoke_stage = 1;
                    } else if self.smoke_stage == 1
                        && self.smoke_float_frames.get(&2).copied().unwrap_or(0) >= 2
                    {
                        self.float_panel(dock::DockId(3), event_loop)?;
                        self.smoke_stage = 2;
                    } else if self.smoke_stage == 2
                        && self.smoke_float_frames.get(&3).copied().unwrap_or(0) >= 2
                    {
                        self.redock_panel(dock::DockId(2))?;
                        self.redock_panel(dock::DockId(3))?;
                        let locale = self.workspace.locale;
                        self.workspace = workspace::Workspace::default();
                        self.workspace.locale = locale;
                        self.smoke_stage = 3;
                        self.dirty = true;
                    } else if self.smoke_nodes {
                        self.node_smoke_step(event_loop)?;
                    } else if self.smoke_panels {
                        self.panel_smoke_step(event_loop)?;
                    } else if self.smoke_stage == 3 && self.smoke_frames >= 6 {
                        self.finish_smoke()?;
                        self.smoke_stage = 4;
                    }
                    Ok(())
                })();
                if let Err(e) = step {
                    self.fatal = Some(e);
                    event_loop.exit();
                    return;
                }
                if self.smoke_stage == 4 {
                    event_loop.exit();
                    return;
                }
            }
        }
        if self.smoke.is_some()
            && self.attached
            && self.rendering.is_empty()
            && self.editing.is_none()
            && self.resizing.is_none()
            && self.resize_queue.is_empty()
        {
            // Windows may suppress RedrawRequested for a hidden acceptance window.
            if let Err(e) = self.present(event_loop) {
                self.fatal = Some(e);
                event_loop.exit();
                return;
            }
            let floating: Vec<_> = self.floating.keys().copied().collect();
            for id in floating {
                if let Err(e) = self.present_float(id, event_loop) {
                    self.fatal = Some(e);
                    event_loop.exit();
                    return;
                }
            }
        }
        let busy = self.attaching.is_some()
            || self.editing.is_some()
            || !self.rendering.is_empty()
            || self.resizing.is_some()
            || !self.resize_queue.is_empty()
            || !self.actions.is_empty()
            || self.node_tasks.busy()
            || !self.node_installing.is_empty()
            || self.node_compile_due.is_some();
        let node_playing = self.panels.node_preview.playing
            && (panels::node_view(&self.workspace.root, self.size().unwrap_or([0., 0.])).is_some()
                || self
                    .floating
                    .values()
                    .any(|f| f.panel == dock::Panel::Nodes));
        let ui_animating = self.panels.motion.active();
        if busy
            || node_playing
            || ui_animating
            || self.pending
            || self.playback.is_some()
            || self.dirty
            || self.importing_image.is_some()
            || self.importing_media.is_some()
            || self.freezing_movie.is_some()
            || self.exporting_movie.as_ref().is_some_and(|e| !e.finished())
        {
            if self.attached
                && self.resizing.is_none()
                && self.resize_queue.is_empty()
                && (self.dirty || Instant::now() >= self.next_redraw)
            {
                self.redraw();
                let rate = if ui_animating {
                    60.
                } else {
                    self.state["project"]["fps"]
                        .as_f64()
                        .unwrap_or(30.0)
                        .min(self.state["preview"]["fps"].as_f64().unwrap_or(60.0))
                        .clamp(1.0, 120.0)
                };
                self.next_redraw = Instant::now() + Duration::from_secs_f64(1.0 / rate);
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(if busy { 4 } else { 16 }),
            ));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}
fn playback_frame(origin: f64, elapsed: Duration, fps: u32, frames: u32) -> f64 {
    (origin + elapsed.as_secs_f64() * f64::from(fps.max(1))).rem_euclid(f64::from(frames.max(1)))
}
fn mouse_wheel_delta(delta: MouseScrollDelta, scale: f32) -> [f32; 2] {
    // UI scrolling uses positive offsets towards the end of a list.
    match delta {
        MouseScrollDelta::LineDelta(x, y) => [-x * 32., -y * 32.],
        MouseScrollDelta::PixelDelta(p) => [-p.x as f32 / scale, -p.y as f32 / scale],
    }
}
fn translate_key(key: &WinitKey) -> Option<Key> {
    Some(match key {
        WinitKey::Character(s) => match s.to_ascii_lowercase().as_str() {
            "a" => Key::A,
            "f" => Key::F,
            "g" => Key::G,
            "t" => Key::T,
            "0" => Key::Num0,
            "n" => Key::N,
            "o" => Key::O,
            "i" => Key::I,
            "s" => Key::S,
            "z" => Key::Z,
            "y" => Key::Y,
            "d" => Key::D,
            "k" => Key::K,
            "p" => Key::P,
            "r" => Key::R,
            "u" => Key::U,
            "j" => Key::J,
            "m" => Key::M,
            "x" => Key::X,
            "=" | "+" => Key::Equal,
            "," => Key::Comma,
            "." => Key::Period,
            _ => return None,
        },
        WinitKey::Named(k) => match k {
            NamedKey::Escape => Key::Escape,
            NamedKey::Enter => Key::Enter,
            NamedKey::Tab => Key::Tab,
            NamedKey::Space => Key::Space,
            NamedKey::Delete => Key::Delete,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::ArrowLeft => Key::Left,
            NamedKey::ArrowRight => Key::Right,
            NamedKey::ArrowUp => Key::Up,
            NamedKey::ArrowDown => Key::Down,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::F3 => Key::F3,
            NamedKey::F9 => Key::F9,
            NamedKey::Shift => Key::Shift,
            NamedKey::Control => Key::Control,
            NamedKey::Alt => Key::Alt,
            _ => return None,
        },
        _ => return None,
    })
}

/// Main and floating windows normalize keyboard input through the same adapter.
fn keyboard_events(event: &winit::event::KeyEvent, input: &Input) -> Vec<Event> {
    let mut result = vec![];
    if let Some(key) = translate_key(&event.logical_key) {
        let modifiers = input.modifiers;
        result.push(if event.state == ElementState::Released {
            Event::KeyReleased { key, modifiers }
        } else if event.repeat {
            Event::KeyRepeated { key, modifiers }
        } else {
            Event::KeyPressed { key, modifiers }
        });
    }
    if event.state == ElementState::Pressed
        && !input.composing
        && !input.ime_consumed
        && !input.modifiers.control
        && !input.modifiers.alt
    {
        if let Some(text) = event
            .text
            .as_ref()
            .filter(|t| !t.chars().all(char::is_control))
        {
            result.push(Event::TextInput(text.to_string()));
        }
    }
    result
}

fn translate_ime(ime: winit::event::Ime) -> Option<Event> {
    use winit::event::Ime;
    match ime {
        Ime::Preedit(text, _) => Some(Event::ImeComposition(!text.is_empty())),
        Ime::Commit(text) => Some(Event::TextCommit(text)),
        Ime::Disabled => Some(Event::ImeComposition(false)),
        Ime::Enabled => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_examples_are_independent_and_reject_stale_replacement() {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::Zh)).unwrap();
        let before = shell.engine.state().unwrap();
        let initial = shell.panels.node_editor.snapshot();
        shell.open_node_example(&initial, "uv_gradient").unwrap();
        assert_eq!(shell.panels.node_editor.document().id, "uv_gradient");
        assert!(shell.panels.node_editor.snapshot().source_path.is_none());
        let snapshot = shell.panels.node_editor.snapshot();
        shell
            .panels
            .node_editor
            .add_node(motion_graph::NodeOp::Float { value: 2. }, [0., 0.])
            .unwrap();
        let edited = shell.panels.node_editor.document().clone();
        assert!(shell.open_node_example(&snapshot, "sdf_ring").is_err());
        assert_eq!(shell.panels.node_editor.document(), &edited);
        assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
        assert_eq!(
            shell.engine.state().unwrap()["revision"],
            before["revision"]
        );
    }
    #[test]
    fn adjustment_layer_ui_action_uses_shared_history_and_survives_reopen() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        let mut shell = Shell::new(root.clone(), None, Some(Locale::Zh)).unwrap();
        let before = shell.state["project"].clone();
        shell.action(panels::Action::AddAdjustment, None).unwrap();
        let id = shell.panels.selected.unwrap();
        let project: Project = serde_json::from_value(shell.state["project"].clone()).unwrap();
        let layer = project.layers.iter().find(|l| l.id == id).unwrap();
        assert!(matches!(layer.content, motion_core::Content::Adjustment));
        assert_eq!(layer.size, [project.width as f32, project.height as f32]);
        assert_eq!(layer.name, "调整图层");
        shell.action(panels::Action::History(0), None).unwrap();
        shell.state = shell
            .editing
            .take()
            .unwrap()
            .1
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(shell.state["project"], before);
        shell.action(panels::Action::History(1), None).unwrap();
        shell.state = shell
            .editing
            .take()
            .unwrap()
            .1
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        shell.action(panels::Action::Save, None).unwrap();
        drop(shell);
        let shell = Shell::new(root, None, Some(Locale::En)).unwrap();
        let project: Project = serde_json::from_value(shell.state["project"].clone()).unwrap();
        assert!(project
            .layers
            .iter()
            .any(|l| l.id == id && matches!(l.content, motion_core::Content::Adjustment)));
    }
    #[test]
    fn example_open_waits_for_actual_source_save_before_replacing_graph() {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
        let source = folder.path().join("saved.msgraph");
        shell
            .panels
            .node_editor
            .set_parameter_default("color", 0, 0.9)
            .unwrap();
        let snapshot = shell.panels.node_editor.snapshot();
        shell.node_example_after_save = Some((snapshot.clone(), "sdf_ring"));
        shell
            .node_tasks
            .submit_to(
                node_editor::Action::Save(snapshot.clone()),
                source.clone(),
                Locale::En,
                None,
            )
            .unwrap();
        assert_ne!(shell.panels.node_editor.document().id, "sdf_ring");
        let deadline = Instant::now() + Duration::from_secs(10);
        while shell.node_example_after_save.is_some() {
            assert!(Instant::now() < deadline);
            shell.poll_nodes().unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            motion_graph::GraphDocument::load(&source).unwrap(),
            snapshot.document
        );
        assert_eq!(shell.panels.node_editor.document().id, "sdf_ring");
        assert!(shell.panels.node_editor.snapshot().source_path.is_none());
    }
    #[test]
    fn wheel_and_trackpad_share_up_down_direction_at_display_scale() {
        for direction in [-1., 1.] {
            let lines = mouse_wheel_delta(MouseScrollDelta::LineDelta(0., direction), 2.);
            let pixels = mouse_wheel_delta(
                MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(
                    0.,
                    64. * f64::from(direction),
                )),
                2.,
            );
            assert_eq!(lines, pixels);
            assert_eq!(lines[1].signum(), -direction);
        }
    }
    #[test]
    fn unsaved_node_exit_routes_save_discard_cancel_without_project_edits() {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
        let state = shell.engine.state().unwrap();
        shell
            .panels
            .node_editor
            .add_node(motion_graph::NodeOp::Float { value: 2. }, [0., 0.])
            .unwrap();
        shell.request_close();
        assert!(!shell.closing);
        assert!(shell.node_close_prompt.is_some());
        let size = [1280., 800.];
        let card_min = [380., 295.];
        let click = |index: f32| {
            let point = [
                card_min[0] + 172. + index * 110. + 50.,
                card_min[1] + 210. - 54. + 15.,
            ];
            Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            }
        };
        shell.input = click(2.);
        shell.draw_commands(size);
        assert!(!shell.closing);
        assert!(shell.node_close_prompt.is_none());
        assert!(shell.panels.node_editor.dirty());
        shell.request_close();
        shell.input = click(0.);
        shell.draw_commands(size);
        assert!(!shell.closing);
        assert!(shell.node_close_save.is_some());
        assert!(shell
            .actions
            .iter()
            .any(|a| matches!(a, panels::Action::Node(node_editor::Action::Save(_)))));
        // A save request only closes after the worker acknowledges actual source bytes.
        shell.actions.clear();
        shell.node_close_save = None;
        shell.request_close();
        shell.input = click(1.);
        shell.draw_commands(size);
        assert!(shell.closing);
        let after = shell.engine.state().unwrap();
        assert_eq!(state["revision"], after["revision"]);
        assert_eq!(state["project"], after["project"]);
    }
    #[test]
    fn node_exit_rejects_edits_made_during_confirmation() {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::Zh)).unwrap();
        shell
            .panels
            .node_editor
            .add_node(motion_graph::NodeOp::Float { value: 2. }, [0., 0.])
            .unwrap();
        shell.request_close();
        let captured = shell.node_close_prompt.as_ref().unwrap().clone();
        shell
            .panels
            .node_editor
            .add_node(motion_graph::NodeOp::Float { value: 3. }, [0., 0.])
            .unwrap();
        assert!(!shell.node_close_current(&captured));
        let point = [380. + 172. + 110. + 50., 295. + 210. - 54. + 15.];
        shell.input = Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        };
        shell.draw_commands([1280., 800.]);
        assert!(!shell.closing);
        assert!(shell.node_close_prompt.is_none());
        assert_eq!(
            shell.panels.node_editor.document().nodes.len(),
            motion_graph::demo_graph().nodes.len() + 2
        );
    }
    #[test]
    fn playback_wraps_without_sampling_the_invalid_end_frame() {
        assert_eq!(playback_frame(29.0, Duration::from_secs(1), 30, 30), 29.0);
        for ms in [0, 33, 1000, 100000] {
            let f = playback_frame(179.0, Duration::from_millis(ms), 60, 180);
            assert!((0.0..180.0).contains(&f));
        }
    }
    #[test]
    fn keyboard_chords_use_character_keys() {
        assert_eq!(
            translate_key(&WinitKey::Character("S".into())),
            Some(Key::S)
        );
    }
}
