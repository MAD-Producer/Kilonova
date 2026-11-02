use super::*;
use crate::layer_clipboard::controller::Intent;

fn complete(shell: &mut crate::Shell, actions: Vec<Action>) {
    assert_eq!(actions.len(), 1);
    for action in actions {
        let Action::CurveClipboard {
            intent,
            source,
            selections,
            local_selections,
        } = action
        else {
            panic!("{action:?}")
        };
        assert!(selections.is_empty());
        assert_eq!(local_selections.len(), 1);
        shell
            .request_curve_clipboard(intent, source, selections, local_selections)
            .unwrap();
    }
    let began = std::time::Instant::now();
    while shell.layer_clipboard.busy() {
        shell.poll_layer_clipboard();
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "clipboard stuck: {:?}",
            shell.layer_clipboard.phase()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
#[ignore = "manual actual GPU chrome input and native system clipboard with process reopen"]
fn fractional_curve_system_clipboard_native_copy_paste_and_reopen() {
    const NAME:&str="panels::animation_controls::mapped_tests::native::clipboard::fractional_curve_system_clipboard_native_copy_paste_and_reopen";
    const CHILD: &str = "MOTION_LOCAL_EASING_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let reopen = std::env::var_os("MOTION_LOCAL_EASING_REOPEN").is_some();
        let _logging =
            crate::logging::init(root.join(if reopen { "reopen-logs" } else { "logs" })).unwrap();
        record(
            root.join(if reopen {
                "reopen-process.json"
            } else {
                "capture-process.json"
            }),
            &json!({"pid":std::process::id()}),
        );
        if reopen {
            for tag in ["zh-main-forward", "en-floating-reverse"] {
                let folder = root.join(tag);
                let engine = crate::engine::Engine::start(
                    folder.join("project"),
                    motion_desktop_media::platform(),
                    8 << 30,
                )
                .unwrap();
                let expected: Value =
                    serde_json::from_slice(&std::fs::read(folder.join("saved.json")).unwrap())
                        .unwrap();
                let state = engine.state().unwrap();
                assert_eq!(state["project"], expected["project"]);
                assert_eq!(state["history"]["undoEntries"], 0);
                record(folder.join("reopened.json"), &state);
                engine.request_shutdown().unwrap();
            }
            return;
        }
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let mut events = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        #[allow(deprecated)]
        let main = Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 800)),
                )
                .unwrap(),
        );
        #[allow(deprecated)]
        let float = Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 800)),
                )
                .unwrap(),
        );
        for (floating, locale, tag) in [
            (false, Locale::Zh, "zh-main-forward"),
            (true, Locale::En, "en-floating-reverse"),
        ] {
            let folder = root.join(tag);
            std::fs::create_dir_all(&folder).unwrap();
            let mut core = fixture(floating);
            seed_curve(&mut core);
            motion_core::storage::save(&folder.join("project"), core.project()).unwrap();
            let mut shell = crate::Shell::new(folder.join("project"), None, Some(locale)).unwrap();
            shell.home = None;
            let attached = shell.engine.attach_async(main.clone(), 1000, 800).unwrap();
            shell.state = crate::native_test_support::receive(
                &mut events,
                &attached,
                Duration::from_secs(30),
            )
            .unwrap()
            .unwrap();
            shell.window = Some(main.clone());
            shell
                .engine
                .float_window(1, float.clone(), 1000, 800)
                .unwrap();
            let before = shell.engine.state().unwrap();
            record(folder.join("before.json"), &before);
            let mut p = Panels {
                selected: Some(7),
                focused: Some(Panel::Curves),
                locale,
                ..Default::default()
            };
            for (local, index, intent, label) in [
                (3, 0, Intent::Copy, "copy"),
                (-3, 1, Intent::Paste { in_place: true }, "paste"),
            ] {
                shell
                    .engine
                    .seek(projected(&core, f64::from(local)))
                    .unwrap();
                p.animation_controls = controls();
                let (paint, actions) = draw(&mut shell, &mut p, floating, Input::default());
                assert!(actions.is_empty());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    floating,
                    folder.join(format!("{label}-idle.png")),
                );
                let hit = p.animation_controls.clipboard_test_rects().unwrap()[index];
                let point = [
                    (hit.min[0] + hit.max[0]) * 0.5,
                    (hit.min[1] + hit.max[1]) * 0.5,
                ];
                let (paint, actions) = draw(&mut shell, &mut p, floating, click(point));
                assert!(
                    matches!(actions.as_slice(),[Action::CurveClipboard {intent:actual,local_selections,..}]
                    if *actual==intent && local_selections[0].frames==[local]),
                    "{actions:?}"
                );
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    floating,
                    folder.join(format!("{label}-click.png")),
                );
                complete(&mut shell, actions);
                record(
                    folder.join(format!("{label}-result.json")),
                    &shell.engine.state().unwrap(),
                );
            }
            let after = shell.engine.state().unwrap();
            let old = &before["project"]["layers"][0]["transform"]["opacity"]["keys"];
            let next = &after["project"]["layers"][0]["transform"]["opacity"]["keys"];
            assert_eq!(next[0]["curve"], old[1]["curve"]);
            assert_ne!(next[0]["curve"], old[0]["curve"]);
            assert_eq!(next[0]["frame"], -3);
            assert_eq!(next[0]["value"], old[0]["value"]);
            assert_eq!(next[1], old[1]);
            assert_eq!(next[2], old[2]);
            let mut restored = after["project"].clone();
            restored["layers"][0]["transform"]["opacity"]["keys"][0] = old[0].clone();
            assert_eq!(restored, before["project"]);
            let saved = shell.engine.save().unwrap();
            assert_eq!(saved["project"], after["project"]);
            record(folder.join("saved.json"), &saved);
            let undo = shell.engine.history(0).unwrap();
            assert_eq!(undo["project"], before["project"]);
            assert_eq!(
                undo["history"]["undoEntries"],
                before["history"]["undoEntries"]
            );
            record(folder.join("undo.json"), &undo);
            let redo = shell.engine.history(1).unwrap();
            assert_eq!(redo["project"], after["project"]);
            record(folder.join("redo.json"), &redo);
            let invalid = Command::PasteEasing {
                clipboard: Box::new(motion_core::CurveClipboard::new(quadratic()).unwrap()),
                selections: vec![],
                local_selections: vec![motion_core::LocalKeyframeSelection {
                    target: motion_core::KeyframeTarget::Property {
                        object: 7,
                        property: Property::Opacity,
                        axis: None,
                    },
                    frames: vec![999],
                }],
            };
            let cause = shell
                .engine
                .edit(serde_json::to_string(&invalid).unwrap())
                .unwrap_err();
            assert!(cause.contains("does not exist"), "{cause}");
            assert_eq!(shell.engine.state().unwrap()["project"], after["project"]);
            record(
                folder.join("failure.json"),
                &json!({"command":invalid,"error":cause}),
            );
            shell.engine.close_window(1).unwrap();
            shell.engine.request_shutdown().unwrap();
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LOCAL_EASING_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    struct Restore(String);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Err(cause) = arboard::Clipboard::new()
                .and_then(|mut clipboard| clipboard.set_text(self.0.clone()))
            {
                log::error!("local easing acceptance restore failed reason={cause}");
            }
        }
    }
    let _restore = Restore(
        arboard::Clipboard::new()
            .unwrap()
            .get_text()
            .expect("native acceptance requires an existing text clipboard to restore"),
    );
    for phase in ["capture", "reopen"] {
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
            .env_remove("MOTION_LOG")
            .stdout(std::fs::File::create(root.join(format!("{phase}-stdout.log"))).unwrap())
            .stderr(std::fs::File::create(root.join(format!("{phase}-stderr.log"))).unwrap());
        if phase == "reopen" {
            command.env("MOTION_LOCAL_EASING_REOPEN", "1");
        }
        let mut child = command.spawn().unwrap();
        let began = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if began.elapsed() > Duration::from_secs(120) {
                let killed = child.kill();
                let waited = child.wait();
                panic!("native {phase} timeout kill={killed:?} wait={waited:?}");
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let stdout = std::fs::read_to_string(root.join(format!("{phase}-stdout.log"))).unwrap();
        let stderr = std::fs::read_to_string(root.join(format!("{phase}-stderr.log"))).unwrap();
        assert!(status.success(), "{phase}: {stdout}\n{stderr}");
        assert!(!stdout.contains("motion_studio::"));
        if phase == "capture" {
            assert_eq!(
                stderr
                    .lines()
                    .filter(|s| s.contains("ERROR") && s.contains("does not exist"))
                    .count(),
                2,
                "{stderr}"
            );
            let log = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
            assert!(
                log.contains("start #")
                    && log.contains("curve_clipboard_read")
                    && log.contains("paste_easing_local")
                    && log.contains("does not exist"),
                "{log}"
            );
        }
    }
    let read = |name| -> Value {
        serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap()
    };
    assert_ne!(
        read("capture-process.json")["pid"],
        read("reopen-process.json")["pid"]
    );
}
