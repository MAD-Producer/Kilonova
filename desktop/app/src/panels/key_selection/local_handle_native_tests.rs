//! Native GPU, serial worker and a separate process reopen the same projects.
use super::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
fn record(path: impl AsRef<Path>, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn dispatch(shell: &mut crate::Shell, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(c) => {
                shell
                    .engine
                    .edit(serde_json::to_string(&c).unwrap())
                    .unwrap();
            }
            Action::History(op) => {
                shell.engine.history(op).unwrap();
            }
            other => panic!("unexpected native handle action {other:?}"),
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
#[ignore = "manual main/floating GPU capture, worker errors, independent-process reopen"]
fn native_local_handles_edit_undo_save_pixels_and_reopen() {
    const CHILD: &str = "MOTION_GRAPH_HANDLES_CHILD";
    const NAME:&str="panels::key_selection::tests::ui::local_timing::handles::native::native_local_handles_edit_undo_save_pixels_and_reopen";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let reopening = std::env::var_os("MOTION_GRAPH_HANDLES_REOPEN").is_some();
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
            let core = fixture(floating);
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
            let mut p = ready(&before, locale, false);
            let (paint, actions) = draw(
                &mut p,
                &before,
                Input::default(),
                floating,
                &mut shell.atlas,
            );
            dispatch(&mut shell, actions);
            let at = points(&paint)[2];
            capture(
                &mut shell,
                &mut events,
                paint,
                floating,
                folder.join("before.png"),
            );
            let modifiers = Modifiers {
                alt: true,
                ..Default::default()
            };
            let (_, actions) = draw(
                &mut p,
                &before,
                pointer(at, true, false, modifiers),
                floating,
                &mut shell.atlas,
            );
            dispatch(&mut shell, actions);
            let next = [at[0] + if floating { -5. } else { 5. }, at[1] - 12.];
            let (_, actions) = draw(
                &mut p,
                &before,
                pointer(next, false, false, modifiers),
                floating,
                &mut shell.atlas,
            );
            dispatch(&mut shell, actions);
            let preview = shell.engine.state().unwrap();
            assert_ne!(preview["project"], before["project"]);
            let (_, actions) = draw(
                &mut p,
                &preview,
                pointer(next, false, true, modifiers),
                floating,
                &mut shell.atlas,
            );
            dispatch(&mut shell, actions);
            let after = shell.engine.state().unwrap();
            record(folder.join("after.json"), &after);
            let mut expected = before["project"].clone();
            expected["layers"][0]["transform"]["position"]["keys"][1]["curve"] =
                after["project"]["layers"][0]["transform"]["position"]["keys"][1]["curve"].clone();
            assert_eq!(after["project"], expected);
            assert_eq!(
                after["history"]["undoEntries"].as_u64(),
                before["history"]["undoEntries"].as_u64().map(|n| n + 1)
            );
            let t: motion_core::Track<[f32; 3]> = serde_json::from_value(
                after["project"]["layers"][0]["transform"]["position"].clone(),
            )
            .unwrap();
            // Obtain the canonical opposite control through the established
            // handle protocol. Its linear conversion uses 1 - 1/3 for Y,
            // which is one ULP apart from spelling 2/3 directly.
            let mut reference = core.snapshot();
            reference.layers[0].time_mapping = None;
            let mut reference = Engine::new(reference).unwrap();
            reference
                .apply(Command::KeyframeHandle {
                    target: key(7, None, 0).target,
                    frame: 5,
                    incoming: false,
                    control: [0.25, 0.1],
                })
                .unwrap();
            assert_ne!(
                controls(t.effective_easing(1).unwrap())[0],
                [1. / 3., 1. / 3.]
            );
            assert_eq!(
                controls(t.effective_easing(1).unwrap())[1],
                controls(
                    reference.project().layers[0]
                        .transform
                        .position
                        .effective_easing(1)
                        .unwrap()
                )[1]
            );
            shell.engine.history(0).unwrap();
            let undo = shell.engine.state().unwrap();
            assert_eq!(undo["project"], before["project"]);
            record(folder.join("undo.json"), &undo);
            shell.engine.history(1).unwrap();
            let redo = shell.engine.state().unwrap();
            assert_eq!(redo["project"], after["project"]);
            record(folder.join("redo.json"), &redo);
            let invalid = Command::LocalKeyframes {
                target: key(7, None, 0).target,
                edit: LocalKeyEdit::Handle {
                    frame: 4,
                    incoming: false,
                    control: [0.2, 0.1],
                },
            };
            assert!(shell
                .engine
                .edit(serde_json::to_string(&invalid).unwrap())
                .unwrap_err()
                .to_string().contains("selected keyframe does not exist"));
            assert_eq!(shell.engine.state().unwrap()["project"], after["project"]);
            for (speed, suffix) in [(false, "values"), (true, "speed")] {
                p.timeline.speed_graph = speed;
                let (paint, actions) =
                    draw(&mut p, &after, Input::default(), floating, &mut shell.atlas);
                dispatch(&mut shell, actions);
                assert_eq!(points(&paint).len(), 4);
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    floating,
                    folder.join(format!("{suffix}.png")),
                );
            }
            p.timeline.speed_graph = false;
            let (paint, actions) =
                draw(&mut p, &after, Input::default(), floating, &mut shell.atlas);
            dispatch(&mut shell, actions);
            let at = points(&paint)[2];
            let (_, actions) = draw(
                &mut p,
                &after,
                pointer(at, true, false, modifiers),
                floating,
                &mut shell.atlas,
            );
            dispatch(&mut shell, actions);
            let next = [at[0], at[1] - 15.];
            let (_, actions) = draw(
                &mut p,
                &after,
                pointer(next, false, false, modifiers),
                floating,
                &mut shell.atlas,
            );
            dispatch(&mut shell, actions);
            let preview = shell.engine.state().unwrap();
            assert_ne!(preview["project"], after["project"]);
            record(folder.join("cancel-preview.json"), &preview);
            let mut escape = pointer(next, false, false, modifiers);
            escape
                .keys
                .push((motion_ui::input::Key::Escape, Modifiers::default()));
            let (_, actions) = draw(&mut p, &preview, escape, floating, &mut shell.atlas);
            dispatch(&mut shell, actions);
            let canceled = shell.engine.state().unwrap();
            assert_eq!(canceled["project"], after["project"]);
            assert_eq!(
                canceled["history"]["undoEntries"],
                after["history"]["undoEntries"]
            );
            record(folder.join("canceled.json"), &canceled);
            shell.engine.save().unwrap();
            record(folder.join("saved.json"), &shell.engine.state().unwrap());
        }
        return;
    }
    let root = std::env::var_os("MOTION_GRAPH_HANDLES_ARTIFACT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("motion-graph-handles-{}", std::process::id()))
        });
    std::fs::create_dir_all(&root).unwrap();
    for phase in ["capture", "reopen"] {
        let stdout = std::fs::File::create(root.join(format!("{phase}-stdout.log"))).unwrap();
        let stderr = std::fs::File::create(root.join(format!("{phase}-stderr.log"))).unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--ignored", "--exact", NAME, "--nocapture"])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG")
            .stdout(stdout)
            .stderr(stderr);
        if phase == "reopen" {
            command.env("MOTION_GRAPH_HANDLES_REOPEN", "1");
        }
        let mut child = command.spawn().unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(120) {
                let killed = child.kill();
                let waited = child.wait();
                panic!(
                    "{phase} exceeded 120s: pid={} kill={killed:?} wait={waited:?}",
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
                .map(|p| std::fs::read_to_string(p.unwrap().path()).unwrap())
                .collect::<String>();
            assert!(
                logs.contains("start #")
                    && logs.contains("local_handle")
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
