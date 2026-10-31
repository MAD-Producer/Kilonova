use super::*;
use crate::engine::{AtlasUpload, Command, Engine};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};
mod inspection;

fn frame(window: u64, size: [u32; 2], pan: bool) -> UiFrame {
    let w = size[0] as f32 / 2.;
    let h = size[1] as f32 / 2.;
    UiFrame {
        views: Some(Plan {
            active: 0,
            cells: [1, 2, 1, 2]
                .into_iter()
                .enumerate()
                .map(|(i, camera)| Cell {
                    camera: if i == 2 {
                        Camera::Composition
                    } else if i == 3 {
                        Camera::Observer(motion_core::Observer::new(32, 32))
                    } else {
                        Camera::Camera(camera)
                    },
                    inspection: Default::default(),
                    placement: ViewPlacement {
                        viewport: [(i % 2) as f32 * w, (i / 2) as f32 * h, w, h],
                        image: (pan && i == 0).then_some([12., 0., w, h]),
                    },
                })
                .collect(),
        }),
        paint: Default::default(),
        atlas: Some(AtlasUpload {
            pixels: vec![255; 4],
            width: 1,
            height: 1,
        }),
        logical_size: [size[0] as f32, size[1] as f32],
        composition: Some([0., 0., size[0] as f32, size[1] as f32]),
        composition_image: None,
        inspection: Default::default(),
        window,
        node_preview: None,
    }
}
fn receive<T>(
    events: &mut EventLoop<()>,
    pending: &std::sync::mpsc::Receiver<Result<T, String>>,
) -> Result<T, String> {
    crate::native_test_support::receive(events, pending, Duration::from_secs(30)).unwrap()
}
fn ready(events: &mut EventLoop<()>, engine: &Engine, window: u64, size: [u32; 2], pan: bool) {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if receive(
            events,
            &engine.present_async(frame(window, size, pan)).unwrap(),
        )
        .unwrap()
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "normal Present did not finish its group"
        );
    }
}
fn capture(
    events: &mut EventLoop<()>,
    engine: &Engine,
    ui: UiFrame,
    path: PathBuf,
) -> Result<serde_json::Value, String> {
    receive(
        events,
        &engine
            .request(|reply| Command::CaptureUi { ui, path, reply })
            .unwrap(),
    )
}
fn crop(image: &image::RgbaImage, cell: usize) -> Vec<u8> {
    image::imageops::crop_imm(
        image,
        (cell % 2) as u32 * image.width() / 2,
        (cell / 2) as u32 * image.height() / 2,
        image.width() / 2,
        image.height() / 2,
    )
    .to_image()
    .into_raw()
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires native Windows surfaces and GPU"
)]
fn normal_present_and_capture_route_cameras_freeze_canvas_and_window_identity() {
    const TEST: &str="engine::viewer_frame::native::normal_present_and_capture_route_cameras_freeze_canvas_and_window_identity";
    const CHILD: &str = "MOTION_VIEWER_FRAME_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        let log =
            std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap_or_default();
        if let Some(destination) = std::env::var_os("MOTION_VIEWER_FRAME_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for entry in std::fs::read_dir(root.path()).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
                }
            }
            std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
        }
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(log.contains("logging initialized level=INFO"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        for marker in [
            "viewer_camera object=998",
            "phase=capture: completed group is no longer current",
            "floating window is closed",
            "inspection must be set on each cell",
        ] {
            assert!(log.contains(marker), "file missing {marker}");
            assert!(stderr.contains(marker), "stderr missing {marker}");
            assert!(!stdout.contains(marker), "stdout leaked {marker}");
        }
        assert!(!log
            .lines()
            .any(|line| line.contains("start #") && line.ends_with(" present")));
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let mut project = motion_core::Project::new(32, 32, 30, 60).unwrap();
    project.background = [0., 0., 0., 1.];
    for (id, z) in [(1, -50.), (2, 50.)] {
        let mut layer = motion_core::Layer::camera(id, "Independent view", 32, 32);
        if let motion_core::Content::Camera { camera } = &mut layer.content {
            camera.position.value = [16., 16., z];
        }
        project.layers.push(layer);
    }
    for (id, z, color) in [(7, -5., [1., 0., 0., 1.]), (8, 5., [0., 0., 1., 1.])] {
        let mut layer =
            motion_core::Layer::solid(id, "Animated plane", [12.; 2], [16., 16., z], color);
        layer.three_d = true;
        for (f, x) in [(0, 12.), (20, 20.)] {
            layer
                .transform
                .position
                .upsert(f, [x, 16., z], motion_core::Ease::Linear)
                .unwrap();
        }
        project.layers.push(layer);
    }
    motion_core::storage::save(&root.join("project"), &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(96, 96)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(128, 96)),
            )
            .unwrap(),
    );
    let engine = Engine::start(
        root.join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    receive(&mut events, &engine.attach_async(main, 96, 96).unwrap()).unwrap();
    engine.preview_mode(2, 0).unwrap();
    let before = engine.state().unwrap();
    ready(&mut events, &engine, 0, [96, 96], false);
    capture(
        &mut events,
        &engine,
        frame(0, [96, 96], false),
        root.join("main-zero.png"),
    )
    .unwrap();
    let zero = image::open(root.join("main-zero.png")).unwrap().to_rgba8();
    assert_ne!(
        crop(&zero, 0),
        crop(&zero, 1),
        "opposite authored cameras must produce different pixels"
    );
    let attempts = engine.preview_info().unwrap()["video"]["renderAttempts"]
        .as_u64()
        .unwrap();
    ready(&mut events, &engine, 0, [96, 96], true);
    assert_eq!(
        engine.preview_info().unwrap()["video"]["renderAttempts"],
        attempts
    );
    capture(
        &mut events,
        &engine,
        frame(0, [96, 96], true),
        root.join("main-pan.png"),
    )
    .unwrap();
    let pan = image::open(root.join("main-pan.png")).unwrap().to_rgba8();
    assert_ne!(crop(&zero, 0), crop(&pan, 0));
    for i in 1..4 {
        assert_eq!(crop(&zero, i), crop(&pan, i), "pan leaked into cell {i}");
    }

    engine.seek(10.).unwrap();
    let frozen = receive(
        &mut events,
        &engine
            .present_frozen_async(frame(0, [96, 96], false))
            .unwrap(),
    )
    .unwrap();
    assert!(frozen);
    assert_eq!(engine.state().unwrap()["frame"], 10.);
    assert_eq!(
        engine.preview_info().unwrap()["video"]["renderAttempts"],
        attempts,
        "frozen repaint rendered new content"
    );
    let stale = capture(
        &mut events,
        &engine,
        frame(0, [96, 96], false),
        root.join("stale.png"),
    )
    .unwrap_err();
    assert!(stale.contains("completed group is no longer current"));
    assert!(!root.join("stale.png").exists());
    ready(&mut events, &engine, 0, [96, 96], false);
    capture(
        &mut events,
        &engine,
        frame(0, [96, 96], false),
        root.join("main-ten.png"),
    )
    .unwrap();
    let ten = image::open(root.join("main-ten.png")).unwrap().to_rgba8();
    assert_ne!(
        zero, ten,
        "stopping frozen presentation must render the shared clock"
    );

    engine.float_window(7, float, 128, 96).unwrap();
    ready(&mut events, &engine, 7, [128, 96], false);
    capture(
        &mut events,
        &engine,
        frame(7, [128, 96], false),
        root.join("float-ten.png"),
    )
    .unwrap();
    let floating = image::open(root.join("float-ten.png")).unwrap().to_rgba8();
    assert_eq!(floating.dimensions(), (128, 96));
    assert_ne!(crop(&floating, 0), crop(&floating, 1));
    let foreign = capture(
        &mut events,
        &engine,
        frame(0, [96, 96], false),
        root.join("foreign.png"),
    )
    .unwrap_err();
    assert!(foreign.contains("completed group is no longer current"));
    assert!(!root.join("foreign.png").exists());
    ready(&mut events, &engine, 0, [96, 96], false);
    let mut invalid = frame(0, [96, 96], false);
    invalid.views.as_mut().unwrap().cells[1].camera = Camera::Camera(998);
    let missing = receive(&mut events, &engine.present_async(invalid).unwrap()).unwrap_err();
    assert!(missing.contains("viewer_camera object=998"));
    let mut unsupported = frame(0, [96, 96], false);
    unsupported.inspection.channel = motion_render::ViewChannel::Alpha;
    let inspection = receive(&mut events, &engine.present_async(unsupported).unwrap()).unwrap_err();
    assert!(inspection.contains("inspection must be set on each cell"));
    engine.close_window(7).unwrap();
    let closed = receive(
        &mut events,
        &engine.present_async(frame(7, [128, 96], false)).unwrap(),
    )
    .unwrap_err();
    assert!(closed.contains("floating window is closed"));
    ready(&mut events, &engine, 0, [96, 96], false);
    capture(
        &mut events,
        &engine,
        frame(0, [96, 96], false),
        root.join("main-recovered.png"),
    )
    .unwrap();
    assert_eq!(
        ten,
        image::open(root.join("main-recovered.png"))
            .unwrap()
            .to_rgba8()
    );
    let mut single = frame(0, [96, 96], false);
    single.views = None;
    assert!(receive(&mut events, &engine.present_async(single).unwrap()).unwrap());
    let retired = capture(
        &mut events,
        &engine,
        frame(0, [96, 96], false),
        root.join("retired.png"),
    )
    .unwrap_err();
    assert!(retired.contains("completed group is no longer current"));
    assert!(!root.join("retired.png").exists());
    let mut single = frame(0, [96, 96], false);
    single.views = None;
    capture(&mut events, &engine, single, root.join("single-ten.png")).unwrap();
    ready(&mut events, &engine, 0, [96, 96], false);
    capture(
        &mut events,
        &engine,
        frame(0, [96, 96], false),
        root.join("main-after-single.png"),
    )
    .unwrap();
    assert_eq!(
        ten,
        image::open(root.join("main-after-single.png"))
            .unwrap()
            .to_rgba8()
    );
    let after = engine.state().unwrap();
    for key in ["project", "revision", "canUndo", "canRedo"] {
        assert_eq!(before[key], after[key], "presentation edited {key}");
    }
    assert_eq!(after["frame"], 10.);
    drop(engine);
    crate::logging::flush();
    std::fs::write(root.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "protocol":"production Present, PresentFrozen, CaptureUi and FloatWindow", "staleCapture":stale,
        "foreignCapture":foreign,"missingCamera":missing,"closedWindow":closed,"unsupportedInspection":inspection,
        "retiredCapture":retired,"singleViewRoundTripPreservedPixels":true,
        "frozenClockPreserved":true,"panOnlyChangedFirstCell":true,"authoredStateUnchanged":true,
        "uiActivation":"layout/menu producers still pending; hidden native protocol acceptance"
    })).unwrap()).unwrap();
}
