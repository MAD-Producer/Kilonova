use super::*;
use motion_render::ViewChannel;

fn comparison_frame(window: u64, size: [u32; 2]) -> UiFrame {
    let mut ui = frame(window, size, false);
    for cell in &mut ui.views.as_mut().unwrap().cells[..2] {
        cell.inspection.comparison_split = Some(0.);
    }
    ui
}
fn present(events: &mut EventLoop<()>, engine: &Engine, ui: UiFrame) {
    assert!(receive(events, &engine.present_async(ui).unwrap()).unwrap());
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires native Windows surfaces and GPU"
)]
fn per_cell_channels_regions_and_snapshots_stay_with_their_window_and_active_cell() {
    const TEST:&str="engine::viewer_frame::native::inspection::per_cell_channels_regions_and_snapshots_stay_with_their_window_and_active_cell";
    const CHILD: &str = "MOTION_CELL_INSPECTION_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        if let Some(destination) = std::env::var_os("MOTION_CELL_INSPECTION_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for entry in std::fs::read_dir(root.path()).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
                }
            }
            std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
            std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
        }
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(log.contains("logging initialized level=INFO"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        for marker in [
            "window=0 cell=2: snapshot is unavailable",
            "window=7 cell=0: snapshot is unavailable",
            "phase=snapshot: completed group is no longer current",
            "floating viewer window is closed",
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
    project.working_precision = motion_effects::image_contract::ChannelPrecision::Float16;
    project.background = [0., 0., 0., 1.];
    for (id, z) in [(1, -50.), (2, 50.)] {
        let mut layer = motion_core::Layer::camera(id, "Snapshot camera", 32, 32);
        if let motion_core::Content::Camera { camera } = &mut layer.content {
            camera.position.value = [16., 16., z];
        }
        project.layers.push(layer);
    }
    for (id, z, color) in [(7, -5., [1., 0., 0., 1.]), (8, 5., [0., 0., 1., 1.])] {
        let mut layer =
            motion_core::Layer::solid(id, "Moving snapshot plane", [12.; 2], [16., 16., z], color);
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
    let floating = Arc::new(
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
    // High preserves the composition's working resolution. A snapshot is not
    // resized to the physical viewport (48 here, 64 in the floating window).
    engine.preview_mode(1, 0).unwrap();
    let before = engine.state().unwrap();
    ready(&mut events, &engine, 0, [96, 96], false);
    capture(
        &mut events,
        &engine,
        frame(0, [96, 96], false),
        root.join("zero.png"),
    )
    .unwrap();
    let zero = image::open(root.join("zero.png")).unwrap().to_rgba8();
    let first = receive(
        &mut events,
        &engine.capture_viewer_snapshot_async(0).unwrap(),
    )
    .unwrap();
    assert_eq!(first["cell"], 0);
    assert_eq!(first["width"], 32);
    assert_eq!(first["height"], 32);
    let mut active = frame(0, [96, 96], false);
    active.views.as_mut().unwrap().active = 1;
    present(&mut events, &engine, active);
    let second = receive(
        &mut events,
        &engine.capture_viewer_snapshot_async(0).unwrap(),
    )
    .unwrap();
    assert_eq!(second["cell"], 1);
    assert!(second["generation"].as_u64().unwrap() > first["generation"].as_u64().unwrap());
    receive(
        &mut events,
        &engine
            .save_viewer_snapshot_async(0, root.join("snapshot-one.png"), ViewChannel::Color)
            .unwrap(),
    )
    .unwrap();
    present(&mut events, &engine, frame(0, [96, 96], false));
    receive(
        &mut events,
        &engine
            .save_viewer_snapshot_async(0, root.join("snapshot-zero.png"), ViewChannel::Color)
            .unwrap(),
    )
    .unwrap();
    assert_ne!(
        image::open(root.join("snapshot-zero.png"))
            .unwrap()
            .to_rgba8(),
        image::open(root.join("snapshot-one.png"))
            .unwrap()
            .to_rgba8(),
        "saving the active cell must select its own captured camera"
    );
    engine.seek(10.).unwrap();
    let stale = receive(
        &mut events,
        &engine.capture_viewer_snapshot_async(0).unwrap(),
    )
    .unwrap_err();
    assert!(stale.contains("completed group is no longer current"));
    ready(&mut events, &engine, 0, [96, 96], false);
    capture(
        &mut events,
        &engine,
        frame(0, [96, 96], false),
        root.join("live-ten.png"),
    )
    .unwrap();
    let live = image::open(root.join("live-ten.png")).unwrap().to_rgba8();
    let attempts = engine.preview_info().unwrap()["video"]["renderAttempts"].clone();
    present(&mut events, &engine, comparison_frame(0, [96, 96]));
    capture(
        &mut events,
        &engine,
        comparison_frame(0, [96, 96]),
        root.join("comparison.png"),
    )
    .unwrap();
    let comparison = image::open(root.join("comparison.png")).unwrap().to_rgba8();
    for index in 0..2 {
        assert_eq!(
            crop(&comparison, index),
            crop(&zero, index),
            "cell {index} bound the wrong preserved image"
        );
    }
    for index in 2..4 {
        assert_eq!(
            crop(&comparison, index),
            crop(&live, index),
            "comparison changed live cell {index}"
        );
    }
    let inspection_frame = || {
        let mut ui = frame(0, [96, 96], false);
        let plan = ui.views.as_mut().unwrap();
        for (cell, channel) in plan.cells.iter_mut().zip([
            ViewChannel::Red,
            ViewChannel::Green,
            ViewChannel::Blue,
            ViewChannel::Alpha,
        ]) {
            cell.inspection.channel = channel;
        }
        plan.cells[0].inspection.region = Some([0., 0., 0.5, 1.]);
        ui
    };
    present(&mut events, &engine, inspection_frame());
    capture(
        &mut events,
        &engine,
        inspection_frame(),
        root.join("channels-roi.png"),
    )
    .unwrap();
    let channels = image::open(root.join("channels-roi.png"))
        .unwrap()
        .to_rgba8();
    for pixel in channels.pixels() {
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
    }
    for y in 0..48 {
        for x in 24..48 {
            assert_eq!(*channels.get_pixel(x, y), image::Rgba([0, 0, 0, 255]));
        }
    }
    assert_eq!(
        engine.preview_info().unwrap()["video"]["renderAttempts"],
        attempts,
        "inspection changes rendered new composition content"
    );
    let mut missing = frame(0, [96, 96], false);
    missing.views.as_mut().unwrap().cells[2]
        .inspection
        .comparison_split = Some(0.);
    let missing_cell = receive(&mut events, &engine.present_async(missing).unwrap()).unwrap_err();
    assert!(missing_cell.contains("window=0 cell=2: snapshot is unavailable"));
    engine.float_window(7, floating, 128, 96).unwrap();
    let missing_window = receive(
        &mut events,
        &engine
            .present_async(comparison_frame(7, [128, 96]))
            .unwrap(),
    )
    .unwrap_err();
    assert!(missing_window.contains("window=7 cell=0: snapshot is unavailable"));
    ready(&mut events, &engine, 7, [128, 96], false);
    let foreign = receive(
        &mut events,
        &engine.capture_viewer_snapshot_async(0).unwrap(),
    )
    .unwrap_err();
    assert!(foreign.contains("completed group is no longer current"));
    let float_snapshot = receive(
        &mut events,
        &engine.capture_viewer_snapshot_async(7).unwrap(),
    )
    .unwrap();
    assert_eq!(float_snapshot["cell"], 0);
    assert_eq!(float_snapshot["width"], 32);
    assert_eq!(float_snapshot["height"], 32);
    engine.close_window(7).unwrap();
    let closed = receive(
        &mut events,
        &engine.capture_viewer_snapshot_async(7).unwrap(),
    )
    .unwrap_err();
    assert!(closed.contains("floating viewer window is closed"));
    ready(&mut events, &engine, 0, [96, 96], false);
    capture(
        &mut events,
        &engine,
        frame(0, [96, 96], false),
        root.join("reset.png"),
    )
    .unwrap();
    assert_eq!(
        live,
        image::open(root.join("reset.png")).unwrap().to_rgba8()
    );
    present(&mut events, &engine, comparison_frame(0, [96, 96]));
    capture(
        &mut events,
        &engine,
        comparison_frame(0, [96, 96]),
        root.join("comparison-restored.png"),
    )
    .unwrap();
    assert_eq!(
        comparison,
        image::open(root.join("comparison-restored.png"))
            .unwrap()
            .to_rgba8()
    );
    let after = engine.state().unwrap();
    for key in ["project", "revision", "canUndo", "canRedo"] {
        assert_eq!(before[key], after[key], "inspection changed {key}");
    }
    assert_eq!(after["frame"], 10.);
    drop(engine);
    crate::logging::flush();
    std::fs::write(root.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "firstSnapshot":first,"secondSnapshot":second,"floatingSnapshot":float_snapshot,"stale":stale,
        "missingCell":missing_cell,"missingWindow":missing_window,"foreignOwner":foreign,"closedWindow":closed,
        "snapshotSourcesAreIndependent":true,"inspectionDidNotRenderContent":true,"authoredStateUnchanged":true,
        "gateway":"production Present/CaptureUi/CaptureViewerSnapshot/SaveViewerSnapshot; hidden windows, product layout producers pending"
    })).unwrap()).unwrap();
}
