use super::*;
use crate::layer_clipboard::controller::Intent;

fn check(actions: &[panels::Action], intent: Intent, source: &Value) {
    let [panels::Action::CurveClipboard {
        intent: actual,
        source: stamp,
        selections,
    }] = actions
    else {
        panic!("expected one curve clipboard action: {actions:?}");
    };
    assert_eq!(*actual, intent);
    assert_eq!(stamp, &crate::layer_clipboard::KeyframeSource::of(source));
    assert_eq!(
        selections,
        &vec![motion_core::KeyframeSelection {
            target: motion_core::KeyframeTarget::Property {
                object: 7,
                property: motion_core::Property::Position,
                axis: None
            },
            frames: vec![0],
        }]
    );
}
fn click(rect: motion_ui::input::Rect) -> Input {
    let point = [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ];
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}

#[test]
#[ignore = "manual native GPU screenshots; injects production Input, not physical OS input"]
fn curve_clipboard_native_main_and_float_keep_explicit_targets_and_cancel_focus_loss() {
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CURVE_CLIPBOARD_GPU_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let _logging = crate::logging::init(root.join("logs")).unwrap();
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(
        7,
        "Easing study",
        [100.; 2],
        [100., 100., 0.],
        [0.25, 0.5, 0.8, 1.],
    );
    for (frame, x, opacity, ease) in [
        (0, 100., 0.2, Ease::InOut),
        (30, 300., 0.8, Ease::Out),
        (60, 150., 0.5, Ease::In),
    ] {
        layer
            .transform
            .position
            .upsert(frame, [x, x, 0.], ease)
            .unwrap();
        layer
            .transform
            .opacity
            .upsert(frame, opacity, ease)
            .unwrap();
    }
    project.layers.push(layer);
    motion_core::storage::save(&root.join("project"), &project).unwrap();
    let mut shell = Shell::new(root.join("project"), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    shell.workspace.root = dock::Node::Split {
        edge: dock::Edge::Left,
        split: 0.35,
        first: Box::new(dock::Node::dock(dock::Panel::Curves.dock())),
        second: Box::new(dock::Node::dock(dock::Panel::Timeline.dock())),
    };
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1600, 1000)),
            )
            .unwrap(),
    );
    let pending = shell
        .engine
        .attach_async(window.clone(), 1600, 1000)
        .unwrap();
    shell.state =
        crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
            .unwrap()
            .unwrap();
    shell.window = Some(window);
    shell.attached = true;
    shell.panels.select_only(Some(7));
    shell
        .panels
        .animation_controls
        .select_key(7, motion_core::Property::Position, None, 0);
    shell.panels.focused = Some(dock::Panel::Curves);
    let before = shell.engine.state().unwrap();
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1080, 1275)),
            )
            .unwrap(),
    );
    shell
        .engine
        .float_window(1, float.clone(), 1080, 1275)
        .unwrap();
    let id = float.id();
    shell.floating.insert(
        id,
        FloatWindow {
            window: float,
            key: 1,
            panel: dock::Panel::Curves,
            input: Default::default(),
            paint_input: Default::default(),
            pointer_position: Default::default(),
            scale: 1.5,
        },
    );
    for (locale, lang) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        shell.scale = 1.;
        shell.input = Input::default();
        let (_, actions) = shell.draw_commands([1600., 1000.]);
        assert!(actions.is_empty());
        let hits = shell
            .panels
            .animation_controls
            .clipboard_test_rects()
            .expect("mini easing controls missing");
        for (index, intent) in [(0, Intent::Copy), (1, Intent::Paste { in_place: true })] {
            shell.input = click(hits[index]);
            let (paint, actions) = shell.draw_commands([1600., 1000.]);
            check(&actions, intent, &shell.state);
            if index == 1 {
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    [1600., 1000.],
                    0,
                    root.join(format!("{lang}-main-clipboard.png")),
                );
            }
            for mode in 0..3 {
                shell.input = Input {
                    focus_lost: mode == 0,
                    composing: mode == 1,
                    ime_consumed: mode == 2,
                    ..click(hits[index])
                };
                assert!(shell.draw_commands([1600., 1000.]).1.is_empty());
            }
        }
        shell.input = Input::default();
        shell.panels.menu = Some(5);
        let (paint, actions) = shell.draw_commands([1600., 1000.]);
        assert!(actions.is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [1600., 1000.],
            0,
            root.join(format!("{lang}-animation-menu.png")),
        );
        shell.input = Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        };
        assert!(shell.draw_commands([1600., 1000.]).1.is_empty());
        assert!(shell.panels.menu.is_none());
        shell.input = Input::default();
        shell.present_due = false;
        let mut host = DrawFloat {
            shell: &mut shell,
            id,
            called: false,
        };
        let _ = events.pump_app_events(Some(Duration::from_millis(5)), &mut host);
        assert!(host.called);
        let mut paint = PaintList::default();
        paint.stylesheet = shell.panels.styles.clone();
        shell.atlas.set_pixel_ratio(1.5);
        assert!(panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            dock::Panel::Curves,
            [720., 850.],
            &mut shell.panels,
            &shell.state,
            &Input::default()
        )
        .is_empty());
        capture(
            &mut shell,
            &mut events,
            paint,
            [720., 850.],
            1,
            root.join(format!("{lang}-float-clipboard-ratio1.5.png")),
        );
        let hits = shell
            .panels
            .animation_controls
            .clipboard_test_rects()
            .unwrap();
        for (index, intent) in [(0, Intent::Copy), (1, Intent::Paste { in_place: true })] {
            let mut paint = PaintList::default();
            paint.stylesheet = shell.panels.styles.clone();
            let actions = panels::floating_chrome(
                &mut paint,
                &mut shell.atlas,
                dock::Panel::Curves,
                [720., 850.],
                &mut shell.panels,
                &shell.state,
                &click(hits[index]),
            );
            check(&actions, intent, &shell.state);
            for mode in 0..3 {
                let input = Input {
                    focus_lost: mode == 0,
                    composing: mode == 1,
                    ime_consumed: mode == 2,
                    ..click(hits[index])
                };
                assert!(panels::floating_chrome(
                    &mut paint,
                    &mut shell.atlas,
                    dock::Panel::Curves,
                    [720., 850.],
                    &mut shell.panels,
                    &shell.state,
                    &input
                )
                .is_empty());
            }
        }
    }
    let after = shell.engine.state().unwrap();
    for field in ["project", "history", "revision"] {
        assert_eq!(before[field], after[field]);
    }
    std::fs::write(root.join("result.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "screenshots":6, "nativeMainAndFloat":true, "locales":["zh","en"], "input":"production Input injection; physical OS events not tested",
        "routing":"copy/paste capture explicit source and outgoing interval", "focusLossAndImeCancel":true,
        "projectRevisionHistoryUnchanged":true, "systemClipboard":"verified separately in the process roundtrip test"
    })).unwrap()).unwrap();
    shell.engine.request_shutdown().unwrap();
}
