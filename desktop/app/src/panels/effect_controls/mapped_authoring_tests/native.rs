//! GPU output and the actual serial worker in both window owners.
use super::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
fn record(path: impl AsRef<Path>, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn capture(
    shell: &mut crate::Shell,
    events: &mut winit::event_loop::EventLoop<()>,
    paint: PaintList,
    floating: bool,
    path: PathBuf,
) {
    let mut ui = shell.ui_frame(paint, [1000., 800.], true);
    ui.window = u64::from(floating);
    ui.composition = None;
    ui.composition_image = None;
    ui.node_preview = None;
    crate::native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    let image = image::open(path).unwrap();
    assert_eq!((image.width(), image.height()), (1000, 800));
}
fn apply_worker(shell: &mut crate::Shell, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => {
                shell
                    .engine
                    .edit(serde_json::to_string(&command).unwrap())
                    .unwrap();
            }
            Action::History(op) => {
                shell.engine.history(op).unwrap();
            }
            _ => panic!("unexpected worker action: {action:?}"),
        }
    }
}
#[test]
#[ignore = "manual actual GPU capture, default error-channel verification and separate-process reopen"]
fn mapped_numeric_native_worker_pixels_errors_history_and_reopen() {
    const NAME:&str="panels::effect_controls::mapped_authoring_tests::native::mapped_numeric_native_worker_pixels_errors_history_and_reopen";
    const CHILD: &str = "MOTION_TRACK_AT_NATIVE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let reopening = std::env::var_os("MOTION_TRACK_AT_REOPEN").is_some();
        let _log = crate::logging::init(root.join(if reopening { "reopen-logs" } else { "logs" }))
            .unwrap();
        record(
            root.join(if reopening {
                "reopen-process.json"
            } else {
                "capture-process.json"
            }),
            &json!({"pid":std::process::id()}),
        );
        if reopening {
            for tag in ["zh-main-forward", "en-floating-reverse"] {
                let folder = root.join(tag);
                let engine = crate::engine::Engine::start(
                    folder.join("project"),
                    motion_desktop_media::platform(),
                    8 << 30,
                )
                .unwrap();
                let actual = engine.state().unwrap();
                let expected: Value =
                    serde_json::from_slice(&std::fs::read(folder.join("saved.json")).unwrap())
                        .unwrap();
                assert_eq!(actual["project"], expected["project"]);
                assert_eq!(actual["history"]["undoEntries"], 0);
                record(folder.join("reopened.json"), &actual);
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
            let (core, catalogue) = fixture(floating);
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
            shell.engine.seek(6.5).unwrap();
            let before = shell.engine.state().unwrap();
            record(folder.join("before.json"), &before);
            let invalid = Command::TrackAt {
                target: effect_target(),
                frame: 6.6,
                edit: TrackEdit::Set {
                    value: TrackValue::Vec4([99., 0., 0., 0.]),
                },
            };
            let error = shell
                .engine
                .edit(serde_json::to_string(&invalid).unwrap())
                .unwrap_err()
                .to_string();
            assert!(error.contains("continuous key addressing"), "{error}");
            assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
            record(
                folder.join("failure.json"),
                &json!({"error":error,"request":invalid,"state":shell.engine.state().unwrap()}),
            );
            let mut controls = controls(&catalogue);
            controls.context_window = floating.then_some(motion_ui::dock::Panel::EffectControls);
            let (paint, actions) = draw(
                &mut controls,
                &mut shell.atlas,
                &before,
                &Input::default(),
                locale,
            );
            assert!(actions.is_empty());
            capture(
                &mut shell,
                &mut events,
                paint,
                floating,
                folder.join("idle.png"),
            );
            let rect = controls.test_rects[&(7, 1, "p0001".into(), 0)];
            let point = [
                (rect.min[0] + rect.max[0]) * 0.5,
                (rect.min[1] + rect.max[1]) * 0.5,
            ];
            let (paint, _) = draw(
                &mut controls,
                &mut shell.atlas,
                &before,
                &Input {
                    mouse: point,
                    ..Default::default()
                },
                locale,
            );
            assert_eq!(
                controls
                    .drag_readout
                    .as_ref()
                    .map(|(_, text)| text.as_str()),
                Some("20")
            );
            capture(
                &mut shell,
                &mut events,
                paint,
                floating,
                folder.join("hover.png"),
            );
            let inputs = [
                pointer(point, true, false),
                pointer([point[0] + 20., point[1]], false, false),
                pointer([point[0] + 20., point[1]], false, false),
                pointer([point[0] + 30., point[1]], false, false),
                pointer([point[0] + 30., point[1]], false, true),
            ];
            let mut command_count = 0;
            for (index, input) in inputs.into_iter().enumerate() {
                let current = shell.engine.state().unwrap();
                let (paint, actions) =
                    draw(&mut controls, &mut shell.atlas, &current, &input, locale);
                for action in &actions {
                    if let Action::Edit(command) = action {
                        assert!(
                            matches!(command,Command::TrackAt {target,frame,..} if target==&effect_target()&&*frame==6.5)
                        );
                        command_count += 1;
                    }
                }
                apply_worker(&mut shell, actions);
                if index == 3 {
                    capture(
                        &mut shell,
                        &mut events,
                        paint,
                        floating,
                        folder.join("drag.png"),
                    );
                }
            }
            assert!(command_count >= 2);
            let after = shell.engine.state().unwrap();
            assert_ne!(after["project"], before["project"]);
            let old =
                &before["project"]["layers"][0]["effects"][0]["params"]["p0001"]["track"]["keys"];
            let next =
                &after["project"]["layers"][0]["effects"][0]["params"]["p0001"]["track"]["keys"];
            assert_eq!(next[0], old[0]);
            assert_ne!(next[1]["value"], old[1]["value"]);
            assert_eq!(next[2], old[2]);
            let (paint, _) = draw(
                &mut controls,
                &mut shell.atlas,
                &after,
                &Input::default(),
                locale,
            );
            capture(
                &mut shell,
                &mut events,
                paint,
                floating,
                folder.join("after.png"),
            );
            record(folder.join("after.json"), &after);
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
            shell.engine.close_window(1).unwrap();
            shell.engine.request_shutdown().unwrap();
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_TRACK_AT_NATIVE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
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
            .env_remove("MOTION_LOG");
        if phase == "reopen" {
            command.env("MOTION_TRACK_AT_REOPEN", "1");
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
        assert!(!String::from_utf8_lossy(&output.stdout).contains("motion_studio::"));
        if phase == "capture" {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("ERROR") && stderr.contains("continuous key addressing"),
                "{stderr}"
            );
            let logs = std::fs::read_dir(root.join("logs"))
                .unwrap()
                .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
                .collect::<String>();
            assert!(
                logs.contains("start #")
                    && logs.contains("track_at")
                    && logs.contains("continuous key addressing"),
                "{logs}"
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
