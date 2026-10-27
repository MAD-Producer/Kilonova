//! Hidden native windows, real GPU captures and production Shell routing.
//! Controlled Input is not a claim of foreground OS keyboard/IME acceptance.
use super::*;
use crate::{native_test_support, FloatWindow, Shell};
use std::{sync::Arc, time::Duration};
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::{pump_events::EventLoopExtPumpEvents, windows::EventLoopBuilderExtWindows},
    window::{Window, WindowId},
};

fn main_draw(shell: &mut Shell, size: [f32; 2], input: Input) -> (PaintList, Vec<Action>) {
    shell.input = input;
    let result = shell.draw_commands(size);
    shell.input = Input::default();
    result
}
struct FloatDraw<'a> {
    shell: &'a mut Shell,
    id: WindowId,
    called: bool,
}
impl ApplicationHandler for FloatDraw<'_> {
    fn resumed(&mut self, e: &ActiveEventLoop) {
        self.draw(e);
    }
    fn about_to_wait(&mut self, e: &ActiveEventLoop) {
        self.draw(e);
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: winit::event::WindowEvent) {}
}
impl FloatDraw<'_> {
    fn draw(&mut self, e: &ActiveEventLoop) {
        if !self.called {
            self.called = true;
            self.shell.present_float(self.id, e).unwrap();
        }
    }
}
fn float_draw(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    id: WindowId,
    input: Input,
) -> Vec<Action> {
    shell.floating.get_mut(&id).unwrap().input = input;
    shell.present_due = false;
    let mut app = FloatDraw {
        shell,
        id,
        called: false,
    };
    events.pump_app_events(Some(Duration::ZERO), &mut app);
    assert!(app.called);
    if let Some(reply) = shell.rendering.remove(&1) {
        native_test_support::receive(events, &reply, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    }
    std::mem::take(&mut shell.actions).into_iter().collect()
}
fn capture(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    paint: PaintList,
    size: [f32; 2],
    window: u64,
    path: PathBuf,
    scale: f32,
) {
    let mut ui = shell.ui_frame(paint, size, true);
    ui.window = window;
    if window != 0 {
        ui.composition = None;
        ui.composition_image = None;
        ui.node_preview = None;
    }
    if ui.composition.is_some() {
        // First use can report normal asynchronous image decode pending. Poll
        // production Present until it commits; never capture its empty texture.
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let reply = shell
                .engine
                .present_async(crate::engine::UiFrame {
                    paint: PaintList::default(),
                    atlas: None,
                    logical_size: ui.logical_size,
                    composition: ui.composition,
                    composition_image: ui.composition_image,
                    window: ui.window,
                    node_preview: None,
                })
                .unwrap();
            if native_test_support::receive(events, &reply, Duration::from_secs(30))
                .unwrap()
                .unwrap()
            {
                break;
            }
            let current = shell.engine.state().unwrap();
            assert!(current["renderError"].is_null(), "{current}");
            assert!(
                std::time::Instant::now() < deadline,
                "text presentation never became ready: {current}"
            );
        }
    }
    let viewport = ui.composition;
    native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    let image = image::open(path).unwrap().to_rgba8();
    assert_eq!(
        image.dimensions(),
        ((size[0] * scale) as u32, (size[1] * scale) as u32)
    );
    assert!(image.pixels().any(|p| p[0] > 150));
    if let Some([x, y, w, h]) = viewport {
        let white = image
            .enumerate_pixels()
            .filter(|(px, py, p)| {
                (*px as f32) >= x
                    && (*px as f32) < x + w
                    && (*py as f32) >= y
                    && (*py as f32) < y + h
                    && p[0] > 160
                    && p[0].abs_diff(p[1]) < 10
                    && p[1].abs_diff(p[2]) < 10
            })
            .count();
        assert!(
            white > 10,
            "composition must contain actual rendered text, white pixels={white}"
        );
    }
}
fn panel_root(panel: Panel, width: f32) -> dock::Node {
    dock::Node::Split {
        edge: dock::Edge::Left,
        split: (width - 300.) / width,
        first: Box::new(dock::Node::dock(Panel::Composition.dock())),
        second: Box::new(dock::Node::dock(panel.dock())),
    }
}
fn resize(shell: &mut Shell, events: &mut EventLoop<()>, size: [f32; 2], scale: f32) {
    let physical =
        winit::dpi::PhysicalSize::new((size[0] * scale) as u32, (size[1] * scale) as u32);
    let _ = shell.window.as_ref().unwrap().request_inner_size(physical);
    let reply = shell
        .engine
        .resize_async(0, physical.width, physical.height)
        .unwrap();
    shell.state = native_test_support::receive(events, &reply, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    shell.scale = scale;
    shell.atlas.set_pixel_ratio(scale);
}
fn poll(shell: &mut Shell) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while shell.text_style_task.is_some() {
        assert!(std::time::Instant::now() < deadline);
        shell.poll_text_layout();
        std::thread::yield_now();
    }
}
#[test]
#[ignore = "manual native GPU character/paragraph captures and window routing"]
fn text_style_native_controls_pixels_transactions_and_floating_input() {
    const CHILD: &str = "MOTION_TEXT_STYLE_NATIVE_CHILD";
    const NAME:&str="panels::auxiliary::text_style::tests::native::text_style_native_controls_pixels_transactions_and_floating_input";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_TEXT_STYLE_NATIVE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.join("logs")).unwrap();
    motion_core::storage::save(
        &root.join("project"),
        &motion_core::Project::new(640, 360, 30, 2).unwrap(),
    )
    .unwrap();
    let mut shell = Shell::new(root.join("project"), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    let font = PathBuf::from(std::env::var_os("WINDIR").expect("Windows directory"))
        .join("Fonts/msyh.ttc");
    assert!(
        font.is_file(),
        "this manual CJK fixture needs the installed Microsoft YaHei font"
    );
    shell.state = shell
        .engine
        .create_text("中文标题\nMotion Studio".into(), 40., 420, Some((font, 0)))
        .unwrap();
    let object = shell.state["project"]["layers"][0]["id"].as_u64().unwrap();
    shell.panels.selected = Some(object);
    shell.panels.focused = Some(Panel::Character);
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(900, 600)),
            )
            .unwrap(),
    );
    let reply = shell.engine.attach_async(window.clone(), 900, 600).unwrap();
    shell.state = native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    shell.window = Some(window);
    shell.attached = true;
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(450, 690)),
            )
            .unwrap(),
    );
    shell
        .engine
        .float_window(1, float.clone(), 450, 690)
        .unwrap();
    let float_id = float.id();
    shell.floating.insert(
        float_id,
        FloatWindow {
            window: float,
            key: 1,
            panel: Panel::Character,
            input: Input::default(),
            paint_input: Input::default(),
            pointer_position: Default::default(),
            scale: 1.5,
        },
    );
    let baseline = shell.state["project"].clone();
    for (locale, lang) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        shell.scale = 1.;
        shell.atlas.set_pixel_ratio(1.);
        for (size, suffix, scale) in [
            ([900., 600.], "min", 1.),
            ([1600., 1000.], "default", 1.5),
            ([900., 600.], "dpi2", 2.),
        ] {
            resize(&mut shell, &mut events, size, scale);
            for (panel, name) in [
                (Panel::Character, "character"),
                (Panel::Paragraph, "paragraph"),
            ] {
                shell.workspace.root = panel_root(panel, size[0]);
                shell.panels.focused = Some(panel);
                let (paint, actions) = main_draw(&mut shell, size, Input::default());
                assert!(actions.is_empty());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    0,
                    root.join(format!("{lang}-{name}-{suffix}.png")),
                    scale,
                );
            }
        }
        resize(&mut shell, &mut events, [900., 600.], 1.);
        shell.workspace.root = panel_root(Panel::Character, 900.);
        shell.panels.focused = Some(Panel::Character);
        main_draw(&mut shell, [900., 600.], Input::default());
        let rect = shell.panels.auxiliary.text_style.font_rect.unwrap();
        let (paint, actions) = main_draw(&mut shell, [900., 600.], press(rect));
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-font-menu.png")),
            1.,
        );
        assert!(main_draw(&mut shell, [900., 600.], key(Key::Escape))
            .1
            .is_empty());
        let rect = shell.panels.auxiliary.text_style.fields[0];
        let point = press(rect).mouse;
        for input in [
            press(rect),
            Input {
                mouse: point,
                released: Some((MouseButton::Left, point)),
                ..Default::default()
            },
            Input {
                keys: vec![(
                    Key::A,
                    Modifiers {
                        control: true,
                        ..Default::default()
                    },
                )],
                ..Default::default()
            },
            Input {
                text: "48".into(),
                ..Default::default()
            },
            key(Key::Enter),
        ] {
            assert!(main_draw(&mut shell, [900., 600.], input).1.is_empty());
        }
        let before = shell.state["project"].clone();
        let rect = shell.panels.auxiliary.text_style.apply_rect.unwrap();
        let (_, actions) = main_draw(&mut shell, [900., 600.], press(rect));
        assert!(matches!(actions.as_slice(), [Action::TextApply { .. }]));
        for action in actions {
            shell.action(action, None).unwrap();
        }
        poll(&mut shell);
        assert_eq!(
            shell.state["project"]["layers"][0]["content"]["style"]["font_size"],
            48.
        );
        let (paint, actions) = main_draw(&mut shell, [900., 600.], Input::default());
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-applied.png")),
            1.,
        );
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], before);
        // Choosing the font without CJK glyphs must report its actual error,
        // preserve the rendered text and allow cancellation of the failed draft.
        main_draw(&mut shell, [900., 600.], Input::default());
        let rect = shell.panels.auxiliary.text_style.font_rect.unwrap();
        for input in [press(rect), key(Key::Down), key(Key::Enter)] {
            assert!(main_draw(&mut shell, [900., 600.], input).1.is_empty());
        }
        let rect = shell.panels.auxiliary.text_style.apply_rect.unwrap();
        let actions = main_draw(&mut shell, [900., 600.], press(rect)).1;
        assert!(matches!(actions.as_slice(), [Action::TextApply { .. }]));
        for action in actions {
            shell.action(action, None).unwrap();
        }
        poll(&mut shell);
        assert_eq!(shell.state["project"], before);
        assert!(shell
            .panels
            .auxiliary
            .text_style
            .draft
            .as_ref()
            .unwrap()
            .message
            .is_some());
        let (paint, actions) = main_draw(&mut shell, [900., 600.], Input::default());
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-font-error.png")),
            1.,
        );
        let rect = shell.panels.auxiliary.text_style.cancel_rect.unwrap();
        assert!(main_draw(&mut shell, [900., 600.], press(rect))
            .1
            .is_empty());
        assert!(shell.panels.auxiliary.text_style.draft.is_none());
        // Use the actual native floating-window presentation and action queue.
        shell.workspace.root = dock::Node::dock(Panel::Composition.dock());
        assert!(float_draw(&mut shell, &mut events, float_id, Input::default()).is_empty());
        let rect = shell.panels.auxiliary.text_style.fields[0];
        let point = press(rect).mouse;
        for input in [
            press(rect),
            Input {
                mouse: point,
                released: Some((MouseButton::Left, point)),
                ..Default::default()
            },
            Input {
                keys: vec![(
                    Key::A,
                    Modifiers {
                        control: true,
                        ..Default::default()
                    },
                )],
                ..Default::default()
            },
            Input {
                text: "52".into(),
                ..Default::default()
            },
            key(Key::Enter),
        ] {
            assert!(float_draw(&mut shell, &mut events, float_id, input).is_empty());
        }
        let rect = shell.panels.auxiliary.text_style.apply_rect.unwrap();
        let actions = float_draw(&mut shell, &mut events, float_id, press(rect));
        assert!(matches!(actions.as_slice(), [Action::TextApply { .. }]));
        for action in actions {
            shell.action(action, None).unwrap();
        }
        poll(&mut shell);
        assert_eq!(
            shell.state["project"]["layers"][0]["content"]["style"]["font_size"],
            52.
        );
        assert_eq!(
            shell.panels.message.as_deref(),
            Some(if locale == Locale::Zh {
                "文字排版已应用"
            } else {
                "Text layout applied"
            })
        );
        let mut paint = PaintList::default();
        crate::panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Character,
            [300., 460.],
            &mut shell.panels,
            &shell.state,
            &Input::default(),
        );
        capture(
            &mut shell,
            &mut events,
            paint,
            [300., 460.],
            1,
            root.join(format!("{lang}-float-applied.png")),
            1.5,
        );
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], baseline);
    }
    // Exercise the styled layout path with real CJK glyph metrics as well as
    // the portable IBM Plex Mono export test; the unstyled path is separate.
    let content = &shell.state["project"]["layers"][0]["content"];
    let mut style: motion_core::TextStyle =
        serde_json::from_value(content["style"].clone()).unwrap();
    style.font_size = 48.;
    style.line_height = Some(76.);
    style.tracking = 3.;
    style.stroke_width = 1.5;
    style.alignment = motion_core::TextAlign::Center;
    style.indent_left = 12.;
    style.indent_right = 12.;
    style.indent_first = 16.;
    let request = serde_json::json!({"op":"font_edit_text","object":object,"revision":shell.state["revision"],
        "text":content["text"],"font":content["font"],"style":style});
    let source = crate::layer_clipboard::KeyframeSource::of(&shell.state);
    let reply = shell
        .engine
        .request(|reply| crate::engine::Command::TextLayout {
            source,
            request,
            reply,
        })
        .unwrap();
    shell.state = native_test_support::receive(&mut events, &reply, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    assert_eq!(
        shell.state["project"]["layers"][0]["content"]["style"],
        serde_json::to_value(&style).unwrap()
    );
    shell.engine.save().unwrap();
    let output = root.join("zh-styled-output.png");
    let reply = shell
        .engine
        .capture_async(output.clone(), Default::default())
        .unwrap();
    let result = native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    assert_eq!(
        (result["width"].as_u64(), result["height"].as_u64()),
        (Some(640), Some(360))
    );
    let pixels = image::open(output).unwrap().to_rgba8();
    assert!(pixels.pixels().filter(|p| p[0] > 160 && p[3] > 160).count() > 200);
    shell.panels.locale = Locale::Zh;
    shell.workspace.locale = Locale::Zh;
    resize(&mut shell, &mut events, [900., 600.], 1.);
    for (panel, name) in [
        (Panel::Character, "character"),
        (Panel::Paragraph, "paragraph"),
    ] {
        shell.workspace.root = panel_root(panel, 900.);
        shell.panels.focused = Some(panel);
        let (paint, actions) = main_draw(&mut shell, [900., 600.], Input::default());
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("zh-styled-{name}.png")),
            1.,
        );
    }
    shell.engine.close_window(1).unwrap();
    shell.floating.clear();
    shell.attached = false;
    shell.window = None;
}
