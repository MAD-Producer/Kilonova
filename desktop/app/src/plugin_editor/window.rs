//! Standalone native interaction preview. No user project is opened or saved.
use super::*;
use crate::engine::{AtlasUpload, Engine, UiFrame};
use motion_ui::input::{Event, Modifiers};
use std::{
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton as WinitButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const STAMP: (u64, u64) = (1, 1);
#[cfg(all(test, target_os = "windows"))]
#[path = "window_acceptance.rs"]
mod acceptance;
#[path = "window_host.rs"]
mod host;
pub use host::Hosted;
pub(super) type Prepared = (
    motion_graph::plugin_project::PluginProject,
    crate::node_preview::PreparedPlugin,
);
type Reply<T> = Receiver<Result<T, String>>;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "motion-plugin-preview-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).map_err(|e| {
            format!(
                "create isolated plugin preview session {}: {e}",
                path.display()
            )
        })?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        // Created exclusively above; never a user-supplied or computed parent path.
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            log::error!(
                "plugin.preview cleanup source={} cause={error}",
                crate::fallback_observation::short(self.0.display())
            );
        }
    }
}
pub fn run(source: &Path, zh: bool) -> Result<(), Box<dyn std::error::Error>> {
    run_mode(source, zh, false)
}
pub fn run_author(source: &Path, zh: bool) -> Result<(), Box<dyn std::error::Error>> {
    run_mode(source, zh, true)
}
fn run_mode(source: &Path, zh: bool, author_mode: bool) -> Result<(), Box<dyn std::error::Error>> {
    let (mut app, worker) = prepare_window(source, zh, author_mode)?;
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;
    if let Err(payload) = worker.join() {
        let error = format!(
            "plugin preview prepare worker panicked: {}",
            motion_media::panic_reason(payload)
        );
        log::error!("{}", crate::fallback_observation::error_cause(&error));
        return Err(error.into());
    }
    if let Some(error) = app.error.take() {
        return Err(error.into());
    }
    Ok(())
}
fn prepare_window(
    source: &Path,
    zh: bool,
    author_mode: bool,
) -> Result<(App, std::thread::JoinHandle<()>), String> {
    let scratch = Scratch::new()?;
    let engine = Engine::start(
        scratch.0.clone(),
        motion_desktop_media::platform(),
        crate::total_memory(),
    )?;
    let styles = motion_ui::style::default_stylesheet_arc();
    let atlas = TextAtlas::from_family(styles.font_family(), styles.base_font_size(), zh)?;
    let (send, load) = mpsc::sync_channel(1);
    let (author_send, author_load) = mpsc::sync_channel(1);
    let path = source.to_owned();
    let preparation = Arc::new(super::preparation::Preparation::new(source));
    let flight = preparation.clone();
    let worker = std::thread::Builder::new()
        .name("plugin-preview-prepare".into())
        .spawn(move || {
            if author_mode {
                flight.phase.store(3, std::sync::atomic::Ordering::Release);
                log::info!(
                    "plugin.author start request=0 phase=read in_flight=true source={}",
                    crate::fallback_observation::short(path.display())
                );
                struct ReadScope(Arc<super::preparation::Preparation>);
                impl Drop for ReadScope { fn drop(&mut self) {
                    self.0.phase.store(0,std::sync::atomic::Ordering::Release);
                    if std::thread::panicking() { log::error!("plugin.author aborted request=0 phase=read elapsed_ms={} cause=panic; original cause recorded by process hook",self.0.began.elapsed().as_millis()); }
                    else { log::debug!("plugin.author done request=0 phase=read elapsed_ms={}",self.0.began.elapsed().as_millis()); }
                } }
                let scope = ReadScope(flight);
                crate::logging::flush();
                let result = motion_graph::plugin_project::PluginProject::load(&path)
                    .map_err(|e| e.to_string());
                if let Err(error) = &result {
                    log::error!(
                        "plugin.author error request=0 phase=read cause={}",
                        crate::fallback_observation::error_cause(error)
                    );
                }
                drop(scope);
                let _receiver_closed = author_send.send(result);
                return;
            }
            let result = flight.prepare(&path);
            // A closed window intentionally abandons this protocol reply.
            let _receiver_closed = send.send(result);
        }).map_err(|e| format!("start plugin source reader: {e}"))?;
    let app = App {
        engine,
        atlas,
        styles,
        window: None,
        input: Input {
            text_clipboard: Some(Default::default()),
            ..Default::default()
        },
        pointer: Default::default(),
        preview: None,
        load: (!author_mode).then_some(load),
        author: None,
        author_path: author_mode.then(|| source.to_owned()),
        author_load: author_mode.then_some(author_load),
        author_installing: None,
        stamp: STAMP,
        prepared: None,
        attach: None,
        install: None,
        values: None,
        present: None,
        resize: None,
        desired_size: None,
        latest: None,
        revision: u64::MAX,
        installed: false,
        dirty: true,
        scale: 1.,
        error: None,
        zh,
        force_atlas: true,
        preparation,
        frame_started: false,
        frame_visible: false,
        startup_warned: false,
        embedded: false,
        finished: false,
        _scratch: scratch,
    };
    Ok((app, worker))
}
struct App {
    engine: Engine,
    atlas: TextAtlas,
    styles: Arc<motion_ui::style::StyleSheet>,
    window: Option<Arc<Window>>,
    input: Input,
    pointer: crate::shell::pointer_position::PointerPosition,
    preview: Option<Preview>,
    author: Option<super::author::Workspace>,
    author_path: Option<PathBuf>,
    author_load: Option<Reply<motion_graph::plugin_project::PluginProject>>,
    author_installing: Option<(u64, Vec<motion_effects::ParamDefinition>)>,
    stamp: (u64, u64),
    load: Option<Reply<Prepared>>,
    prepared: Option<crate::node_preview::PreparedPlugin>,
    attach: Option<Reply<serde_json::Value>>,
    install: Option<Reply<bool>>,
    values: Option<Reply<bool>>,
    present: Option<Reply<bool>>,
    resize: Option<Reply<serde_json::Value>>,
    desired_size: Option<[u32; 2]>,
    latest: Option<UiFrame>,
    revision: u64,
    installed: bool,
    dirty: bool,
    scale: f32,
    error: Option<String>,
    zh: bool,
    force_atlas: bool,
    preparation: Arc<super::preparation::Preparation>,
    frame_started: bool,
    frame_visible: bool,
    startup_warned: bool,
    embedded: bool,
    finished: bool,
    // Last: engine/surfaces and author workers release before scratch cleanup.
    _scratch: Scratch,
}
fn poll<T>(slot: &mut Option<Reply<T>>, operation: &str) -> Option<Result<T, String>> {
    let reply = slot.as_ref()?;
    match reply.try_recv() {
        Ok(result) => {
            *slot = None;
            Some(result)
        }
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => {
            *slot = None;
            Some(Err(format!(
                "plugin preview {operation}: worker disconnected before replying"
            )))
        }
    }
}
pub(super) fn paint_outside(
    list: &mut PaintList,
    area: Rect,
    hole: Option<Rect>,
    color: motion_ui::Color,
) {
    let Some(hole) = hole
        .map(|h| h.intersection(area))
        .filter(|h| h.width() > 0. && h.height() > 0.)
    else {
        paint::rect(list, area, color);
        return;
    };
    for r in [
        Rect::new(
            area.min[0],
            area.min[1],
            area.width(),
            hole.min[1] - area.min[1],
        ),
        Rect::new(
            area.min[0],
            hole.max[1],
            area.width(),
            area.max[1] - hole.max[1],
        ),
        Rect::new(
            area.min[0],
            hole.min[1],
            hole.min[0] - area.min[0],
            hole.height(),
        ),
        Rect::new(
            hole.max[0],
            hole.min[1],
            area.max[0] - hole.max[0],
            hole.height(),
        ),
    ] {
        paint::rect(list, r, color);
    }
}
impl App {
    fn exit(&mut self, event_loop: &ActiveEventLoop) {
        self.finished = true;
        if !self.embedded {
            event_loop.exit();
        }
    }
    fn pending(&self) -> bool {
        self.load.is_some()
            || self.author_load.is_some()
            || self.author.as_ref().is_some_and(|a| a.pending())
            || self.attach.is_some()
            || self.install.is_some()
            || self.values.is_some()
            || self.present.is_some()
            || self.resize.is_some()
    }
    fn fail(&mut self, operation: &str, cause: String) {
        log::error!(
            "plugin.preview error request=1 phase={operation} cause={}",
            crate::fallback_observation::short(&cause)
        );
        // Keep the original failure visible if presenting/closing its message
        // also fails. The secondary cause still has its own Error record.
        if self.error.is_none() {
            if let Some(author) = &mut self.author {
                author.runtime_error(format!("{operation}: {cause}"));
            }
            self.error = Some(format!(
                "{operation}: {}",
                crate::fallback_observation::short(cause)
            ));
        }
        self.dirty = true;
    }
    fn event(&mut self, event: Event) {
        ui::accumulate(&mut self.input, event);
        self.dirty = true;
    }
    fn draw(&mut self) -> Result<UiFrame, String> {
        let window = self.window.as_ref().ok_or("preview window missing")?;
        let physical = window.inner_size();
        let width = physical.width as f32 / self.scale;
        let height = physical.height as f32 / self.scale;
        let bounds = Rect::new(0., 0., width, height);
        let s = &self.styles;
        let colors = s.colors();
        let pad = s.metric("padding.panel", 8.);
        let row = s.metric("panel.header.height", 24.);
        let mut paint = self.engine.take_paint();
        paint.stylesheet = Some(s.clone());
        if let Some(author) = &mut self.author {
            let slot = author.draw(&mut paint, &mut self.atlas, bounds.shrink(pad), &self.input)?;
            let viewport = slot
                .filter(|r| self.installed && r.width() > 0. && r.height() > 0.)
                .map(|r| crate::node_preview::Viewport {
                    rect: [
                        r.min[0] * self.scale,
                        r.min[1] * self.scale,
                        r.width() * self.scale,
                        r.height() * self.scale,
                    ],
                    seconds: 0.,
                });
            let mut background = PaintList {
                stylesheet: Some(s.clone()),
                ..Default::default()
            };
            paint_outside(
                &mut background,
                bounds,
                viewport.map(|v| {
                    Rect::new(
                        v.rect[0] / self.scale,
                        v.rect[1] / self.scale,
                        v.rect[2] / self.scale,
                        v.rect[3] / self.scale,
                    )
                }),
                colors.PANEL,
            );
            background.append(paint);
            return Ok(UiFrame {
                paint: background,
                atlas: None,
                logical_size: [width, height],
                composition: None,
                composition_image: None,
                inspection: Default::default(),
                window: 0,
                node_preview: viewport,
            });
        }
        let mut background_canvas = None;
        let mut preview_hole = None;
        let title = tr(
            self.zh,
            "插件参数预览 · 独立示例，未连接工程",
            "Plugin parameter preview · independent sample, no host project",
        );
        ui::text(
            &mut paint,
            &mut self.atlas,
            Rect::new(pad, 0., (width - pad * 2.).max(0.), row),
            &title,
            colors.TEXT_MUTED,
            Align::Left,
        );
        let body = Rect::new(
            pad,
            row * 2. + pad,
            (width - pad * 2.).max(0.),
            (height - row * 2. - pad * 2.).max(0.),
        );
        let mut viewport = None;
        if let Some(preview) = &mut self.preview {
            let disabled = neutral();
            let event = if preview.owner().is_none()
                && preview.color.is_none()
                && !preview.session.active()
            {
                &self.input
            } else {
                &disabled
            };
            let action = s.metric("inspector.action.width", 112.);
            let gap = s.metric("padding", 5.);
            for (i, (label, enabled)) in [
                (tr(self.zh, "撤销", "Undo"), preview.session.can_undo()),
                (tr(self.zh, "重做", "Redo"), preview.session.can_redo()),
                (tr(self.zh, "重置示例", "Reset sample"), true),
            ]
            .into_iter()
            .enumerate()
            {
                let r = Rect::new(pad + i as f32 * (action + gap), row, action, row);
                if ui::button(&mut paint, &mut self.atlas, r, &label, event, enabled).clicked {
                    match i {
                        0 => preview.session.undo(),
                        1 => preview.session.redo(),
                        _ => preview.session.reset(),
                    }
                }
            }
            let panel = s.metric("plugin.preview.panel.width", 360.);
            let min_canvas = s.metric("plugin.preview.canvas.min_width", 240.);
            let embedded = preview.has_preview();
            let (canvas, parameters) = if embedded {
                (Rect::new(body.min[0], body.min[1], 0., 0.), body)
            } else if body.width() >= panel + min_canvas + pad {
                (
                    Rect::new(
                        body.min[0],
                        body.min[1],
                        body.width() - panel - pad,
                        body.height(),
                    ),
                    Rect::new(body.max[0] - panel, body.min[1], panel, body.height()),
                )
            } else {
                let canvas_height = s
                    .metric("plugin.preview.canvas.min_height", 200.)
                    .min(body.height() * 0.5);
                (
                    Rect::new(body.min[0], body.min[1], body.width(), canvas_height),
                    Rect::new(
                        body.min[0],
                        body.min[1] + canvas_height + pad,
                        body.width(),
                        (body.height() - canvas_height - pad).max(0.),
                    ),
                )
            };
            if !embedded {
                background_canvas = Some(canvas);
            }
            let inner = canvas.shrink(pad);
            let side = inner.width().min(inner.height()).max(0.);
            let sample = Rect::new(
                inner.min[0] + (inner.width() - side) * 0.5,
                inner.min[1] + (inner.height() - side) * 0.5,
                side,
                side,
            );
            let slot = preview.draw(
                &mut paint,
                &mut self.atlas,
                parameters,
                parameters.intersection(bounds),
                &self.input,
            )?;
            if let Some(rect) = slot
                .or_else(|| (!embedded).then_some(sample))
                .filter(|r| self.installed && r.width() > 0. && r.height() > 0.)
            {
                let side = rect.width().min(rect.height());
                let rect = Rect::new(
                    rect.min[0] + (rect.width() - side) * 0.5,
                    rect.min[1] + (rect.height() - side) * 0.5,
                    side,
                    side,
                );
                preview_hole = Some(rect);
                viewport = Some(crate::node_preview::Viewport {
                    rect: [
                        rect.min[0] * self.scale,
                        rect.min[1] * self.scale,
                        rect.width() * self.scale,
                        rect.height() * self.scale,
                    ],
                    seconds: 0.,
                });
            }
        } else {
            let message = if self.error.is_some() {
                tr(
                    self.zh,
                    "无法预览插件，请检查源文件和日志",
                    "Cannot preview this plugin; check the source file and log",
                )
            } else if self
                .preparation
                .phase
                .load(std::sync::atomic::Ordering::Acquire)
                == 2
            {
                tr(
                    self.zh,
                    "正在准备插件图像资源…",
                    "Preparing plugin image resources…",
                )
            } else {
                tr(
                    self.zh,
                    "正在读取并编译插件源文件…",
                    "Reading and compiling plugin source…",
                )
            };
            ui::text(
                &mut paint,
                &mut self.atlas,
                body,
                &message,
                colors.TEXT_MUTED,
                Align::Center,
            );
        }
        if let Some(error) = &self.error {
            let r = Rect::new(pad, height - row - pad, (width - pad * 2.).max(0.), row);
            paint::rect(&mut paint, r, colors.PANEL);
            ui::text(
                &mut paint,
                &mut self.atlas,
                r,
                error,
                s.color("error"),
                Align::Left,
            );
        }
        // Plugin content is composed below native chrome, so menus remain above
        // the shader even when the authored layout contains its own viewport.
        let mut background = PaintList {
            stylesheet: Some(s.clone()),
            ..Default::default()
        };
        paint_outside(&mut background, bounds, preview_hole, colors.PANEL);
        if let Some(canvas) = background_canvas {
            paint_outside(&mut background, canvas, preview_hole, colors.SUNKEN);
        }
        background.append(paint);
        Ok(UiFrame {
            paint: background,
            atlas: None,
            logical_size: [width, height],
            composition: None,
            composition_image: None,
            inspection: Default::default(),
            window: 0,
            node_preview: viewport,
        })
    }
    fn poll_state(&mut self) -> Result<(), String> {
        if let Some(result) = poll(&mut self.author_load, "author_read") {
            match result {
                Ok(project) => {
                    self.author = Some(super::author::Workspace::new(
                        project,
                        self.author_path
                            .clone()
                            .ok_or("author source path missing")?,
                        self.zh,
                    )?);
                    self.dirty = true;
                }
                Err(error) => self.fail("author_read", error),
            }
        }
        if let Some(author) = &mut self.author {
            author.poll()?;
            self.dirty |= author.redraw;
            if self.attach.is_none() && self.install.is_none() && self.values.is_none() {
                if let Some((revision, ready)) = author.take_ready() {
                    let definitions = ready.prepared.definitions().to_vec();
                    self.stamp = (2, revision);
                    self.engine.node_expect(self.stamp)?;
                    self.install = Some(self.engine.plugin_install(self.stamp, ready.prepared)?);
                    self.author_installing = Some((revision, definitions));
                }
            }
        }
        if let Some(result) = poll(&mut self.load, "prepare") {
            match result {
                Ok((project, prepared)) => {
                    self.preview = Some(Preview::new(
                        project.interface,
                        prepared.definitions().to_vec(),
                        self.zh,
                    )?);
                    self.prepared = Some(prepared);
                    self.dirty = true;
                }
                Err(error) => self.fail("prepare", error),
            }
        }
        if let Some(result) = poll(&mut self.attach, "attach") {
            result?;
            self.engine.node_expect(self.stamp)?;
            self.dirty = true;
        }
        if self.attach.is_none() && self.window.is_some() && self.install.is_none() {
            if let Some(prepared) = self.prepared.take() {
                self.install = Some(self.engine.plugin_install(STAMP, prepared)?);
            }
        }
        if let Some(result) = poll(&mut self.install, "install") {
            if let Some((revision, definitions)) = self.author_installing.take() {
                if let Some(author) = &mut self.author {
                    match result {
                        Ok(true) => {
                            author.install_preview(revision, definitions)?;
                            self.installed = true;
                            self.revision = u64::MAX;
                        }
                        Ok(false) => {}
                        Err(error) => author.runtime_error(error),
                    }
                }
                self.dirty = true;
            } else {
                if !result? {
                    return Err("plugin preview installation was superseded".into());
                }
                self.installed = true;
                self.dirty = true;
            }
        }
        if let Some(result) = poll(&mut self.values, "values") {
            if let Some(author) = &mut self.author {
                match result {
                    Ok(true) => {}
                    Ok(false) => { /* Normal superseded runtime revision. */ }
                    Err(error) => author.runtime_error(error),
                }
            } else if !result? {
                return Err("plugin preview parameters were superseded".into());
            }
            self.dirty = true;
        }
        if let Some(result) = poll(&mut self.present, "present") {
            let visible = result?;
            if visible && !self.frame_visible {
                self.frame_visible = true;
                log::debug!(
                    "plugin.preview done request=1 phase=first_frame elapsed_ms={}",
                    self.preparation.began.elapsed().as_millis()
                );
            }
            if !visible {
                // The engine has already classified/reconfigured a transient
                // surface miss. Keep the UI dirty until a frame is presented;
                // consuming the frame here can leave the initial window blank.
                self.dirty = true;
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
        if let Some(result) = poll(&mut self.resize, "resize") {
            result?;
            self.dirty = true;
        }
        if self.attach.is_none() && self.resize.is_none() {
            if let Some([width, height]) = self.desired_size.take() {
                self.resize = Some(self.engine.resize_async(0, width, height)?);
            }
        }
        if !self.frame_visible
            && !self.startup_warned
            && self.preparation.began.elapsed() >= Duration::from_secs(5)
        {
            self.startup_warned = true;
            log::warn!("plugin.preview first frame delayed request=1 elapsed_ms={} attached={} installed={} resize_pending={} latest_paint={} present_pending={} engine_in_flight={:?}",self.preparation.began.elapsed().as_millis(),self.attach.is_none(),self.installed,self.resize.is_some(),self.latest.is_some(),self.present.is_some(),self.engine.in_flight());
        }
        Ok(())
    }
    fn pump(&mut self) -> Result<(), String> {
        self.poll_state()?;
        let drawable = self.window.as_ref().is_some_and(|w| {
            let s = w.inner_size();
            s.width > 0 && s.height > 0
        });
        if self.dirty && drawable {
            self.latest = Some(self.draw()?);
            self.input.begin_frame();
            self.dirty = self.preview.as_ref().is_some_and(|p| p.redraw)
                || self.author.as_ref().is_some_and(|a| a.redraw);
            if let (Some(window), Some(preview)) = (&self.window, &self.preview) {
                window.set_cursor(preview.cursor);
            }
        }
        if self.installed && self.values.is_none() {
            if let Some(author) = &self.author {
                if author.runtime_current() && author.value_revision() != self.revision {
                    self.values = Some(
                        self.engine
                            .plugin_values(self.stamp, author.values().to_vec())?,
                    );
                    self.revision = author.value_revision();
                }
            }
            if let Some(preview) = &self.preview {
                if preview.session.revision != self.revision {
                    self.values = Some(
                        self.engine
                            .plugin_values(STAMP, preview.session.values().to_vec())?,
                    );
                    self.revision = preview.session.revision;
                }
            }
        }
        // Interaction passes keep consuming input while an older GPU present is
        // pending. Only the newest paint is retained; parameter requests are bounded.
        if drawable
            && self.attach.is_none()
            && self.resize.is_none()
            && self.desired_size.is_none()
            && self.present.is_none()
        {
            if let Some(mut frame) = self.latest.take() {
                if !self.frame_started {
                    self.frame_started = true;
                    log::info!("plugin.preview start request=1 phase=first_frame in_flight=true logical_size={:?}",frame.logical_size);
                    crate::logging::flush();
                }
                if self.atlas.take_dirty() || self.force_atlas {
                    let config = self.atlas.config();
                    frame.atlas = Some(AtlasUpload {
                        pixels: self.atlas.pixel_data().to_vec(),
                        width: config.width,
                        height: config.height,
                    });
                    self.force_atlas = false;
                }
                self.present = Some(self.engine.present_async(frame)?);
            }
        }
        Ok(())
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| {
            let s = &self.styles;
            let attributes = Window::default_attributes()
                .with_title(tr(
                    self.zh,
                    "Motion Studio · 插件参数预览",
                    "Motion Studio · Plugin Parameter Preview",
                ))
                .with_inner_size(LogicalSize::new(
                    s.metric("plugin.preview.panel.width", 360.)
                        + s.metric("plugin.preview.canvas.min_width", 240.) * 2.
                        + s.metric("padding.panel", 8.) * 2.,
                    s.metric("plugin.preview.canvas.min_height", 200.) * 3.
                        + s.metric("panel.header.height", 24.) * 2.
                        + s.metric("padding.panel", 8.) * 2.,
                ))
                .with_min_inner_size(LogicalSize::new(
                    s.metric("plugin.preview.window.min_width", 360.),
                    s.metric("plugin.preview.window.min_height", 400.),
                ));
            let attributes = if self.author_path.is_some() {
                let unit = s.metric("plugin.author.window.unit", 480.);
                attributes
                    .with_title(tr(
                        self.zh,
                        "Motion Studio · 插件编辑器",
                        "Motion Studio · Plugin Editor",
                    ))
                    .with_inner_size(LogicalSize::new(unit * 3., unit * 1.75))
                    .with_min_inner_size(LogicalSize::new(unit * 1.6, unit * 1.2))
            } else {
                attributes
            };
            let window = Arc::new(
                event_loop
                    .create_window(attributes)
                    .map_err(|e| e.to_string())?,
            );
            self.scale = window.scale_factor() as f32;
            self.atlas.set_pixel_ratio(self.scale);
            window.set_ime_allowed(true);
            let size = window.inner_size();
            self.attach = Some(self.engine.attach_async(
                window.clone(),
                size.width,
                size.height,
            )?);
            self.window = Some(window);
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            self.fail("window_attach", error);
            self.exit(event_loop);
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|w| w.id() != id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                if let Some(author) = &mut self.author {
                    author.request_close();
                    self.dirty = true;
                    return;
                }
                if let Some(p) = &mut self.preview {
                    p.cancel();
                }
                self.exit(event_loop);
            }
            WindowEvent::Resized(size) => {
                if let Some(author) = &mut self.author {
                    author.cancel();
                }
                if let Some(p) = &mut self.preview {
                    p.cancel();
                }
                if size.width > 0 && size.height > 0 {
                    self.desired_size = Some([size.width, size.height]);
                }
                self.dirty = true;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(author) = &mut self.author {
                    author.cancel();
                }
                if let Some(p) = &mut self.preview {
                    p.cancel();
                }
                self.scale = scale_factor as f32;
                self.atlas.set_pixel_ratio(self.scale);
                self.force_atlas = true;
                self.dirty = true;
            }
            WindowEvent::ModifiersChanged(m) => {
                let m = m.state();
                self.input.modifiers = Modifiers {
                    control: m.control_key() || m.super_key(),
                    shift: m.shift_key(),
                    alt: m.alt_key(),
                };
            }
            WindowEvent::CursorMoved { position, .. } => {
                let event = self.pointer.moved(position, self.scale);
                self.event(event);
            }
            #[cfg(target_os = "windows")]
            WindowEvent::CursorEntered { .. } => {
                if let Some(event) = self.pointer.entered(self.scale) {
                    self.event(event);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if let Some(author) = &mut self.author {
                    author.cancel();
                }
                if let Some(preview) = &mut self.preview {
                    preview.cancel();
                }
                self.event(Event::PointerLeft);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    WinitButton::Left => MouseButton::Left,
                    WinitButton::Right => MouseButton::Right,
                    WinitButton::Middle => MouseButton::Middle,
                    _ => return,
                };
                let position = self.input.mouse;
                self.event(if state == ElementState::Pressed {
                    Event::MousePressed { position, button }
                } else {
                    Event::MouseReleased { position, button }
                });
            }
            WindowEvent::MouseWheel { delta, .. } => self.event(Event::MouseWheel {
                delta: crate::mouse_wheel_delta(delta, self.scale),
            }),
            WindowEvent::KeyboardInput { event, .. } => {
                for event in crate::keyboard_events(&event, &self.input) {
                    self.event(event)
                }
            }
            WindowEvent::Ime(event) => {
                if let Some(event) = crate::translate_ime(event) {
                    self.event(event)
                }
            }
            WindowEvent::Focused(false) => {
                if let Some(author) = &mut self.author {
                    author.cancel();
                }
                self.event(Event::FocusLost);
            }
            WindowEvent::RedrawRequested => {
                self.dirty = true;
                if let Some(window) = &self.window {
                    window.pre_present_notify();
                }
                if let Err(error) = self.pump() {
                    self.fail("runtime", error);
                    self.exit(event_loop);
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.poll_state() {
            self.fail("runtime", error);
            self.exit(event_loop);
        }
        if self.author.as_ref().is_some_and(|a| a.close_ready) {
            self.exit(event_loop);
            return;
        }
        // Match the owning shell: submit inside RedrawRequested, after the OS
        // paint notification. Worker polling itself never presents a surface.
        let can_submit = self.attach.is_none()
            && self.resize.is_none()
            && self.desired_size.is_none()
            && self.present.is_none();
        if self.dirty || (self.latest.is_some() && can_submit) {
            if let Some(window) = &self.window {
                let size = window.inner_size();
                if size.width > 0 && size.height > 0 {
                    window.request_redraw();
                }
            }
        }
        if !self.embedded {
            event_loop.set_control_flow(if self.pending() {
                ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(8))
            } else {
                ControlFlow::Wait
            });
        }
    }
}
