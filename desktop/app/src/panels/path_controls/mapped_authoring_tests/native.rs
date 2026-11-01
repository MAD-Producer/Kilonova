use super::*;
use std::{path::Path, sync::Arc, time::Duration};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};
const NAME:&str="panels::path_controls::mapped_authoring_tests::native::native_mapped_paths_keep_exact_keys_in_both_hosts_languages_and_directions";
fn record(path: impl AsRef<Path>, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn capture(events: &mut EventLoop<()>, h: &mut Harness, path: &Path) {
    h.state = h.engine.state().unwrap();
    let (paint, actions) = h.paint(Input::default());
    assert!(actions.is_empty());
    crate::native_test_support::capture_ui(
        events,
        &h.engine,
        crate::engine::UiFrame {
            views: None,
            inspection: Default::default(),
            paint,
            atlas: Some(crate::engine::AtlasUpload {
                pixels: h.atlas.pixel_data().to_vec(),
                width: h.atlas.config().width,
                height: h.atlas.config().height,
            }),
            logical_size: [1000., 900.],
            composition: None,
            composition_image: None,
            window: u64::from(h.floating),
            node_preview: None,
        },
        path.to_owned(),
    )
    .unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        (1000, 900)
    );
}
#[test]
#[ignore = "manual native mapped vector and mask chrome inputs, GPU captures, worker failures and process reopen"]
fn native_mapped_paths_keep_exact_keys_in_both_hosts_languages_and_directions() {
    const CHILD: &str = "MOTION_MAPPED_PATH_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_MAPPED_PATH_ARTIFACTS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        for reopen in [false, true] {
            let mut c = std::process::Command::new(std::env::current_exe().unwrap());
            c.args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG");
            if reopen {
                c.env("MOTION_MAPPED_PATH_REOPEN", "1");
            } else {
                c.env_remove("MOTION_MAPPED_PATH_REOPEN");
            }
            let output = c.output().unwrap();
            std::fs::write(root.join(format!("{reopen}-stdout.log")), &output.stdout).unwrap();
            std::fs::write(root.join(format!("{reopen}-stderr.log")), &output.stderr).unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let log = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        let stderr = std::fs::read_to_string(root.join("false-stderr.log")).unwrap();
        let stdout = std::fs::read_to_string(root.join("false-stdout.log")).unwrap();
        for cause in ["continuous key addressing", "phase=track_at object=7"] {
            assert!(log.contains(cause));
            assert!(stderr.contains(cause));
            assert!(!stdout.contains(cause));
        }
        assert!(log
            .lines()
            .any(|l| l.contains("INFO") && l.contains("start #") && l.ends_with(" edit")));
        let a: Value =
            serde_json::from_slice(&std::fs::read(root.join("capture-process.json")).unwrap())
                .unwrap();
        let b: Value =
            serde_json::from_slice(&std::fs::read(root.join("reopen-process.json")).unwrap())
                .unwrap();
        assert_ne!(a["pid"], b["pid"]);
        return;
    };
    let root = std::path::PathBuf::from(root);
    let reopen = std::env::var_os("MOTION_MAPPED_PATH_REOPEN").is_some();
    let _log =
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
        let folders: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(Result::unwrap)
            .map(|e| e.path())
            .filter(|p| p.join("saved.json").is_file())
            .collect();
        assert_eq!(folders.len(), 16);
        for folder in folders {
            let e = Engine::start(
                folder.join("project"),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
            let expected: Value =
                serde_json::from_slice(&std::fs::read(folder.join("saved.json")).unwrap()).unwrap();
            let state = e.state().unwrap();
            assert_eq!(state["project"], expected["project"]);
            assert_eq!(state["history"]["undoEntries"], 0);
            record(folder.join("reopened.json"), &state);
            e.request_shutdown().unwrap();
        }
        return;
    }
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let windows: [Arc<winit::window::Window>; 2] = std::array::from_fn(|_| {
        Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 900)),
                )
                .unwrap(),
        )
    });
    for host in [Panel::Masks, Panel::Vector] {
        for reverse in [false, true] {
            for locale in [Locale::Zh, Locale::En] {
                for floating in [false, true] {
                    let folder = root.join(format!("{host:?}-{locale:?}-{floating}-{reverse}"));
                    std::fs::create_dir_all(&folder).unwrap();
                    let mut h = Harness::new(host, floating, locale, reverse);
                    crate::native_test_support::receive(
                        &mut events,
                        &h.engine
                            .attach_async(windows[0].clone(), 1000, 900)
                            .unwrap(),
                        Duration::from_secs(60),
                    )
                    .unwrap()
                    .unwrap();
                    h.engine
                        .float_window(1, windows[1].clone(), 1000, 900)
                        .unwrap();
                    let before = h.state["project"].clone();
                    capture(&mut events, &mut h, &folder.join("before.png"));
                    assert!(h.type_number("17.25").is_empty());
                    assert_eq!(h.state["project"], h.expected(&before, 17.25));
                    let after = h.state["project"].clone();
                    capture(&mut events, &mut h, &folder.join("committed.png"));
                    assert_eq!(h.engine.history(0).unwrap()["project"], before);
                    assert_eq!(h.engine.history(1).unwrap()["project"], after);
                    h.state = h.engine.state().unwrap();
                    let pt = center(h.rect().1);
                    h.input(pointer(pt, true, false));
                    h.input(pointer([pt[0] + 20., pt[1]], false, false));
                    assert_ne!(h.state["project"], after);
                    h.input(Input {
                        focus_lost: true,
                        ..Default::default()
                    });
                    assert_eq!(h.state["project"], after);
                    capture(&mut events, &mut h, &folder.join("cancelled.png"));
                    h.state = h.engine.seek(8.75).unwrap();
                    let undo = h.state["history"]["undoEntries"].clone();
                    let errors = h.type_number("20.25");
                    assert!(!errors.is_empty());
                    assert!(
                        errors
                            .iter()
                            .all(|e| e.contains("continuous key addressing")),
                        "{errors:?}"
                    );
                    assert_eq!(h.state["project"], after);
                    assert_eq!(h.state["history"]["undoEntries"], undo);
                    let saved = h.engine.save().unwrap();
                    record(folder.join("saved.json"), &saved);
                    let dest = folder.join("project");
                    std::fs::create_dir_all(&dest).unwrap();
                    std::fs::copy(
                        h.folder.path().join("project/project.json"),
                        dest.join("project.json"),
                    )
                    .unwrap();
                    h.engine.close_window(1).unwrap();
                    h.engine.request_shutdown().unwrap();
                }
            }
        }
    }
    crate::logging::flush();
}
