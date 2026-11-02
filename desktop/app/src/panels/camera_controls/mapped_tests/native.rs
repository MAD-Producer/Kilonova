use super::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};
const NAME:&str="panels::camera_controls::mapped_tests::native::native_mapped_cameras_commit_cancel_and_reopen_in_both_hosts_languages_and_directions";
fn record(path: impl AsRef<Path>, state: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(state).unwrap()).unwrap();
}
fn capture(events: &mut EventLoop<()>, h: &mut Harness, path: &Path) {
    let (paint, out) = h.paint(Input::default());
    assert!(out.is_empty());
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
        path.into(),
    )
    .unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        (1000, 900)
    );
}
#[test]
#[ignore = "manual native mapped camera chrome, GPU capture and separate-process reopen"]
fn native_mapped_cameras_commit_cancel_and_reopen_in_both_hosts_languages_and_directions() {
    const CHILD: &str = "MOTION_MAPPED_CAMERA_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_MAPPED_CAMERA_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        for reopen in [false, true] {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    NAME,
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, &root)
                .env(
                    "MOTION_MAPPED_CAMERA_REOPEN",
                    if reopen { "1" } else { "0" },
                )
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            std::fs::write(root.join(format!("{reopen}-stdout.log")), &out.stdout).unwrap();
            std::fs::write(root.join(format!("{reopen}-stderr.log")), &out.stderr).unwrap();
            assert!(
                out.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        return;
    };
    let root = PathBuf::from(root);
    let _log = crate::logging::init(root.clone()).unwrap();
    let reopen = std::env::var("MOTION_MAPPED_CAMERA_REOPEN").unwrap() == "1";
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
        assert_eq!(folders.len(), 8);
        for folder in folders {
            let expected: Value =
                serde_json::from_slice(&std::fs::read(folder.join("saved.json")).unwrap()).unwrap();
            let e = Engine::start(
                folder.join("project"),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
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
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for reverse in [false, true] {
                let folder = root.join(format!("{locale:?}-{floating}-{reverse}"));
                std::fs::create_dir_all(&folder).unwrap();
                let mut h = Harness::new(floating, locale, reverse);
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
                h.type_at(h.rect(Property::Position, 0), "75");
                let mut expected = before.clone();
                camera_mut(&mut expected)["position"]["keys"][1]["value"][0] = json!(75.);
                assert_eq!(h.state["project"], expected);
                capture(&mut events, &mut h, &folder.join("pose.png"));
                let undo = expected.clone();
                h.type_at(h.lens_rect(lens::Parameter::Focal), "35");
                let fov = motion_core::lens::Lens::from_focal_length(36., 35., 640, 360)
                    .unwrap()
                    .vertical_fov_degrees as f32;
                camera_mut(&mut expected)["fov"]["keys"][1]["value"] = json!(fov);
                assert_eq!(h.state["project"], expected);
                assert_eq!(h.engine.history(0).unwrap()["project"], undo);
                h.state = h.engine.history(1).unwrap();
                assert_eq!(h.state["project"], expected);
                capture(&mut events, &mut h, &folder.join("lens.png"));
                let point = center(h.rect(Property::Fov, 0));
                h.input(Input {
                    mouse: point,
                    pressed: Some((MouseButton::Right, point)),
                    ..Default::default()
                });
                assert!(h.panels.camera_controls.reset.open);
                capture(&mut events, &mut h, &folder.join("reset-menu.png"));
                h.input(key(Key::Enter));
                let mut reset = expected.clone();
                camera_mut(&mut reset)["fov"]["keys"][1]["value"] = json!(45.);
                assert_eq!(h.state["project"], reset);
                h.state = h.engine.history(0).unwrap();
                assert_eq!(h.state["project"], expected);
                h.input(Input::default());
                let pt = center(h.lens_rect(lens::Parameter::Zoom));
                h.input(pointer(pt, 0));
                h.input(pointer([pt[0] + 20., pt[1]], 1));
                assert_ne!(h.state["project"], expected);
                h.input(Input {
                    focus_lost: true,
                    ..Default::default()
                });
                assert_eq!(h.state["project"], expected);
                capture(&mut events, &mut h, &folder.join("cancelled.png"));
                let saved = h.engine.save().unwrap();
                record(folder.join("saved.json"), &saved);
                std::fs::create_dir_all(folder.join("project")).unwrap();
                std::fs::copy(
                    h.folder.path().join("project/project.json"),
                    folder.join("project/project.json"),
                )
                .unwrap();
                h.engine.close_window(1).unwrap();
                h.engine.request_shutdown().unwrap();
            }
        }
    }
    crate::logging::flush();
}
