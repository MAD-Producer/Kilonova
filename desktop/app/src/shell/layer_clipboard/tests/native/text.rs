//! Actual HWND/wgpu rename fields, component input, shared application text cache.
use super::*;
use crate::panels;
mod legacy_fields;
mod menu;

fn enter() -> Input {
    Input {
        keys: vec![(Key::Enter, Default::default())],
        ..Default::default()
    }
}
#[track_caller]
fn assert_quiet(shell: &mut Shell, size: [f32; 2], window: u64, input: Input) {
    let (_, actions) = draw(shell, size, window, input);
    assert!(
        actions.is_empty(),
        "draft operation escaped into document actions: {actions:?}"
    );
    assert!(!shell.layer_clipboard.busy());
}

#[test]
fn native_text_clipboard_crosses_main_and_floating_fields_and_commits_once() {
    const NAME: &str = "shell::layer_clipboard::tests::native::text::native_text_clipboard_crosses_main_and_floating_fields_and_commits_once";
    const CHILD: &str = "MOTION_TEXT_CLIPBOARD_UI_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_TEXT_CLIPBOARD_UI_ARTIFACTS")
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
    let source_text = "跨窗文本 Text";
    let mut project = motion_core::storage::load(&directory).unwrap();
    project.layers[0].name = source_text.into();
    project.layers.push(motion_core::Layer::solid(
        9, "Target", [32.; 2], [32.; 3], [1.; 4],
    ));
    motion_core::storage::save(&directory, &project).unwrap();
    let mut shell = shell(&directory, Locale::En);
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1280, 800)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1280, 800)),
            )
            .unwrap(),
    );
    let attach = shell.engine.attach_async(main.clone(), 1280, 800).unwrap();
    shell.state = native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    shell.window = Some(main.clone());
    shell
        .engine
        .float_window(1, float.clone(), 1280, 800)
        .unwrap();
    for (source_window, target_window, scale, locale, name) in [
        (0, 1, 1., Locale::En, "main-to-float-en"),
        (1, 0, 1.5, Locale::Zh, "float-to-main-zh"),
    ] {
        shell.panels.locale = locale;
        shell.workspace.locale = locale;
        shell.panels.focused = Some(Panel::Timeline);
        shell.panels.layer_actions.cancel();
        shell.panels.select_only(Some(7));
        shell.scale = scale;
        shell.atlas.set_pixel_ratio(scale);
        let size = [1280., 800.];
        let physical = PhysicalSize::new((size[0] * scale) as u32, (size[1] * scale) as u32);
        for (id, window) in [(0, &main), (1, &float)] {
            let _ = window.request_inner_size(physical);
            let resize = shell
                .engine
                .resize_async(id, physical.width, physical.height)
                .unwrap();
            shell.state =
                native_test_support::receive(&mut events, &resize, Duration::from_secs(30))
                    .unwrap()
                    .unwrap();
        }
        let before = shell.state.clone();
        assert_quiet(&mut shell, size, source_window, enter());
        assert_quiet(&mut shell, size, source_window, chord(Key::X, false));
        assert_eq!(&*shell.text_clipboard.read().unwrap().unwrap(), source_text);
        assert_quiet(&mut shell, size, source_window, chord(Key::Z, false));
        capture(
            &mut events,
            &mut shell,
            size,
            scale,
            source_window,
            root.join(format!("{name}-source-undo.png")),
        );
        // Enter after undo must close an unchanged name without a host command.
        assert_quiet(&mut shell, size, source_window, enter());
        assert!(!shell.panels.layer_actions.text_in(Some(Panel::Timeline)));
        assert_eq!(
            shell.engine.state().unwrap()["revision"],
            before["revision"]
        );
        shell.panels.select_only(Some(9));
        assert_quiet(&mut shell, size, target_window, enter());
        assert_quiet(&mut shell, size, target_window, chord(Key::V, false));
        assert_quiet(&mut shell, size, target_window, chord(Key::Z, false));
        assert_quiet(&mut shell, size, target_window, chord(Key::Z, true));
        capture(
            &mut events,
            &mut shell,
            size,
            scale,
            target_window,
            root.join(format!("{name}-target-paste.png")),
        );
        let (_, mut actions) = draw(&mut shell, size, target_window, enter());
        assert!(
            matches!(actions.as_slice(), [Action::Edit(motion_core::Command::InComposition { composition, command })]
            if composition == shell.state["project"]["composition_id"].as_str().unwrap()
            && matches!(command.as_ref(), motion_core::Command::Rename { object: 9, name } if name == source_text)),
            "{actions:?}"
        );
        shell.action(actions.pop().unwrap(), None).unwrap();
        let (_, reply) = shell.editing.take().unwrap();
        shell.state = native_test_support::receive(&mut events, &reply, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(shell.state["project"]["layers"][1]["name"], source_text);
        shell.engine.save().unwrap();
        assert_eq!(
            serde_json::to_value(motion_core::storage::load(&directory).unwrap()).unwrap(),
            shell.state["project"]
        );
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], before["project"]);
        assert_eq!(shell.state["project"]["layers"][1]["name"], "Target");

        // Both legacy search fields now use the same cache and draft ownership.
        shell.text_clipboard.store("ur").unwrap();
        shell.panels.timeline.timeline_query = "Bl".into();
        shell.panels.timeline.timeline_search = true;
        assert_quiet(&mut shell, size, target_window, chord(Key::V, false));
        assert_eq!(shell.panels.timeline.timeline_query, "Blur");
        assert_quiet(&mut shell, size, target_window, chord(Key::A, false));
        assert_quiet(&mut shell, size, target_window, chord(Key::X, false));
        assert_eq!(shell.panels.timeline.timeline_query, "");
        assert_quiet(&mut shell, size, target_window, chord(Key::Z, false));
        assert_eq!(shell.panels.timeline.timeline_query, "Blur");
        capture(
            &mut events,
            &mut shell,
            size,
            scale,
            target_window,
            root.join(format!("{name}-timeline-search.png")),
        );
        assert_quiet(
            &mut shell,
            size,
            target_window,
            Input {
                keys: vec![(Key::Escape, Default::default())],
                ..Default::default()
            },
        );
        assert!(!shell.panels.timeline.timeline_search);
        shell.panels.timeline.timeline_query.clear();
        shell.panels.effect_search = Some(panels::EffectSearch {
            owner: if source_window == 0 {
                None
            } else {
                Some(Panel::Timeline)
            },
            position: [600., 300.],
            ..Default::default()
        });
        assert_quiet(&mut shell, size, source_window, chord(Key::V, false));
        assert_eq!(shell.panels.effect_search.as_ref().unwrap().query, "Blur");
        capture(
            &mut events,
            &mut shell,
            size,
            scale,
            source_window,
            root.join(format!("{name}-effect-search.png")),
        );
        assert_quiet(&mut shell, size, source_window, chord(Key::Z, false));
        assert_eq!(shell.panels.effect_search.as_ref().unwrap().query, "");
        assert_quiet(&mut shell, size, source_window, chord(Key::Y, false));
        assert_eq!(shell.panels.effect_search.as_ref().unwrap().query, "Blur");
        assert_quiet(
            &mut shell,
            size,
            source_window,
            Input {
                keys: vec![(Key::Escape, Default::default())],
                ..Default::default()
            },
        );
        assert!(shell.panels.effect_search.is_none());
        let after_search = shell.engine.state().unwrap();
        assert_eq!(after_search["project"], before["project"]);
        assert_eq!(after_search["revision"], shell.state["revision"]);
        legacy_fields::run(&mut events, &mut shell, size, scale, name, &root);
        menu::run(&mut events, &mut shell, size, scale, name, &root);
    }
}
