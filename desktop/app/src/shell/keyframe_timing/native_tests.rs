//! Hidden native windows and real GPU pixels; no shared OS foreground injection.
use super::*;
use motion_core::{Ease, KeyframeSelection, KeyframeTarget, Layer, Project, Property};
use motion_ui::input::Modifiers;
use winit::platform::{pump_events::EventLoopExtPumpEvents, windows::EventLoopBuilderExtWindows};

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn selection() -> Vec<KeyframeSelection> {
    vec![KeyframeSelection {
        target: KeyframeTarget::Property {
            object: 7,
            property: Property::Position,
            axis: None,
        },
        frames: vec![30],
    }]
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
    // This isolated capture overrides shell geometry; use the single-view frame protocol.
    ui.views = None;
    ui.composition = None;
    ui.composition_image = None;
    ui.node_preview = None;
    crate::native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    let image = image::open(path).unwrap();
    let ratio = if window == 0 {
        shell.scale
    } else {
        shell
            .floating
            .values()
            .find(|f| f.key == window)
            .unwrap()
            .scale
    };
    assert_eq!(
        (image.width(), image.height()),
        ((size[0] * ratio) as u32, (size[1] * ratio) as u32)
    );
}
fn main_draw(shell: &mut Shell, size: [f32; 2], input: Input) -> PaintList {
    shell.input = input;
    let (paint, actions) = shell.draw_commands(size);
    assert!(actions.is_empty(), "modal input reached background panels");
    shell.input = Input::default();
    paint
}
fn apply_pending(shell: &mut Shell, events: &mut EventLoop<()>) {
    assert_eq!(shell.actions.len(), 1);
    let action = shell.actions.pop_front().unwrap();
    shell.action(action, None).unwrap();
    let (_, pending) = shell.editing.take().unwrap();
    let state = crate::native_test_support::receive(events, &pending, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    shell.accept_edit_state(state);
    shell.present_due = false;
}
struct FloatDraw<'a> {
    shell: &'a mut Shell,
    id: WindowId,
    called: bool,
}
impl ApplicationHandler for FloatDraw<'_> {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        self.draw(events);
    }
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        self.draw(events);
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
impl FloatDraw<'_> {
    fn draw(&mut self, events: &ActiveEventLoop) {
        if !self.called {
            self.called = true;
            self.shell.present_float(self.id, events).unwrap();
        }
    }
}
fn float_draw(shell: &mut Shell, events: &mut EventLoop<()>, id: WindowId, input: Input) {
    shell.floating.get_mut(&id).unwrap().input = input;
    shell.present_due = false;
    let mut host = FloatDraw {
        shell,
        id,
        called: false,
    };
    events.pump_app_events(Some(Duration::ZERO), &mut host);
    assert!(host.called);
    if let Some(pending) = shell.rendering.remove(&1) {
        crate::native_test_support::receive(events, &pending, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    }
}

#[test]
#[ignore = "manual native GPU capture; does not inject OS input or monopolize desktop CI"]
fn timing_dialog_native_pixels_focus_transactions_and_independent_reopen() {
    const NAME: &str = "shell::keyframe_timing::native_tests::timing_dialog_native_pixels_focus_transactions_and_independent_reopen";
    const CHILD: &str = "MOTION_KEY_TIMING_GPU_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        if std::env::var_os("MOTION_KEY_TIMING_REOPEN").is_some() {
            let engine = crate::engine::Engine::start(
                root.join("project"),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
            let state = engine.state().unwrap();
            let expected: Project =
                serde_json::from_slice(&std::fs::read(root.join("saved-project.json")).unwrap())
                    .unwrap();
            let actual: Project = serde_json::from_value(state["project"].clone()).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(state["history"]["undoEntries"], 0);
            std::fs::write(
                root.join("reopened-state.json"),
                serde_json::to_vec_pretty(&state).unwrap(),
            )
            .unwrap();
            engine.request_shutdown().unwrap();
            return;
        }
        let mut project = Project::new(640, 360, 30, 120).unwrap();
        let mut layer = Layer::solid(
            7,
            "Timing study",
            [64.; 2],
            [100., 100., 0.],
            [0.2, 0.5, 0.8, 1.],
        );
        for frame in [0, 30, 60] {
            layer
                .transform
                .position
                .upsert(frame, [100. + frame as f32 * 2., 100., 0.], Ease::Linear)
                .unwrap();
        }
        project.layers.push(layer);
        motion_core::storage::save(&root.join("project"), &project).unwrap();
        let mut shell = Shell::new(root.join("project"), None, Some(Locale::Zh)).unwrap();
        shell.home = None;
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
        shell.state =
            crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
                .unwrap()
                .unwrap();
        shell.window = Some(window.clone());
        shell.attached = true;
        let baseline = shell.state.clone();
        shell.panels.select_only(Some(7));
        shell.panels.focused = Some(dock::Panel::Timeline);
        shell
            .panels
            .animation_controls
            .select_key(7, Property::Position, None, 30);
        shell.panels.menu = Some(5);
        let paint = main_draw(&mut shell, [900., 600.], Input::default());
        capture(
            &mut shell,
            &mut events,
            paint,
            [900., 600.],
            0,
            root.join("zh-animation-menu.png"),
        );
        shell.panels.menu = None;
        shell.panels.animation_menu_scope = None;
        // The actual default shortcut reaches a frozen modal action.
        shell.input = Input {
            keys: vec![(
                Key::K,
                Modifiers {
                    control: true,
                    alt: true,
                    shift: false,
                },
            )],
            ..Default::default()
        };
        let (_, mut actions) = shell.draw_commands([900., 600.]);
        shell.input = Input::default();
        assert_eq!(actions.len(), 1);
        shell.action(actions.remove(0), None).unwrap();
        assert_eq!(shell.dialog.as_ref().unwrap().timing_owner(), Some(0));
        for (locale, lang) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
            shell.workspace.locale = locale;
            shell.panels.locale = locale;
            for size in [[900., 600.], [1600., 1000.]] {
                for ratio in [1., 1.5] {
                    shell.scale = ratio;
                    let physical = [(size[0] * ratio) as u32, (size[1] * ratio) as u32];
                    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(
                        physical[0],
                        physical[1],
                    ));
                    let pending = shell
                        .engine
                        .resize_async(0, physical[0], physical[1])
                        .unwrap();
                    crate::native_test_support::receive(
                        &mut events,
                        &pending,
                        Duration::from_secs(30),
                    )
                    .unwrap()
                    .unwrap();
                    let paint = main_draw(&mut shell, size, Input::default());
                    capture(
                        &mut shell,
                        &mut events,
                        paint,
                        size,
                        0,
                        root.join(format!(
                            "{lang}-{}x{}-ratio{ratio}-dialog.png",
                            size[0] as u32, size[1] as u32
                        )),
                    );
                }
            }
        }
        shell.workspace.locale = Locale::Zh;
        shell.panels.locale = Locale::Zh;
        shell.scale = 1.;
        let size = [900., 600.];
        let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(900, 600));
        let pending = shell.engine.resize_async(0, 900, 600).unwrap();
        crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        main_draw(&mut shell, size, key(Key::Tab));
        main_draw(&mut shell, size, key(Key::Tab));
        let paint = main_draw(&mut shell, size, key(Key::Space));
        capture(
            &mut shell,
            &mut events,
            paint,
            size,
            0,
            root.join("zh-velocity-units.png"),
        );
        main_draw(&mut shell, size, key(Key::Escape));
        for _ in 0..2 {
            main_draw(&mut shell, size, key(Key::Tab));
        }
        main_draw(
            &mut shell,
            size,
            Input {
                text: "NaN".into(),
                ..Default::default()
            },
        );
        main_draw(&mut shell, size, key(Key::Enter));
        let paint = main_draw(&mut shell, size, key(Key::Enter));
        capture(
            &mut shell,
            &mut events,
            paint,
            size,
            0,
            root.join("zh-invalid-speed.png"),
        );
        assert_eq!(
            shell.engine.state().unwrap()["project"],
            baseline["project"]
        );
        main_draw(&mut shell, size, key(Key::Escape));
        assert!(shell.dialog.is_none());
        // Main-window Apply uses the real queue, one revision and one undo.
        shell
            .open_keyframe_timing(
                crate::layer_clipboard::KeyframeSource::of(&shell.state),
                selection(),
            )
            .unwrap();
        for _ in 0..4 {
            main_draw(&mut shell, size, key(Key::Tab));
        }
        let paint = main_draw(
            &mut shell,
            size,
            Input {
                text: "25".into(),
                ..Default::default()
            },
        );
        capture(
            &mut shell,
            &mut events,
            paint,
            size,
            0,
            root.join("zh-edit-speed.png"),
        );
        main_draw(&mut shell, size, key(Key::Enter));
        main_draw(&mut shell, size, key(Key::Enter));
        assert!(shell.dialog.is_none());
        apply_pending(&mut shell, &mut events);
        assert_eq!(shell.state["history"]["undoEntries"], 1);
        let saved: Project = serde_json::from_value(shell.state["project"].clone()).unwrap();
        shell.engine.save().unwrap();
        std::fs::write(
            root.join("saved-project.json"),
            serde_json::to_vec_pretty(&saved).unwrap(),
        )
        .unwrap();
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], baseline["project"]);
        assert_eq!(shell.state["canUndo"], false);
        shell.present_due = false;
        // Actual present_float runs with an ActiveEventLoop and its own input.
        #[allow(deprecated)]
        let floating = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1080, 780)),
                )
                .unwrap(),
        );
        shell
            .engine
            .float_window(1, floating.clone(), 1080, 780)
            .unwrap();
        let id = floating.id();
        shell.floating.insert(
            id,
            FloatWindow {
                window: floating,
                key: 1,
                panel: dock::Panel::Timeline,
                input: Default::default(),
                paint_input: Default::default(),
                pointer_position: Default::default(),
                scale: 1.5,
            },
        );
        shell.dialog = Some(
            dialogs::Dialog::keyframe_timing(
                &shell.state,
                crate::layer_clipboard::KeyframeSource::of(&shell.state),
                selection(),
                1,
            )
            .unwrap(),
        );
        let paint = main_draw(&mut shell, size, key(Key::Escape));
        assert!(
            shell.dialog.is_some(),
            "main window consumed floating dialog Escape"
        );
        capture(
            &mut shell,
            &mut events,
            paint,
            size,
            0,
            root.join("zh-main-blocked-by-float.png"),
        );
        float_draw(&mut shell, &mut events, id, Input::default());
        // Capture the float paint with the same renderer and modal implementation.
        let mut paint = PaintList::default();
        paint.stylesheet = shell.panels.styles.clone();
        shell.atlas.set_pixel_ratio(1.5);
        panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            dock::Panel::Timeline,
            [720., 520.],
            &mut shell.panels,
            &shell.state,
            &Input::default(),
        );
        shell.dialog.as_mut().unwrap().render(
            &mut paint,
            &mut shell.atlas,
            [720., 520.],
            &Input::default(),
            Locale::Zh,
        );
        capture(
            &mut shell,
            &mut events,
            paint,
            [720., 520.],
            1,
            root.join("zh-float-dialog-ratio1.5.png"),
        );
        for _ in 0..4 {
            float_draw(&mut shell, &mut events, id, key(Key::Tab));
        }
        float_draw(
            &mut shell,
            &mut events,
            id,
            Input {
                text: "15".into(),
                ..Default::default()
            },
        );
        float_draw(&mut shell, &mut events, id, key(Key::Enter));
        assert!(shell.dialog.is_some());
        float_draw(&mut shell, &mut events, id, key(Key::Enter));
        assert!(shell.dialog.is_none());
        apply_pending(&mut shell, &mut events);
        assert_eq!(shell.state["history"]["undoEntries"], 1);
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], baseline["project"]);
        // A known stale draft is cancelled locally without submitting another edit.
        let frozen = crate::layer_clipboard::KeyframeSource::of(&shell.state);
        let selected = selection();
        shell.state = shell
            .engine
            .edit(
                serde_json::to_string(&motion_core::Command::SetScalar {
                    object: 7,
                    property: Property::Opacity,
                    frame: 0,
                    value: 0.8,
                })
                .unwrap(),
            )
            .unwrap();
        shell
            .timing_dialog_event(dialogs::Event::KeyframeTiming {
                source: frozen,
                selections: selected,
                settings: motion_core::timing::TimingSettings {
                    interpolation: motion_core::timing::TimingInterpolation::Hold,
                    units: Default::default(),
                    incoming: None,
                    outgoing: Some(Default::default()),
                    match_speed: false,
                },
            })
            .unwrap();
        assert!(shell.actions.is_empty());
        assert!(shell.panels.message.as_ref().unwrap().contains("未应用"));
        shell.engine.close_window(1).unwrap();
        shell.engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_KEY_TIMING_GPU_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    for (phase, reopen) in [("capture", false), ("reopen", true)] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG");
        if reopen {
            command.env("MOTION_KEY_TIMING_REOPEN", "1");
        }
        let output = command.output().unwrap();
        std::fs::write(root.join(format!("{phase}-stdout.log")), &output.stdout).unwrap();
        std::fs::write(root.join(format!("{phase}-stderr.log")), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{phase}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
