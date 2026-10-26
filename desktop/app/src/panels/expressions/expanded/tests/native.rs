//! Actual native GPU captures; input is controlled through the production shell.
//! This does not inject foreground OS IME events.
use super::*;
use crate::{native_test_support, FloatWindow, Shell};
use std::{path::PathBuf, sync::Arc};
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::{pump_events::EventLoopExtPumpEvents, windows::EventLoopBuilderExtWindows},
    window::{Window, WindowId},
};

fn main_draw(shell: &mut Shell, input: Input) -> (PaintList, Vec<Action>) {
    shell.input = input;
    let result = shell.draw_commands([900., 600.]);
    shell.input = Default::default();
    result
}
fn capture(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    paint: PaintList,
    size: [f32; 2],
    window: u64,
    path: PathBuf,
) {
    let mut ui = shell.ui_frame(paint, size, true);
    ui.window = window;
    ui.composition = None;
    ui.composition_image = None;
    ui.node_preview = None;
    native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    let scale = if window == 0 { shell.scale } else { 1.5 };
    assert_eq!(
        image::open(&path).unwrap().to_rgba8().dimensions(),
        ((size[0] * scale) as u32, (size[1] * scale) as u32)
    );
    assert!(image::open(&path)
        .unwrap()
        .to_rgba8()
        .pixels()
        .any(|p| p[0] > 150));
}
fn poll(shell: &mut Shell) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while shell.expression_task.is_some() {
        assert!(std::time::Instant::now() < deadline);
        shell.poll_expression_task();
        std::thread::yield_now();
    }
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
fn float_draw(shell: &mut Shell, events: &mut EventLoop<()>, id: WindowId, input: Input) {
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
}
#[test]
#[ignore = "manual native GPU expression editor captures and input routing"]
fn expression_editor_native_drafts_menus_diagnostics_and_floating_input() {
    const CHILD: &str = "MOTION_EXPRESSION_NATIVE_CHILD";
    const NAME:&str="panels::expressions::expanded::tests::native::expression_editor_native_drafts_menus_diagnostics_and_floating_input";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_EXPRESSION_NATIVE_ARTIFACTS")
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
    let (project_root, host, _, target) = fixture();
    drop(host);
    let mut shell = Shell::new(project_root.path().into(), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    shell.workspace.root = dock::Node::dock(Panel::Timeline.dock());
    shell.panels.selected = Some(7);
    shell.panels.focused = Some(Panel::Timeline);
    shell.panels.timeline.expanded.insert(7);
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
    let pending = shell.engine.attach_async(window.clone(), 900, 600).unwrap();
    shell.state = native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    shell.window = Some(window.clone());
    shell.attached = true;
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(900, 630)),
            )
            .unwrap(),
    );
    shell
        .engine
        .float_window(1, float.clone(), 900, 630)
        .unwrap();
    let float_id = float.id();
    shell.floating.insert(
        float_id,
        FloatWindow {
            window: float,
            key: 1,
            panel: Panel::Timeline,
            input: Default::default(),
            paint_input: Default::default(),
            pointer_position: Default::default(),
            scale: 1.5,
        },
    );
    let baseline = shell.state.clone();
    for (locale, lang) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        shell.workspace.root = dock::Node::dock(Panel::Timeline.dock());
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        shell.scale = 1.;
        shell.atlas.set_pixel_ratio(1.);
        shell.panels.expressions.cancel();
        main_draw(&mut shell, Input::default());
        let trigger = shell
            .panels
            .expressions
            .triggers
            .iter()
            .find(|(t, _)| *t == target)
            .unwrap()
            .1;
        let mut right = click(trigger);
        right.pressed = right.pressed.map(|(_, p)| (MouseButton::Right, p));
        let (paint, actions) = main_draw(&mut shell, right);
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-property-context.png")),
        );
        // The appended command is reached through the actual context menu.
        // Remove is disabled for an absent expression and is skipped by navigation.
        for _ in 0..2 {
            assert!(main_draw(&mut shell, key(Key::Down, false)).1.is_empty());
        }
        assert!(main_draw(&mut shell, key(Key::Enter, false)).1.is_empty());
        assert!(shell.panels.expressions.expanded.is_some());
        main_draw(&mut shell, Input::default());
        main_draw(&mut shell, key(Key::A, true));
        let code =
            "const speed = 48;\nconst drift = [Math.sin(time) * speed, 0, 0];\nvalue + drift";
        let (paint, actions) = main_draw(
            &mut shell,
            Input {
                text: code.into(),
                ..Default::default()
            },
        );
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-multiline-draft.png")),
        );
        for (size, scale, suffix) in [
            ([1600., 1000.], 1., "default"),
            ([900., 600.], 2., "ratio2"),
        ] {
            let physical = [(size[0] * scale) as u32, (size[1] * scale) as u32];
            let _ =
                window.request_inner_size(winit::dpi::PhysicalSize::new(physical[0], physical[1]));
            let receive = shell
                .engine
                .resize_async(0, physical[0], physical[1])
                .unwrap();
            native_test_support::receive(&mut events, &receive, Duration::from_secs(30))
                .unwrap()
                .unwrap();
            shell.scale = scale;
            let (paint, actions) = shell.draw_commands(size);
            assert!(actions.is_empty());
            capture(
                &mut shell,
                &mut events,
                paint,
                size,
                0,
                root.join(format!("{lang}-{suffix}.png")),
            );
        }
        let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(900, 600));
        let receive = shell.engine.resize_async(0, 900, 600).unwrap();
        native_test_support::receive(&mut events, &receive, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        shell.scale = 1.;
        main_draw(&mut shell, Input::default());
        main_draw(&mut shell, key(Key::A, true));
        let edit_menu = crate::panels::menus::regions(&mut shell.atlas, locale)[1].1;
        let (paint, actions) = main_draw(&mut shell, click(edit_menu));
        assert!(actions.is_empty());
        assert!(shell.panels.text_menu.is_some());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-text-edit-menu.png")),
        );
        let copy = Rect::new(edit_menu.min[0] + 4., 48. + 4. + 2. * 32., 100., 30.);
        assert!(main_draw(&mut shell, click(copy)).1.is_empty());
        assert_eq!(shell.text_clipboard.read().unwrap().as_deref(), Some(code));
        assert_eq!(
            shell
                .panels
                .expressions
                .expanded
                .as_ref()
                .unwrap()
                .source
                .text,
            code
        );
        let areas = shell
            .panels
            .expressions
            .expanded
            .as_ref()
            .unwrap()
            .areas
            .unwrap();
        let (paint, actions) = main_draw(&mut shell, click(areas[2]));
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-snippet-menu.png")),
        );
        main_draw(&mut shell, key(Key::Escape, false));
        assert!(shell.panels.expressions.expanded.is_some());
        main_draw(&mut shell, key(Key::A, true));
        main_draw(
            &mut shell,
            Input {
                text: "const 名称 = 1;\nvalue + )".into(),
                ..Default::default()
            },
        );
        let (_, mut actions) = main_draw(&mut shell, click(areas[3]));
        assert_eq!(actions.len(), 1);
        shell.action(actions.pop().unwrap(), None).unwrap();
        poll(&mut shell);
        let (paint, actions) = main_draw(&mut shell, Input::default());
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-syntax-diagnostic.png")),
        );
        assert_eq!(shell.state["project"], baseline["project"]);
        // Main and floating windows keep the same property-owned draft. Actual
        // floating shell input must insert Space/Delete, never play/delete a layer.
        shell
            .panels
            .expressions
            .open_expanded(&shell.state, target.clone(), Panel::Timeline);
        shell.workspace.root = dock::Node::dock(Panel::Composition.dock());
        float_draw(&mut shell, &mut events, float_id, Input::default());
        float_draw(&mut shell, &mut events, float_id, key(Key::A, true));
        float_draw(
            &mut shell,
            &mut events,
            float_id,
            Input {
                text: code.into(),
                ..Default::default()
            },
        );
        float_draw(
            &mut shell,
            &mut events,
            float_id,
            Input {
                text: " ".into(),
                keys: vec![(Key::Space, Default::default())],
                ..Default::default()
            },
        );
        assert!(shell.actions.is_empty());
        assert!(shell.playback.is_none());
        assert!(shell
            .panels
            .expressions
            .expanded
            .as_ref()
            .unwrap()
            .source
            .text
            .ends_with(' '));
        float_draw(&mut shell, &mut events, float_id, key(Key::Home, false));
        float_draw(&mut shell, &mut events, float_id, key(Key::Delete, false));
        assert!(shell
            .panels
            .expressions
            .expanded
            .as_ref()
            .unwrap()
            .source
            .text
            .contains("alue + drift"));
        float_draw(&mut shell, &mut events, float_id, key(Key::Z, true));
        assert!(shell
            .panels
            .expressions
            .expanded
            .as_ref()
            .unwrap()
            .source
            .text
            .contains("value + drift"));
        float_draw(&mut shell, &mut events, float_id, key(Key::Enter, true));
        assert_eq!(shell.actions.len(), 1);
        let action = shell.actions.pop_front().unwrap();
        shell.action(action, None).unwrap();
        poll(&mut shell);
        assert!(shell.state["project"]["expressions"][0]["source"]
            .as_str()
            .unwrap()
            .contains("value + drift"));
        shell.atlas.set_pixel_ratio(1.5);
        let mut paint = PaintList::default();
        paint.stylesheet = shell.panels.styles.clone();
        assert!(floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Timeline,
            [600., 420.],
            &mut shell.panels,
            &shell.state,
            &Input::default()
        )
        .is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [600., 420.],
            1,
            root.join(format!("{lang}-floating-ratio1.5.png")),
        );
        float_draw(&mut shell, &mut events, float_id, key(Key::Escape, false));
        assert!(shell.panels.expressions.expanded.is_none());
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], baseline["project"]);
    }
    shell.engine.save().unwrap();
    assert_eq!(
        motion_core::storage::load(project_root.path())
            .unwrap()
            .expressions
            .len(),
        0
    );
    std::fs::write(root.join("results.json"),serde_json::to_vec_pretty(&serde_json::json!({"schema_version":1,
        "main_and_native_float":true,"locales":["zh","en"],"images":16,"sizes":[[900,600],[1600,1000],[600,420]],"pixel_ratios":[1,1.5,2],"draft_and_check_did_not_mutate_project":true,
        "controlled_input":true,"native_float_apply_undo":true,"foreground_os_ime_verified":false})).unwrap()).unwrap();
    shell.engine.close_window(1).unwrap();
    shell.floating.clear();
    shell.window = None;
    shell.engine.request_shutdown().unwrap();
}
