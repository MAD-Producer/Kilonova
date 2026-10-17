//! Native UI component gallery. No project, media, JS or editor session is loaded.
#[path = "framework/acceptance.rs"]
mod acceptance;
#[path = "framework/native.rs"]
mod native;

use motion_ui::{
    dock::{self, DockId, Edge},
    interaction::{HitRegion, Router},
    paint::{self, PaintList},
    panel::{Lifecycle, PanelFrame, PanelHost, PanelLayout, PanelView},
    registry::{Language, PanelDescriptor},
    style::{self, State, States},
    theme::metrics,
    ui::{self, Align},
    widgets::{Button, NumberEditor, NumberEvent, NumberOptions, TextEditor, TextEvent},
    workspace::Workspace,
    Input, Painter, Rect, TextAtlas,
};
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key as WKey, NamedKey},
    window::{Window, WindowId},
};

#[derive(Default, Clone, PartialEq)]
struct Model {
    value: f64,
    transaction: Option<f64>,
    undo: Vec<f64>,
    text: String,
    commits: u32,
}
enum Action {
    Number(NumberEvent),
    Text(String),
    Undo,
}
struct View {
    id: DockId,
    model: Rc<RefCell<Model>>,
    number: NumberEditor,
    text: TextEditor,
    undo: Button,
    show_icons: bool,
}
fn field_rect(area: Rect, index: usize) -> Rect {
    Rect::new(
        area.min[0] + 14.,
        area.min[1] + 44. + index as f32 * 62.,
        (area.width() - 28.).max(0.),
        28.,
    )
}
impl PanelView for View {
    type Action = Action;
    fn hit_regions(&self, layout: PanelLayout) -> Vec<HitRegion> {
        if self.id.0 != 2 {
            return vec![HitRegion {
                target: layout.target(0),
                rect: layout.bounds,
                clip: layout.clip,
                layer: 0,
                keyboard: true,
            }];
        }
        (1..=3)
            .map(|i| HitRegion {
                target: layout.target(i),
                rect: field_rect(layout.bounds, (i - 1) as usize),
                clip: layout.clip,
                layer: 1,
                keyboard: true,
            })
            .collect()
    }
    fn lifecycle(&mut self, _event: Lifecycle) -> Vec<Action> {
        self.text.cancel();
        self.number
            .cancel()
            .into_iter()
            .map(Action::Number)
            .collect()
    }
    fn draw(&mut self, frame: &mut PanelFrame) -> Vec<Action> {
        let area = frame.layout.bounds;
        let zh = frame.language == Language::Chinese;
        let styles = frame.styles;
        let mut actions = vec![];
        paint::rect(
            frame.paint,
            area,
            styles
                .resolve("panel", States::default())
                .unwrap()
                .background,
        );
        if self.id.0 == 2 {
            let value = self.model.borrow().value;
            let input = frame.input_for(1);
            let number = field_rect(area, 0);
            style::text_style(
                frame.paint,
                frame.atlas,
                Rect::new(number.min[0], number.min[1] - 24., number.width(), 22.),
                if zh {
                    "数值 · 拖动 / 单击输入"
                } else {
                    "Value · drag / click to type"
                },
                motion_ui::style::ResolvedStyle {
                    foreground: styles
                        .resolve("label.muted", States::default())
                        .unwrap()
                        .foreground,
                    ..styles.resolve("label", States::default()).unwrap()
                },
                Align::Left,
            );
            let options = NumberOptions {
                speed: 0.5,
                ..Default::default()
            };
            actions.extend(
                self.number
                    .update(value, options, &input, number)
                    .into_iter()
                    .map(Action::Number),
            );
            self.number.draw_styled(
                frame.paint,
                frame.atlas,
                number,
                value,
                options,
                styles,
                if number.contains(input.mouse) {
                    States::default().with(State::Hovered)
                } else {
                    States::default()
                },
            );
            if let Some(error) = &self.number.text.error {
                style::text_style(
                    frame.paint,
                    frame.atlas,
                    Rect::new(number.min[0], number.max[1] + 2., number.width(), 20.),
                    error,
                    motion_ui::style::ResolvedStyle {
                        foreground: styles
                            .resolve("label.muted", States::default())
                            .unwrap()
                            .foreground,
                        ..styles.resolve("label", States::default()).unwrap()
                    },
                    Align::Left,
                );
            }
            let text = field_rect(area, 1);
            let input = frame.input_for(2);
            style::text_style(
                frame.paint,
                frame.atlas,
                Rect::new(text.min[0], text.min[1] - 24., text.width(), 22.),
                if zh {
                    "文本 · Unicode / 选中替换"
                } else {
                    "Text · Unicode / selection"
                },
                motion_ui::style::ResolvedStyle {
                    foreground: styles
                        .resolve("label.muted", States::default())
                        .unwrap()
                        .foreground,
                    ..styles.resolve("label", States::default()).unwrap()
                },
                Align::Left,
            );
            if input.pressed.is_some() {
                self.text.begin(&self.model.borrow().text);
            }
            if let Some(TextEvent::Commit(value)) = self.text.input(&input) {
                actions.push(Action::Text(value));
                self.text.cancel();
            }
            if !self.text.active() {
                self.text.text = self.model.borrow().text.clone();
            }
            self.text.draw_styled(
                frame.paint,
                frame.atlas,
                text,
                styles,
                "field",
                if text.contains(input.mouse) {
                    States::default().with(State::Hovered)
                } else {
                    States::default()
                },
            );
            let input = frame.input_for(3);
            let button_states = if frame.is_focused(3) {
                States::default().with(State::Focused)
            } else {
                States::default()
            };
            if self
                .undo
                .draw_with_states(
                    frame.paint,
                    frame.atlas,
                    field_rect(area, 2),
                    if zh {
                        "撤销数值修改"
                    } else {
                        "Undo numeric edit"
                    },
                    &input,
                    !self.model.borrow().undo.is_empty(),
                    styles,
                    "button",
                    button_states,
                )
                .clicked
            {
                actions.push(Action::Undo);
            }
            style::text_style(
                frame.paint,
                frame.atlas,
                Rect::new(
                    area.min[0] + 14.,
                    area.max[1] - 52.,
                    area.width() - 28.,
                    44.,
                ),
                if zh {
                    "Shift 精细 · Ctrl 粗调 · Esc 取消"
                } else {
                    "Shift fine · Ctrl coarse · Esc cancel"
                },
                motion_ui::style::ResolvedStyle {
                    foreground: styles
                        .resolve("label.muted", States::default())
                        .unwrap()
                        .foreground,
                    ..styles.resolve("label", States::default()).unwrap()
                },
                Align::Left,
            );
        } else if self.id.0 == 3 {
            if self.show_icons {
                let columns = ((area.width() - 24.) / 86.).floor().max(1.) as usize;
                for (i, icon) in motion_ui::icons::Icon::ALL.iter().enumerate() {
                    let cell = Rect::new(
                        area.min[0] + 12. + (i % columns) as f32 * 86.,
                        area.min[1] + 12. + (i / columns) as f32 * 64.,
                        80.,
                        60.,
                    );
                    motion_ui::icons::draw(
                        frame.paint,
                        Rect::new(cell.min[0], cell.min[1], 80., 36.),
                        *icon,
                        28.,
                        styles
                            .resolve("icon", States::default())
                            .unwrap()
                            .foreground,
                    );
                    style::text_style(
                        frame.paint,
                        frame.atlas,
                        Rect::new(cell.min[0], cell.min[1] + 36., 80., 20.),
                        icon.id(),
                        styles.resolve("label.caption", States::default()).unwrap(),
                        Align::Center,
                    );
                }
                return actions;
            }
            paint::rect(
                frame.paint,
                area,
                styles
                    .resolve("canvas", States::default())
                    .unwrap()
                    .background,
            );
            let card = Rect::new(
                area.min[0] + area.width() * 0.14,
                area.min[1] + area.height() * 0.22,
                area.width() * 0.72,
                area.height() * 0.52,
            );
            paint::rect(
                frame.paint,
                card,
                styles
                    .resolve("canvas.content", States::default())
                    .unwrap()
                    .background,
            );
            style::text_style(
                frame.paint,
                frame.atlas,
                Rect::new(
                    card.min[0] + 26.,
                    card.min[1] + 22.,
                    card.width() - 52.,
                    28.,
                ),
                "MOTION STUDIO",
                motion_ui::style::ResolvedStyle {
                    foreground: styles
                        .resolve("label", States::default())
                        .unwrap()
                        .foreground,
                    ..styles.resolve("label.title", States::default()).unwrap()
                },
                Align::Left,
            );
            style::text_style(
                frame.paint,
                frame.atlas,
                Rect::new(
                    card.min[0] + 26.,
                    card.min[1] + 64.,
                    card.width() - 52.,
                    28.,
                ),
                if zh {
                    "UI 框架 · 独立面板"
                } else {
                    "UI Framework · Independent Panels"
                },
                motion_ui::style::ResolvedStyle {
                    foreground: styles
                        .resolve("label.accent", States::default())
                        .unwrap()
                        .foreground,
                    ..styles.resolve("label", States::default()).unwrap()
                },
                Align::Left,
            );
            let model = self.model.borrow();
            style::text_style(
                frame.paint,
                frame.atlas,
                Rect::new(
                    card.min[0] + 26.,
                    card.min[1] + 110.,
                    card.width() - 52.,
                    28.,
                ),
                &format!("{}  ·  {:.2}", model.text, model.value),
                motion_ui::style::ResolvedStyle {
                    foreground: styles
                        .resolve("label.muted", States::default())
                        .unwrap()
                        .foreground,
                    ..styles.resolve("label", States::default()).unwrap()
                },
                Align::Left,
            );
        } else if self.id.0 == 1 {
            for (i, text) in if zh {
                vec![
                    "面板组件",
                    "独立生命周期",
                    "停靠与标签",
                    "原生浮窗",
                    "可撤销数值",
                    "焦点与模态层",
                    "中英文标签",
                ]
            } else {
                vec![
                    "Panel components",
                    "Independent lifecycle",
                    "Dock and tabs",
                    "Native floating windows",
                    "Numeric transactions",
                    "Focus and modal scopes",
                    "Bilingual labels",
                ]
            }
            .into_iter()
            .enumerate()
            {
                style::text_style(
                    frame.paint,
                    frame.atlas,
                    Rect::new(
                        area.min[0] + 12.,
                        area.min[1] + 12. + i as f32 * 30.,
                        area.width() - 24.,
                        26.,
                    ),
                    text,
                    motion_ui::style::ResolvedStyle {
                        foreground: if i == 0 {
                            styles
                                .resolve("label", States::default())
                                .unwrap()
                                .foreground
                        } else {
                            styles
                                .resolve("label.muted", States::default())
                                .unwrap()
                                .foreground
                        },
                        ..styles.resolve("label", States::default()).unwrap()
                    },
                    Align::Left,
                );
            }
        } else {
            style::text_style(
                frame.paint,
                frame.atlas,
                area.shrink(14.),
                if zh {
                    "拖动面板标题重新排列 · ↗ 浮动 · 停靠返回"
                } else {
                    "Drag panel titles to rearrange · ↗ detach · Dock to return"
                },
                motion_ui::style::ResolvedStyle {
                    foreground: styles
                        .resolve("label.muted", States::default())
                        .unwrap()
                        .foreground,
                    ..styles.resolve("label", States::default()).unwrap()
                },
                Align::Left,
            );
            for i in 0..3 {
                paint::rect(
                    frame.paint,
                    Rect::new(
                        area.min[0] + 16.,
                        area.min[1] + 48. + i as f32 * 28.,
                        (area.width() - 32.) * (0.45 + i as f32 * 0.18),
                        18.,
                    ),
                    styles.colors().TRACKS[i],
                );
            }
        }
        actions
    }
}

struct NativeWindow {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    key: u64,
    panel: Option<DockId>,
    input: Input,
    frames: u32,
    dirty: bool,
}
struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    painter: Painter,
    format: wgpu::TextureFormat,
}
struct App {
    windows: HashMap<WindowId, NativeWindow>,
    gpu: Option<Gpu>,
    atlas: TextAtlas,
    host: PanelHost<Action>,
    workspace: Workspace,
    router: Router,
    model: Rc<RefCell<Model>>,
    language: Language,
    smoke: Option<PathBuf>,
    stage: u8,
    start: Instant,
    error: Option<String>,
    verified_undo: bool,
    style_path: Option<PathBuf>,
    style_status: Option<String>,
}
impl App {
    fn reload_style(&mut self) -> Result<(), String> {
        let path = self
            .style_path
            .as_ref()
            .ok_or("no --style file configured")?;
        let next = style::default_stylesheet().load_overrides(path)?;
        let atlas = TextAtlas::from_family(
            next.font_family(),
            next.base_font_size(),
            self.language == Language::Chinese,
        )?;
        self.host.set_stylesheet(next);
        self.atlas = atlas;
        self.style_status = None;
        for window in self.windows.values_mut() {
            window.dirty = true;
        }
        Ok(())
    }

    fn new(smoke: Option<PathBuf>, language: Language) -> Self {
        let model = Rc::new(RefCell::new(Model {
            value: 25.,
            text: "Motion Studio".into(),
            ..Default::default()
        }));
        let mut host = PanelHost::default();
        for (id, zh, en) in [
            (1, "组件浏览", "Components"),
            (2, "基础属性控件", "Property Widgets"),
            (3, "框架画布", "Framework Canvas"),
            (4, "布局与状态", "Layout and State"),
        ] {
            host.register(
                PanelDescriptor::new(DockId(id), zh, en),
                View {
                    id: DockId(id),
                    model: model.clone(),
                    number: Default::default(),
                    text: Default::default(),
                    undo: Default::default(),
                    show_icons: std::env::args().any(|arg| arg == "--icons"),
                },
            )
            .unwrap();
        }
        let atlas = TextAtlas::from_family(
            host.styles().font_family(),
            host.styles().base_font_size(),
            language == Language::Chinese,
        )
        .unwrap();
        Self {
            windows: Default::default(),
            gpu: None,
            atlas,
            host,
            workspace: {
                // The standalone gallery owns only its four registered panels;
                // application default docks must not leak into this example.
                let mut workspace = Workspace::default();
                let upper = dock::Node::Split {
                    edge: Edge::Left,
                    split: 0.2,
                    first: Box::new(dock::Node::dock(DockId(1))),
                    second: Box::new(dock::Node::Split {
                        edge: Edge::Right,
                        split: 0.28,
                        first: Box::new(dock::Node::dock(DockId(3))),
                        second: Box::new(dock::Node::dock(DockId(2))),
                    }),
                };
                workspace.root = dock::Node::Split {
                    edge: Edge::Top,
                    split: 0.72,
                    first: Box::new(upper),
                    second: Box::new(dock::Node::dock(DockId(4))),
                };
                workspace
            },
            router: Default::default(),
            model,
            language,
            smoke,
            stage: 0,
            start: Instant::now(),
            error: None,
            verified_undo: false,
            style_path: None,
            style_status: None,
        }
    }
    fn apply(&mut self, actions: Vec<Action>) {
        let changed = !actions.is_empty();
        let mut model = self.model.borrow_mut();
        for action in actions {
            match action {
                Action::Number(NumberEvent::Begin { value }) => model.transaction = Some(value),
                Action::Number(NumberEvent::Change { value }) => model.value = value,
                Action::Number(NumberEvent::Commit { value }) => {
                    if let Some(old) = model.transaction.take() {
                        if old != value {
                            model.undo.push(old);
                            model.commits += 1;
                        }
                    }
                    model.value = value;
                }
                Action::Number(NumberEvent::Cancel { value }) => {
                    model.transaction = None;
                    model.value = value;
                }
                Action::Text(text) => model.text = text,
                Action::Undo => {
                    if let Some(old) = model.undo.pop() {
                        model.value = old;
                    }
                }
            }
        }
        drop(model);
        if changed {
            for window in self.windows.values_mut() {
                window.dirty = true;
            }
        }
    }
    fn layouts(&self, window: &NativeWindow) -> Vec<PanelLayout> {
        let size = window.window.inner_size();
        let scale = window.window.scale_factor() as f32;
        let [w, h] = [size.width as f32 / scale, size.height as f32 / scale];
        if let Some(panel) = window.panel {
            vec![PanelLayout {
                window: window.key,
                panel,
                bounds: Rect::new(0., 26., w, (h - 26.).max(0.)),
                clip: Rect::new(0., 26., w, (h - 26.).max(0.)),
            }]
        } else {
            dock::solve(&self.workspace.root, Rect::new(0., 0., w, h))
                .into_iter()
                .map(|p| PanelLayout {
                    window: window.key,
                    panel: p.id,
                    bounds: Rect::new(
                        p.area.min[0],
                        p.area.min[1] + metrics::TAB_BAR,
                        p.area.width(),
                        (p.area.height() - metrics::TAB_BAR).max(0.),
                    ),
                    clip: Rect::new(
                        p.area.min[0],
                        p.area.min[1] + metrics::TAB_BAR,
                        p.area.width(),
                        (p.area.height() - metrics::TAB_BAR).max(0.),
                    ),
                })
                .collect()
        }
    }
    fn paint(&mut self, id: WindowId) -> (PaintList, [f32; 2], Option<DockId>) {
        let window = &self.windows[&id];
        let layouts = self.layouts(window);
        let input = window.input.clone();
        let key = window.key;
        let detached = window.panel;
        let size = window.window.inner_size();
        let scale = window.window.scale_factor() as f32;
        let size = [size.width as f32 / scale, size.height as f32 / scale];
        self.router
            .begin_frame(key, &input, &self.host.hit_regions(&layouts));
        let mut paint = PaintList::default();
        let mut actions = vec![];
        let mut float = None;
        for layout in layouts {
            let header = Rect::new(
                layout.bounds.min[0],
                layout.bounds.min[1] - metrics::TAB_BAR,
                layout.bounds.width(),
                metrics::TAB_BAR,
            );
            let header_style =
                self.host
                    .styles()
                    .resolve(
                        "panel.header",
                        if self.router.focus().is_some_and(|target| {
                            target.window == key && target.panel == layout.panel
                        }) {
                            States::default().with(State::Focused)
                        } else {
                            States::default()
                        },
                    )
                    .unwrap();
            style::box_style(&mut paint, header, header_style);
            let title = self
                .host
                .registry()
                .get(layout.panel)
                .unwrap()
                .title
                .get(self.language)
                .to_owned();
            style::text_style(
                &mut paint,
                &mut self.atlas,
                Rect::new(
                    header.min[0] + 8.,
                    header.min[1],
                    (header.width() - 72.).max(0.),
                    header.height(),
                ),
                &title,
                motion_ui::style::ResolvedStyle {
                    foreground: self
                        .host
                        .styles()
                        .resolve("panel.header", States::default())
                        .unwrap()
                        .foreground,
                    ..self
                        .host
                        .styles()
                        .resolve("panel.header", States::default())
                        .unwrap()
                },
                Align::Left,
            );
            if ui::button_styled(
                &mut paint,
                &mut self.atlas,
                Rect::new(header.max[0] - 64., header.min[1] + 3., 60., 20.),
                if detached.is_some() {
                    if self.language == Language::Chinese {
                        "停靠"
                    } else {
                        "Dock"
                    }
                } else {
                    "↗"
                },
                &input,
                true,
                self.host.styles(),
                "button",
            )
            .clicked
            {
                float = Some(layout.panel);
            }
            actions.extend(self.host.draw(
                layout,
                self.language,
                &input,
                &self.router,
                &mut paint,
                &mut self.atlas,
            ));
            let panel_style = self
                .host
                .styles()
                .resolve("panel", States::default())
                .unwrap();
            paint::stroke(
                &mut paint,
                layout.bounds,
                panel_style.border,
                panel_style.border_width,
            );
        }
        if let Some(message) = &self.style_status {
            style::text_style(
                &mut paint,
                &mut self.atlas,
                Rect::new(8., size[1] - 24., size[0] - 16., 24.),
                message,
                self.host
                    .styles()
                    .resolve("error", States::default())
                    .unwrap(),
                Align::Left,
            );
        }
        self.router.end_frame(&input);
        self.apply(actions);
        self.windows.get_mut(&id).unwrap().input.begin_frame();
        (paint, size, float)
    }
}
fn config(
    surface: &wgpu::Surface,
    adapter: &wgpu::Adapter,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::SurfaceConfiguration {
    let caps = surface.get_capabilities(adapter);
    wgpu::SurfaceConfiguration { color_space: wgpu::SurfaceColorSpace::Auto,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: width.max(1),
        height: height.max(1),
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    }
}
fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        multiview_mask: None,
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            depth_slice: None,
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let smoke = args
        .iter()
        .position(|a| a == "--smoke")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    if let Some(path) = &smoke {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
    }
    let language = if args.windows(2).any(|a| a[0] == "--locale" && a[1] == "en") {
        Language::English
    } else {
        Language::Chinese
    };
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(smoke, language);
    if let Some(index) = args.iter().position(|arg| arg == "--style") {
        let Some(path) = args.get(index + 1).filter(|p| !p.starts_with("--")) else {
            eprintln!("--style requires a JSON file");
            std::process::exit(1)
        };
        app.style_path = Some(PathBuf::from(path));
        if let Err(error) = app.reload_style() {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
    event_loop.run_app(&mut app).unwrap();
    if let Some(error) = app.error {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
