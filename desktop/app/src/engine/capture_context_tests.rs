//! Actual worker failures retain their replies and identify the capture target.
use super::*;
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

fn frame() -> UiFrame {
    UiFrame {
        paint: Default::default(),
        atlas: Some(AtlasUpload {
            pixels: vec![255; 4],
            width: 1,
            height: 1,
        }),
        logical_size: [64.; 2],
        composition: None,
        inspection: Default::default(),
        composition_image: None,
        window: 0,
        node_preview: None,
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_capture_failures_report_bounded_paths_and_phases_without_changing_replies() {
    const NAME: &str = "engine::capture_context_tests::actual_capture_failures_report_bounded_paths_and_phases_without_changing_replies";
    const CHILD: &str = "MOTION_CAPTURE_CONTEXT_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--include-ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .stdout(std::fs::File::create(root.path().join("child.stdout.log")).unwrap())
            .stderr(std::fs::File::create(root.path().join("child.stderr.log")).unwrap())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(300) {
                child.kill().unwrap();
                break child.wait().unwrap();
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let output = std::process::Output {
            status,
            stdout: std::fs::read(root.path().join("child.stdout.log")).unwrap(),
            stderr: std::fs::read(root.path().join("child.stderr.log")).unwrap(),
        };
        if let Some(destination) = std::env::var_os("MOTION_CAPTURE_CONTEXT_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for file in std::fs::read_dir(root.path()).unwrap() {
                let file = file.unwrap();
                if file.file_type().unwrap().is_file() {
                    std::fs::copy(file.path(), destination.join(file.file_name())).unwrap();
                }
            }
            std::fs::write(destination.join("child.stdout.log"), &output.stdout).unwrap();
            std::fs::write(destination.join("child.stderr.log"), &output.stderr).unwrap();
        }
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        let report: Value =
            serde_json::from_slice(&std::fs::read(root.path().join("report.json")).unwrap())
                .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(log.contains("logging initialized level=INFO"));
        assert!(
            !stdout.contains("ERROR") && !stdout.contains("operation=capture_ui"),
            "{stdout}"
        );
        assert!(!log.contains("BOUND_END_MARKER") && !stderr.contains("BOUND_END_MARKER"));
        let failures = report["failures"].as_array().unwrap();
        let primary: Vec<_> = log
            .lines()
            .filter(|line| line.contains("error #") && line.contains(" capture_ui "))
            .collect();
        assert_eq!(primary.len(), failures.len(), "{log}");
        // Verify the original failures and successful recovery before requiring
        // new context. The old implementation must fail only on the latter.
        for (failure, line) in failures.iter().zip(&primary) {
            assert!(line.contains(failure["error"].as_str().unwrap()), "{line}");
            assert!(
                stderr.contains(line.split("motion_studio::engine: ").nth(1).unwrap()),
                "{stderr}"
            );
        }
        assert_eq!(report["project_unchanged"], true);
        assert_eq!(report["healthy_pixels_equal"], true);
        let contexts: Vec<_> = log
            .lines()
            .filter(|line| line.contains("capture failed operation=capture_ui "))
            .collect();
        assert_eq!(
            contexts.len(),
            failures.len(),
            "missing capture failure context:\n{log}"
        );
        for ((failure, primary), context) in failures.iter().zip(primary).zip(contexts) {
            let request = primary
                .split("error #")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap();
            assert!(
                context.contains(&format!("request={request} ")),
                "{context}"
            );
            assert!(
                context.contains(" session=") && context.contains(" window=0 "),
                "{context}"
            );
            assert!(
                context.contains(&format!("phase={} ", failure["phase"].as_str().unwrap())),
                "{context}"
            );
            let elapsed = context
                .split("elapsed_ms=")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap();
            elapsed.parse::<u128>().unwrap();
            if !failure["long_path"].as_bool().unwrap() {
                assert!(
                    context.contains(failure["path"].as_str().unwrap()),
                    "{context}"
                );
            }
            assert!(
                context.contains(failure["error"].as_str().unwrap()),
                "{context}"
            );
            assert!(context.chars().count() < 1024, "{context}");
            assert!(
                stderr.contains(context.split("motion_studio::engine: ").nth(1).unwrap()),
                "{stderr}"
            );
        }
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let directory = root.join("project");
    motion_core::storage::save(
        &directory,
        &motion_core::Project::new(16, 16, 30, 12).unwrap(),
    )
    .unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    let before = engine.state().unwrap();
    let mut failures = Vec::new();
    for (path, long_path) in [
        (root.join("unattached.png"), false),
        (
            root.join(format!("{}BOUND_END_MARKER.png", "界".repeat(600))),
            true,
        ),
    ] {
        let error =
            crate::native_test_support::capture_ui(&mut events, &engine, frame(), path.clone())
                .unwrap_err();
        assert!(error.contains("desktop surface is not attached"), "{error}");
        failures.push(serde_json::json!({"path":path,"long_path":long_path,"error":error,"phase":"capture.target"}));
    }
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
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    let baseline = root.join("before.png");
    crate::native_test_support::capture_ui(&mut events, &engine, frame(), baseline.clone())
        .unwrap();
    let blocked = root.join("路径 空格").join("受阻.png");
    std::fs::create_dir_all(&blocked).unwrap();
    let error =
        crate::native_test_support::capture_ui(&mut events, &engine, frame(), blocked.clone())
            .unwrap_err();
    assert!(error.contains("os error"), "{error}");
    assert!(blocked.is_dir());
    failures.push(
        serde_json::json!({"path":blocked,"long_path":false,"error":error,"phase":"capture.save"}),
    );
    let recovery = root.join("after.png");
    crate::native_test_support::capture_ui(&mut events, &engine, frame(), recovery.clone())
        .unwrap();
    let baseline = image::open(baseline).unwrap().into_rgba8();
    let recovery = image::open(recovery).unwrap().into_rgba8();
    assert_eq!(baseline.dimensions(), (64, 64));
    assert_eq!(recovery.dimensions(), baseline.dimensions());
    assert_eq!(recovery.as_raw(), baseline.as_raw());
    let after = engine.state().unwrap();
    assert_eq!(before["revision"], after["revision"]);
    assert_eq!(before["project"], after["project"]);
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "failures":failures,"project_unchanged":true,"healthy_pixels_equal":true
        }))
        .unwrap(),
    )
    .unwrap();
    drop(engine);
    crate::logging::flush();
}
