use super::*;
use motion_core::ProjectExpressions;

struct MainPresent<'a> {
    shell: &'a mut Shell,
    called: bool,
}
impl ApplicationHandler for MainPresent<'_> {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        self.draw(events);
    }
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        self.draw(events);
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: winit::event::WindowEvent) {}
}
impl MainPresent<'_> {
    fn draw(&mut self, events: &ActiveEventLoop) {
        if !self.called {
            self.called = true;
            self.shell.present(events).unwrap();
        }
    }
}
fn main_present(shell: &mut Shell, events: &mut EventLoop<()>, input: Input) {
    shell.input = input;
    shell.present_due = false;
    let mut app = MainPresent {
        shell,
        called: false,
    };
    events.pump_app_events(Some(Duration::ZERO), &mut app);
    assert!(app.called);
    if let Some(reply) = shell.rendering.remove(&0) {
        native_test_support::receive(events, &reply, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    }
    shell.input = Default::default();
    assert!(
        shell.actions.is_empty(),
        "reference gesture reached shell actions"
    );
    assert!(
        shell.workspace.drag_cursor().is_none(),
        "reference gesture armed background docking"
    );
}

fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn press(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
fn release(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        released: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn destination(shell: &Shell) -> [f32; 2] {
    center(
        shell
            .panels
            .expressions
            .triggers
            .iter()
            .find(|(target, _)| {
                *target
                    == ExpressionTarget::Property {
                        object: 8,
                        property: Property::Scale,
                        axis: None,
                    }
            })
            .unwrap()
            .1,
    )
}
fn assert_applied(shell: &Shell, expected: &str) {
    assert_eq!(shell.state["project"]["expressions"][0]["source"], expected);
    let project: motion_core::Project =
        serde_json::from_value(shell.state["project"].clone()).unwrap();
    assert_eq!(
        project.evaluated_at(0.).unwrap().layers[0]
            .transform
            .position
            .value,
        [150., 75., 100.]
    );
}
#[test]
#[ignore = "manual native GPU property reference gestures and menu captures"]
fn property_reference_native_menus_drag_drafts_and_float_transactions() {
    const CHILD: &str = "MOTION_EXPRESSION_REFERENCE_NATIVE_CHILD";
    const NAME: &str = "panels::expressions::expanded::tests::native::reference::property_reference_native_menus_drag_drafts_and_float_transactions";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_EXPRESSION_REFERENCE_ARTIFACTS")
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
    let (project_root, host, _, target) = super::super::picking::linked_fixture();
    drop(host);
    let mut shell = Shell::new(project_root.path().into(), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    shell.workspace.root = dock::Node::dock(Panel::Timeline.dock());
    shell.panels.selected = Some(7);
    shell.panels.focused = Some(Panel::Timeline);
    shell.panels.timeline.expanded.insert(7);
    shell.panels.timeline.expanded.insert(8);
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
    shell.window = Some(window);
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
    let expected = motion_core::expression_reference_source(&ExpressionTarget::Property {
        object: 8,
        property: Property::Scale,
        axis: None,
    })
    .unwrap();
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
            root.join(format!("{lang}-reference-menu.png")),
        );
        for _ in 0..3 {
            assert!(main_draw(&mut shell, key(Key::Down, false)).1.is_empty());
        }
        assert!(main_draw(&mut shell, key(Key::Enter, false)).1.is_empty());
        assert!(crate::panels::expressions::picking::active(
            &shell.panels,
            None
        ));
        let point = destination(&shell);
        assert!(main_draw(&mut shell, press(point)).1.is_empty());
        assert!(main_draw(&mut shell, release(point)).1.is_empty());
        assert_eq!(
            shell
                .panels
                .expressions
                .expanded
                .as_ref()
                .unwrap()
                .source
                .text,
            expected
        );
        let (paint, actions) = main_draw(&mut shell, Input::default());
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-reference-draft.png")),
        );
        main_draw(&mut shell, key(Key::A, true));
        let link = center(
            shell
                .panels
                .expressions
                .expanded
                .as_ref()
                .unwrap()
                .reference_button
                .unwrap(),
        );
        main_present(&mut shell, &mut events, press(link));
        let point = destination(&shell);
        let held = Input {
            mouse: point,
            held: Some(MouseButton::Left),
            ..Default::default()
        };
        main_present(&mut shell, &mut events, held.clone());
        let (paint, actions) = main_draw(&mut shell, held);
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-reference-drag.png")),
        );
        main_present(&mut shell, &mut events, release(point));
        assert_eq!(shell.state["project"], baseline["project"]);
        let mut actions = main_draw(&mut shell, key(Key::Enter, true)).1;
        assert_eq!(actions.len(), 1);
        shell.action(actions.pop().unwrap(), None).unwrap();
        poll(&mut shell);
        assert_applied(&shell, &expected);
        let (paint, _) = main_draw(&mut shell, Input::default());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join(format!("{lang}-reference-applied.png")),
        );
        main_draw(&mut shell, key(Key::Escape, false));
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], baseline["project"]);

        shell.workspace.root = dock::Node::dock(Panel::Composition.dock());
        shell
            .panels
            .expressions
            .open_expanded(&shell.state, target.clone(), Panel::Timeline);
        float_draw(&mut shell, &mut events, float_id, Input::default());
        float_draw(&mut shell, &mut events, float_id, key(Key::A, true));
        let link = center(
            shell
                .panels
                .expressions
                .expanded
                .as_ref()
                .unwrap()
                .reference_button
                .unwrap(),
        );
        float_draw(&mut shell, &mut events, float_id, press(link));
        let point = destination(&shell);
        let held = Input {
            mouse: point,
            held: Some(MouseButton::Left),
            ..Default::default()
        };
        float_draw(&mut shell, &mut events, float_id, held.clone());
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
            &held
        )
        .is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [600., 420.],
            1,
            root.join(format!("{lang}-reference-float-drag.png")),
        );
        float_draw(&mut shell, &mut events, float_id, release(point));
        assert_eq!(
            shell
                .panels
                .expressions
                .expanded
                .as_ref()
                .unwrap()
                .source
                .text,
            expected
        );
        assert_eq!(shell.state["project"], baseline["project"]);
        float_draw(&mut shell, &mut events, float_id, key(Key::Enter, true));
        assert_eq!(shell.actions.len(), 1);
        let action = shell.actions.pop_front().unwrap();
        shell.action(action, None).unwrap();
        poll(&mut shell);
        assert_applied(&shell, &expected);
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
            root.join(format!("{lang}-reference-float-applied.png")),
        );
        float_draw(&mut shell, &mut events, float_id, key(Key::Escape, false));
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], baseline["project"]);
    }
    shell.engine.save().unwrap();
    assert!(motion_core::storage::load(project_root.path())
        .unwrap()
        .expressions
        .is_empty());
    std::fs::write(root.join("results.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema_version":1,"locales":["zh","en"],"images":12,"main_and_native_float":true,
        "controlled_input":true,"foreground_os_ime_verified":false,"pixel_ratios":[1,1.5],
        "context_menu_pick":true,"link_button_drag":true,"draft_does_not_mutate_project":true,
        "native_float_apply_undo":true,"source_uses_decimal_string_ids":true,"cross_layer_reference":true,
        "shell_docking_interception":true,
    })).unwrap()).unwrap();
    shell.engine.close_window(1).unwrap();
    shell.floating.clear();
    shell.window = None;
    shell.engine.request_shutdown().unwrap();
}
