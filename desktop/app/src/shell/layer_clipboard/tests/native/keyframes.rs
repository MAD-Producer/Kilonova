//! Real HWND/wgpu screenshots, component keyboard/menu input, shared Shell cache.
use super::*;
use motion_core::Property;

fn execute_keys(shell: &mut Shell, actions: Vec<Action>, intent: Intent) {
    assert!(
        matches!(actions.as_slice(), [Action::KeyframeClipboard { intent: actual, .. }] if *actual == intent),
        "{actions:?}"
    );
    for action in actions {
        shell.action(action, None).unwrap();
    }
    wait(shell);
}

#[test]
fn native_keyframe_clipboard_menu_keyboard_and_floating_timeline_share_real_transactions() {
    const NAME: &str = "shell::layer_clipboard::tests::native::keyframes::native_keyframe_clipboard_menu_keyboard_and_floating_timeline_share_real_transactions";
    const CHILD: &str = "MOTION_KEYFRAME_CLIPBOARD_UI_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_KEYFRAME_CLIPBOARD_UI_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
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
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let directory = root.join("project");
    project(&directory, false);
    let mut project = motion_core::storage::load(&directory).unwrap();
    project.layers[0]
        .transform
        .position
        .set_animated(0, true)
        .unwrap();
    project.layers[0]
        .transform
        .position
        .set_at(5, [10.; 3])
        .unwrap();
    let mut destination = motion_core::Layer::solid(9, "Destination", [32.; 2], [32.; 3], [1.; 4]);
    destination.timeline = Some(motion_core::LayerTimeline {
        in_frame: 30,
        out_frame: 90,
        offset_frame: 30,
    });
    project.layers.push(destination);
    motion_core::storage::save(&directory, &project).unwrap();
    let mut shell = shell(&directory, Locale::En);
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(900, 600)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1350, 900)),
            )
            .unwrap(),
    );
    let attach = shell.engine.attach_async(main.clone(), 900, 600).unwrap();
    shell.state = native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    shell.window = Some(main.clone());
    shell
        .engine
        .float_window(1, float.clone(), 1350, 900)
        .unwrap();
    for (window, size, scale, locale, graph, name) in [
        (0, [900., 600.], 1., Locale::En, false, "main-en-single"),
        (0, [1600., 1000.], 1.5, Locale::Zh, true, "main-zh-curve"),
        (1, [900., 600.], 1.5, Locale::En, false, "float-en-single"),
        (1, [1600., 1000.], 1., Locale::Zh, true, "float-zh-curve"),
    ] {
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        shell.panels.menu = None;
        shell.panels.clipboard_menu_allowed = None;
        shell.panels.clipboard_menu_keys = None;
        shell.panels.focused = Some(Panel::Timeline);
        shell.panels.select_only(Some(7));
        shell.panels.timeline.graph_mode = graph;
        if graph {
            shell
                .panels
                .animation_controls
                .choose_track(7, Property::Position, None);
        } else {
            shell
                .panels
                .animation_controls
                .select_key(7, Property::Position, None, 10);
        }
        shell.scale = scale;
        shell.atlas.set_pixel_ratio(scale);
        let physical = [(size[0] * scale) as u32, (size[1] * scale) as u32];
        let window_handle = if window == 0 { &main } else { &float };
        let _ = window_handle.request_inner_size(PhysicalSize::new(physical[0], physical[1]));
        let resize = shell
            .engine
            .resize_async(window, physical[0], physical[1])
            .unwrap();
        shell.state = native_test_support::receive(&mut events, &resize, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let (_, actions) = draw(&mut shell, size, window, chord(Key::C, false));
        execute_keys(&mut shell, actions, Intent::Copy);
        shell.panels.select_only(Some(9));
        shell
            .panels
            .animation_controls
            .choose_track(9, Property::Position, None);
        shell.panels.timeline.expanded.remove(&7);
        shell.panels.timeline.expanded.insert(9);
        shell.panels.timeline.collapsed_transforms.remove(&9);
        shell.state = shell.engine.seek(50.).unwrap();
        if window == 0 {
            let point = [
                8. + shell.atlas.measure(locale.t(Text::File)) + 22. + 12.,
                motion_ui::theme::metrics::TITLE_BAR + motion_ui::theme::metrics::MENU_BAR * 0.5,
            ];
            let (_, actions) = draw(&mut shell, size, window, click(point));
            assert!(actions.is_empty());
            assert_eq!(shell.panels.menu, Some(1));
            capture(
                &mut events,
                &mut shell,
                size,
                scale,
                window,
                root.join(format!("{name}-edit-menu.png")),
            );
            let point = [
                point[0] + 20.,
                motion_ui::theme::metrics::TITLE_BAR
                    + motion_ui::theme::metrics::MENU_BAR
                    + 4.
                    + 32. * 4.
                    + 15.,
            ];
            let (_, actions) = draw(&mut shell, size, window, click(point));
            execute_keys(&mut shell, actions, Intent::Paste { in_place: false });
        } else {
            let (_, actions) = draw(&mut shell, size, window, chord(Key::V, true));
            execute_keys(&mut shell, actions, Intent::Paste { in_place: true });
        }
        assert_eq!(shell.panels.selection_ids(), [9]);
        let keys = shell.state["project"]["layers"][1]["transform"]["position"]["keys"]
            .as_array()
            .unwrap();
        assert_eq!(keys.len(), if graph { 2 } else { 1 });
        assert_eq!(keys[0]["frame"], if window == 0 { 20 } else { -20 });
        if graph {
            assert_eq!(keys[1]["frame"], if window == 0 { 25 } else { -15 });
        }
        capture(
            &mut events,
            &mut shell,
            size,
            scale,
            window,
            root.join(format!("{name}-pasted.png")),
        );
        shell.engine.save().unwrap();
        assert_eq!(
            serde_json::to_value(motion_core::storage::load(&directory).unwrap()).unwrap(),
            shell.state["project"]
        );
        shell.state = shell.engine.history(0).unwrap();
        assert!(
            shell.state["project"]["layers"][1]["transform"]["position"]["keys"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
