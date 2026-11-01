//! Actual chrome input, serial worker, GPU surfaces and separate-process reopen.
use super::*;
use crate::panels::{self, Panel, Panels};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

fn record(path: impl AsRef<Path>, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn draw(
    shell: &mut crate::Shell,
    p: &mut Panels,
    floating: bool,
    input: Input,
) -> (PaintList, Vec<Action>) {
    let state = shell.engine.state().unwrap();
    let mut paint = PaintList::default();
    let actions = if floating {
        panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Curves,
            [1000., 800.],
            p,
            &state,
            &input,
        )
    } else {
        panels::chrome(
            &mut paint,
            &mut shell.atlas,
            [1000., 800.],
            &motion_ui::dock::Node::dock(Panel::Curves.dock()),
            p,
            &state,
            &input,
        )
    };
    (paint, actions)
}
fn worker(shell: &mut crate::Shell, actions: Vec<Action>) {
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
            other => panic!("unexpected native action {other:?}"),
        }
    }
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
#[test]
#[ignore = "manual actual GPU/chrome, worker error channels and separate-process reopen"]
fn mapped_mini_native_chrome_drag_undo_pixels_and_reopen() {
    const CHILD: &str = "MOTION_TIME_MINI_NATIVE_CHILD";
    const NAME:&str="panels::animation_controls::mapped_tests::native::mapped_mini_native_chrome_drag_undo_pixels_and_reopen";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let reopen = std::env::var_os("MOTION_TIME_MINI_REOPEN").is_some();
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
                let state = engine.state().unwrap();
                let expected: Value =
                    serde_json::from_slice(&std::fs::read(folder.join("saved.json")).unwrap())
                        .unwrap();
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
            let frame = projected(&core, 3.);
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
            shell.engine.seek(frame).unwrap();
            let before = shell.engine.state().unwrap();
            record(folder.join("before.json"), &before);
            let invalid = Command::LocalKeyframes {
                target: motion_core::KeyframeTarget::Property {
                    object: 7,
                    property: Property::Opacity,
                    axis: None,
                },
                edit: LocalKeyEdit::Curve {
                    frames: vec![999],
                    easing: quadratic(),
                },
            };
            let cause = shell
                .engine
                .edit(serde_json::to_string(&invalid).unwrap())
                .unwrap_err()
                .to_string();
            assert!(
                cause.contains("selected keyframe does not exist"),
                "{cause}"
            );
            assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
            record(
                folder.join("failure.json"),
                &json!({"error":cause,"command":invalid}),
            );
            let mut p = Panels {
                selected: Some(7),
                focused: Some(Panel::Curves),
                locale,
                ..Default::default()
            };
            p.animation_controls
                .choose_track_in(&before, 7, Property::Opacity, None);
            let (paint, actions) = draw(&mut shell, &mut p, floating, Input::default());
            assert!(actions.is_empty());
            capture(
                &mut shell,
                &mut events,
                paint,
                floating,
                folder.join("idle.png"),
            );
            let (_, graph, bounds) = p.animation_controls.mini_test_geometry().unwrap();
            let point = [
                graph.min[0] + bounds.x.fraction(0.5) as f32 * graph.width(),
                graph.max[1] - bounds.y.fraction(0.25) as f32 * graph.height(),
            ];
            let hover = [
                graph.min[0] + bounds.x.fraction(0.5) as f32 * graph.width(),
                graph.min[1] + graph.height() * 0.5,
            ];
            let (paint, actions) = draw(
                &mut shell,
                &mut p,
                floating,
                Input {
                    mouse: hover,
                    ..Default::default()
                },
            );
            assert!(actions.is_empty());
            let readout = p.animation_controls.mini_test_readout().unwrap();
            assert!(
                readout
                    .0
                    .ends_with(&format!("{:.2} f", projected(&core, 6.))),
                "{readout:?}"
            );
            capture(
                &mut shell,
                &mut events,
                paint,
                floating,
                folder.join("hover.png"),
            );
            let moved = [point[0] + 40., point[1] - 18.];
            let mut updates = 0;
            for (index, input) in [
                pointer(point, true, false),
                pointer(moved, false, false),
                pointer([moved[0] + 15., moved[1] - 3.], false, false),
                pointer([moved[0] + 15., moved[1] - 3.], false, true),
            ]
            .into_iter()
            .enumerate()
            {
                let (paint, actions) = draw(&mut shell, &mut p, floating, input);
                for action in &actions {
                    if let Action::Edit(command) = action {
                        assert!(
                            matches!(command,Command::LocalKeyframes {edit:LocalKeyEdit::Curve {frames,..},..} if frames==&[3])
                        );
                        updates += 1;
                    }
                }
                worker(&mut shell, actions);
                if index == 2 {
                    capture(
                        &mut shell,
                        &mut events,
                        paint,
                        floating,
                        folder.join("drag.png"),
                    );
                }
            }
            assert!(updates >= 2);
            let after = shell.engine.state().unwrap();
            let old = &before["project"]["layers"][0]["transform"]["opacity"]["keys"];
            let next = &after["project"]["layers"][0]["transform"]["opacity"]["keys"];
            assert_eq!(old[0], next[0]);
            assert_ne!(old[1]["curve"], next[1]["curve"]);
            assert_eq!(old[2], next[2]);
            assert_eq!(next[1]["frame"], 3);
            assert_eq!(next[1]["value"], old[1]["value"]);
            assert!(p.animation_controls.user_key().is_none());
            assert!(p
                .selected_keys
                .local_selections()
                .iter()
                .any(|(_, frames)| frames == &[3]));
            let (paint, actions) = draw(&mut shell, &mut p, floating, Input::default());
            assert!(actions.is_empty());
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
    let root = std::env::var_os("MOTION_TIME_MINI_NATIVE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    for phase in ["capture", "reopen"] {
        // File-backed child output permits bounded waiting without pipe backpressure.
        let stdout = std::fs::File::create(root.join(format!("{phase}-stdout.log"))).unwrap();
        let stderr = std::fs::File::create(root.join(format!("{phase}-stderr.log"))).unwrap();
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
            .stdout(stdout)
            .stderr(stderr);
        if phase == "reopen" {
            command.env("MOTION_TIME_MINI_REOPEN", "1");
        }
        let mut child = command.spawn().unwrap();
        let started = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(120) {
                let killed = child.kill();
                let waited = child.wait();
                panic!(
                    "native {phase} exceeded 120s pid={} cleanup={killed:?} wait={waited:?}",
                    child.id()
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let stdout = std::fs::read_to_string(root.join(format!("{phase}-stdout.log"))).unwrap();
        let stderr = std::fs::read_to_string(root.join(format!("{phase}-stderr.log"))).unwrap();
        assert!(status.success(), "{phase}: {stdout}\n{stderr}");
        assert!(!stdout.contains("motion_studio::"));
        if phase == "capture" {
            assert!(
                stderr.contains("ERROR") && stderr.contains("selected keyframe does not exist"),
                "{stderr}"
            );
            let logs = std::fs::read_dir(root.join("logs"))
                .unwrap()
                .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
                .collect::<String>();
            assert!(
                logs.contains("start #")
                    && logs.contains("local_curve")
                    && logs.contains("selected keyframe does not exist"),
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
