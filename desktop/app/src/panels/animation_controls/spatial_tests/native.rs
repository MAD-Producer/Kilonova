//! Separate-process real GPU captures; hidden native owners pump messages.
use super::worker::{point, render, send};
use super::*;
use crate::panels::{Panel, Panels};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winit::platform::windows::EventLoopBuilderExtWindows;
const NAME:&str="panels::animation_controls::spatial_tests::native::native_spatial_mini_captures_commit_and_reopens_in_separate_process";
fn record(path: impl AsRef<Path>, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
#[test]
#[ignore = "requires actual Windows GPU owners and separate-process capture/reopen"]
fn native_spatial_mini_captures_commit_and_reopens_in_separate_process() {
    const CHILD: &str = "MOTION_SPATIAL_MINI_NATIVE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let reopen = std::env::var_os("MOTION_SPATIAL_MINI_NATIVE_REOPEN").is_some();
        let _guard = crate::logging::init(root.join(if reopen {
            "reopen-logs"
        } else {
            "capture-logs"
        }))
        .unwrap();
        record(
            root.join(if reopen {
                "reopen-process.json"
            } else {
                "capture-process.json"
            }),
            &json!({"pid":std::process::id()}),
        );
        let mut builder = winit::event_loop::EventLoop::builder();
        builder.with_any_thread(true);
        let mut events = builder.build().unwrap();
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
        for (floating, locale, mode, tag) in [
            (false, Locale::Zh, "forward", "zh-main-forward"),
            (true, Locale::En, "reverse", "en-floating-reverse"),
        ] {
            let folder = root.join(tag);
            std::fs::create_dir_all(&folder).unwrap();
            let mut core = fixture(mode, false);
            core.apply(Command::SpatialKeyframes {
                target: target(),
                edit: SpatialKeyEdit::Curve {
                    key: 2,
                    easing: quadratic(),
                },
            })
            .unwrap();
            let at = projected(&core, 4.125);
            if !reopen {
                motion_core::storage::save(&folder.join("project"), core.project()).unwrap();
            }
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
            shell.engine.seek(at).unwrap();
            let mut p = Panels {
                locale,
                focused: Some(Panel::Curves),
                ..Default::default()
            };
            p.select_only(Some(7));
            p.animation_controls = ready(&shell.engine.state().unwrap());
            if reopen {
                let expected: Value =
                    serde_json::from_slice(&std::fs::read(folder.join("after.json")).unwrap())
                        .unwrap();
                assert_eq!(shell.engine.state().unwrap()["project"], expected);
            } else {
                record(
                    folder.join("before.json"),
                    &shell.engine.state().unwrap()["project"],
                );
            }
            let mut capture = |shell: &mut crate::Shell, p: &mut Panels, name: &str| {
                let s = shell.engine.state().unwrap();
                let (paint, actions) = render(p, &s, &mut shell.atlas, floating, Input::default());
                assert!(actions.is_empty());
                let mut ui = shell.ui_frame(paint, [1000., 800.], true);
                ui.window = u64::from(floating);
                ui.composition = None;
                ui.composition_image = None;
                ui.node_preview = None;
                crate::native_test_support::capture_ui(
                    &mut events,
                    &shell.engine,
                    ui,
                    folder.join(format!("{name}.png")),
                )
                .unwrap();
                let img = image::open(folder.join(format!("{name}.png"))).unwrap();
                assert_eq!((img.width(), img.height()), (1000, 800));
            };
            capture(&mut shell, &mut p, if reopen { "reopen" } else { "before" });
            if !reopen {
                let s = shell.engine.state().unwrap();
                assert!(
                    render(&mut p, &s, &mut shell.atlas, floating, Input::default())
                        .1
                        .is_empty()
                );
                let hit = point(&p.animation_controls, 0.5, 0.25);
                let actions = render(
                    &mut p,
                    &s,
                    &mut shell.atlas,
                    floating,
                    pointer(hit, true, false),
                )
                .1;
                assert!(
                    matches!(actions.first(), Some(Action::History(2))),
                    "{actions:?}"
                );
                send(&shell.engine, actions);
                let moved = [hit[0] + 40., hit[1] - 20.];
                let actions = render(
                    &mut p,
                    &shell.engine.state().unwrap(),
                    &mut shell.atlas,
                    floating,
                    pointer(moved, false, true),
                )
                .1;
                assert!(matches!(actions.last(), Some(Action::History(3))));
                send(&shell.engine, actions);
                shell.engine.save().unwrap();
                let after = shell.engine.state().unwrap()["project"].clone();
                record(folder.join("after.json"), &after);
                capture(&mut shell, &mut p, "after");
                shell.engine.history(0).unwrap();
                let undone = shell.engine.state().unwrap()["project"].clone();
                let before: Value =
                    serde_json::from_slice(&std::fs::read(folder.join("before.json")).unwrap())
                        .unwrap();
                assert_eq!(undone, before);
                assert_ne!(undone, after);
                shell.engine.history(1).unwrap();
                assert_eq!(shell.engine.state().unwrap()["project"], after);
            }
            shell.engine.request_shutdown().unwrap();
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_SPATIAL_MINI_NATIVE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    for phase in ["capture", "reopen"] {
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            NAME,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG");
        if phase == "reopen" {
            cmd.env("MOTION_SPATIAL_MINI_NATIVE_REOPEN", "1");
        }
        cmd.stdout(std::fs::File::create(root.join(format!("{phase}-stdout.log"))).unwrap());
        cmd.stderr(std::fs::File::create(root.join(format!("{phase}-stderr.log"))).unwrap());
        let mut child = cmd.spawn().unwrap();
        let start = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() > Duration::from_secs(120) {
                let killed = child.kill();
                let waited = child.wait();
                panic!(
                    "native {phase} timeout pid={} kill={killed:?} wait={waited:?}",
                    child.id()
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let stdout = std::fs::read_to_string(root.join(format!("{phase}-stdout.log"))).unwrap();
        let stderr = std::fs::read_to_string(root.join(format!("{phase}-stderr.log"))).unwrap();
        assert!(status.success(), "{stdout}\n{stderr}");
        assert!(!stdout.contains("motion_studio::"));
    }
    let pid = |name: &str| {
        serde_json::from_slice::<Value>(&std::fs::read(root.join(name)).unwrap()).unwrap()["pid"]
            .clone()
    };
    assert_ne!(pid("capture-process.json"), pid("reopen-process.json"));
}
