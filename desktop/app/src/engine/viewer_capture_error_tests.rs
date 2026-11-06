//! Real viewer operations must keep native failures after a worker reply.
use super::*;
use motion_render::{ResourceRejection, ViewChannel};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

fn frame() -> UiFrame {
    UiFrame {
        views: None,
        paint: Default::default(),
        atlas: Some(AtlasUpload {
            pixels: vec![255; 4],
            width: 1,
            height: 1,
        }),
        logical_size: [64.; 2],
        composition: Some([0., 0., 64., 64.]),
        inspection: Default::default(),
        composition_image: None,
        window: 0,
        node_preview: None,
    }
}

fn receive<T>(
    events: &mut EventLoop<()>,
    pending: &std::sync::mpsc::Receiver<Result<T, Error>>,
) -> Result<T, Error> {
    crate::native_test_support::receive(events, pending, Duration::from_secs(60)).unwrap()
}

fn native(error: &Error) -> &motion_host::Error {
    let mut cause = error
        .host()
        .expect("worker must preserve a native host error");
    while let motion_host::Error::Context { cause: next, .. } = cause {
        cause = next;
    }
    cause
}

fn budget_error(error: &Error) {
    let motion_host::Error::Render(cause) = native(error) else {
        panic!("viewer lost its native RenderError: {error:?}");
    };
    assert!(
        matches!(cause.resource_rejection(), Some(ResourceRejection::Budget {
        required_bytes, budget_bytes: 1, ..
    }) if *required_bytes > 1)
    );
    // Identical display text cannot manufacture the native rejection.
    let text_only = Error::from(error.to_string());
    assert_eq!(text_only.to_string(), error.to_string());
    assert!(text_only.host().is_none());
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires a native window and GPU"
)]
fn native_viewer_capture_and_save_failures_keep_types_default_sinks_and_recover() {
    const TEST: &str = "engine::viewer_capture_error_tests::native_viewer_capture_and_save_failures_keep_types_default_sinks_and_recover";
    const CHILD: &str = "MOTION_VIEWER_CAPTURE_ERROR_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let stdout = root.path().join("child.stdout");
        let stderr = root.path().join("child.stderr");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .stdout(std::fs::File::create(&stdout).unwrap())
            .stderr(std::fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let began = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if began.elapsed() > Duration::from_secs(300) {
                child.kill().unwrap();
                break child.wait().unwrap();
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let stdout = std::fs::read_to_string(stdout).unwrap();
        let stderr = std::fs::read_to_string(stderr).unwrap();
        if let Some(destination) = std::env::var_os("MOTION_VIEWER_CAPTURE_ERROR_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for entry in std::fs::read_dir(root.path()).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
                }
            }
        }
        assert!(
            status.success(),
            "child exit={status} elapsed={:?}\n{stdout}\n{stderr}",
            began.elapsed()
        );
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        let failures: Vec<_> = file
            .lines()
            .filter(|line| {
                line.contains("motion_studio::engine: error #")
                    && (line.contains(" capture_viewer_snapshot ")
                        || line.contains(" save_viewer_snapshot "))
            })
            .collect();
        assert_eq!(failures.len(), 3, "{file}");
        for line in failures {
            let record = line.split("motion_studio::engine: ").nth(1).unwrap();
            assert!(stderr.contains(record), "missing stderr: {record}");
            assert!(!stdout.contains(record), "stdout leaked: {record}");
            let (request, operation) = record
                .strip_prefix("error #")
                .unwrap()
                .split_once(' ')
                .unwrap();
            let operation = operation.split_whitespace().next().unwrap();
            let start = format!("start #{request} {operation}");
            assert!(file.find(&start).unwrap() < file.find(record).unwrap());
            assert!(!stderr.contains(&start));
            assert!(!stdout.contains(&start));
        }
        for marker in [
            "phase=target",
            "phase=save",
            "viewer snapshot failed",
            "configured combined budget",
        ] {
            assert!(file.contains(marker), "file missing {marker}");
            assert!(stderr.contains(marker), "stderr missing {marker}");
            assert!(!stdout.contains(marker), "stdout leaked {marker}");
        }
        return;
    };

    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let directory = root.join("project");
    let mut project = motion_core::Project::new(16, 16, 30, 12).unwrap();
    project.background = [0., 1., 0., 1.];
    motion_core::storage::save(&directory, &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
            )
            .unwrap(),
    );
    let attached = engine.attach_async(window, 64, 64).unwrap();
    receive(&mut events, &attached).unwrap();
    let began = Instant::now();
    loop {
        if receive(&mut events, &engine.present_async(frame()).unwrap()).unwrap() {
            break;
        }
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "viewer front did not complete"
        );
    }
    let before = engine.state().unwrap();
    let snapshot = receive(
        &mut events,
        &engine.capture_viewer_snapshot_async(0).unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot["generation"], 1);
    let baseline = root.join("before.png");
    receive(
        &mut events,
        &engine
            .save_viewer_snapshot_async(0, baseline.clone(), ViewChannel::Color)
            .unwrap(),
    )
    .unwrap();
    let baseline = image::open(baseline).unwrap().into_rgba8();
    assert_eq!(baseline.dimensions(), (16, 16));
    assert!(baseline.pixels().all(|pixel| pixel.0 == [0, 255, 0, 255]));

    let normal_limit = engine.allocation_budget.info().unwrap().budget_bytes;
    engine.allocation_budget.set_limit(1).unwrap();
    let failure = receive(
        &mut events,
        &engine.capture_viewer_snapshot_async(0).unwrap(),
    )
    .unwrap_err();
    budget_error(&failure);
    let refused = root.join("refused.png");
    let failure = receive(
        &mut events,
        &engine
            .save_viewer_snapshot_async(0, refused.clone(), ViewChannel::Color)
            .unwrap(),
    )
    .unwrap_err();
    assert!(failure.to_string().starts_with("phase=target: "));
    budget_error(&failure);
    assert!(!refused.exists());
    engine.allocation_budget.set_limit(normal_limit).unwrap();

    let blocked = root.join("blocked.png");
    std::fs::create_dir(&blocked).unwrap();
    let failure = receive(
        &mut events,
        &engine
            .save_viewer_snapshot_async(0, blocked.clone(), ViewChannel::Color)
            .unwrap(),
    )
    .unwrap_err();
    assert!(failure.to_string().starts_with("phase=save: "));
    let motion_host::Error::Image(image::ImageError::IoError(cause)) = native(&failure) else {
        panic!("viewer lost its native image IO error: {failure:?}");
    };
    assert!(matches!(
        cause.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::IsADirectory
    ));
    assert!(cause.raw_os_error().is_some());
    assert!(blocked.is_dir());
    let recovery = root.join("after.png");
    let saved = receive(
        &mut events,
        &engine
            .save_viewer_snapshot_async(0, recovery.clone(), ViewChannel::Color)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved["generation"], snapshot["generation"],
        "failed capture replaced the retained snapshot"
    );
    assert_eq!(image::open(recovery).unwrap().into_rgba8(), baseline);
    let after = engine.state().unwrap();
    assert_eq!(before["project"], after["project"]);
    assert_eq!(before["revision"], after["revision"]);
    let next = receive(
        &mut events,
        &engine.capture_viewer_snapshot_async(0).unwrap(),
    )
    .unwrap();
    assert_eq!(
        next["generation"], 2,
        "failed capture consumed a generation"
    );
    drop(engine);
    crate::logging::flush();
}
