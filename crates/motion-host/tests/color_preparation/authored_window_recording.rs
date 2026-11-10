//! Performance recording through the ordinary managed-window consumer.
use super::*;

#[test]
#[ignore = "requires actual Windows surface, GPU and pinned native OCIO runtime"]
fn managed_window_performance_recording_preserves_pixels_and_success_boundaries() {
    const CHILD: &str = "MOTION_MANAGED_RECORDING_CHILD";
    const TEST: &str = "authored::window::recording::managed_window_performance_recording_preserves_pixels_and_success_boundaries";
    if std::env::var_os(CHILD).is_none() {
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                TEST,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .stdout(std::fs::File::create(root.path().join("stdout")).unwrap())
            .stderr(std::fs::File::create(root.path().join("stderr")).unwrap())
            .spawn()
            .unwrap();
        let began = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if began.elapsed() > Duration::from_secs(60) {
                let killed = child.kill();
                let joined = child.wait();
                panic!("recording test exceeded 60s: kill={killed:?} wait={joined:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let stdout = std::fs::read_to_string(root.path().join("stdout")).unwrap();
        let stderr = std::fs::read_to_string(root.path().join("stderr")).unwrap();
        assert!(status.success(), "{status}\n{stdout}\n{stderr}");
        assert!(stdout.contains(
            "managed recording: three commits; retained and invalid requests excluded; overflow=1"
        ));
        return;
    }
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let reference = reference();
    let root = tempfile::tempdir().unwrap();
    let mut project = document(&reference, 0, AlphaMode::Straight, root.path());
    settings(&mut project).display.transfer = ColorTransfer::Srgb;
    project.layers.push(Layer::solid(
        1,
        "recorded color",
        [64.; 2],
        [32., 32., 0.],
        reference.cases[0].input[3],
    ));
    let before = serde_json::to_value(&project).unwrap();
    let id = motion_host::session::open(
        root.path().to_owned(),
        project,
        Arc::new(session_installation::NativePlatform(super::super::runtime())),
    )
    .unwrap();
    motion_host::session::with_session(id, |session| {
        attach_profiled(session, &events, true);
        Ok(())
    })
    .unwrap();
    let baseline = motion_host::session::with_session(id, |session| {
        Ok([1, 2, 4].map(|count| capture(session, 0.5, &views(count), false)))
    })
    .unwrap();
    let commands = motion_host::ops::preview::start_command_profiling(id, 16).unwrap();
    assert!(
        commands["gpuCommands"].is_object(),
        "this qualification requires explicit command timing support"
    );
    motion_host::ops::preview::start_profiling(id, 3).unwrap();
    motion_host::session::with_session(id, |session| {
        for (index, count) in [1, 2, 4].into_iter().enumerate() {
            assert_eq!(capture(session, 1.5, &views(count), false), baseline[index]);
            let record = session
                .recorder
                .as_ref()
                .unwrap()
                .report(serde_json::json!({}));
            assert_eq!(record["frameCount"], index + 1);
            assert_eq!(capture(session, 1.5, &views(count), true), baseline[index]);
            assert_eq!(
                session
                    .recorder
                    .as_ref()
                    .unwrap()
                    .report(serde_json::json!({}))["frameCount"],
                index + 1
            );
        }
        assert!(session
            .render_views_with_overlay(77, f64::NAN, &views(1), |_, _, _| panic!(
                "invalid request must not paint"
            ))
            .is_err());
        let report = session
            .recorder
            .as_ref()
            .unwrap()
            .report(serde_json::json!({}));
        assert_eq!(report["frameCount"], 3);
        assert_eq!(report["overflowFrames"], 0);
        assert_eq!(capture(session, 2.5, &views(1), false), baseline[0]);
        assert_eq!(
            serde_json::to_value(session.engine.project()).unwrap(),
            before
        );
        Ok(())
    })
    .unwrap();
    motion_host::ops::preview::stop_profiling(id).unwrap();
    let bytes = std::fs::read(root.path().join("exports/preview-performance.json")).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(report["frameCount"], 3);
    assert_eq!(report["overflowFrames"], 1);
    assert_eq!(report["gpuSampleCount"], 0);
    assert_eq!(
        report["metadata"]["frameDimensionSource"],
        "managed_surface"
    );
    let backend = std::env::var("WGPU_BACKEND").unwrap_or_else(|_| "selected".into());
    match backend.as_str() {
        "vulkan" => assert_eq!(report["metadata"]["graphics"]["backend"], "Vulkan"),
        "dx12" => assert_eq!(report["metadata"]["graphics"]["backend"], "Dx12"),
        _ => {}
    }
    let frames = report["frames"].as_array().unwrap();
    for frame in frames {
        assert_eq!(frame["frame"], 1.5);
        assert_eq!(frame["renderWidth"], 128);
        assert_eq!(frame["renderHeight"], 128);
        assert!(frame["presentationCommand"].is_object());
        assert!(frame["gpuTotalUs"].is_null());
        assert!(frame["resources"].is_null());
        let wall = frame["renderWallUs"].as_u64().unwrap();
        let acquire = frame["acquireUs"].as_u64().unwrap();
        let present = frame["presentCallUs"].as_u64().unwrap();
        assert_eq!(
            frame["cpuPrepareUs"],
            wall.saturating_sub(acquire).saturating_sub(present)
        );
        assert!(frame["submitUs"].as_u64().unwrap() <= wall);
    }
    assert!(frames
        .windows(2)
        .all(|p| p[0]["sequence"].as_u64().unwrap() < p[1]["sequence"].as_u64().unwrap()));
    motion_host::ops::preview::stop_command_profiling(id).unwrap();
    motion_host::ops::preview::start_profiling(id, 1).unwrap();
    motion_host::session::with_session(id, |session| {
        assert_eq!(capture(session, 3.5, &views(1), false), baseline[0]);
        let ordinary = session
            .recorder
            .as_ref()
            .unwrap()
            .report(serde_json::json!({}));
        assert_eq!(ordinary["frameCount"], 1);
        assert!(ordinary["frames"][0]["presentationCommand"].is_null());
        assert!(ordinary["frames"][0]["gpuTotalUs"].is_null());
        Ok(())
    })
    .unwrap();
    motion_host::ops::preview::stop_profiling(id).unwrap();
    if let Some(folder) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let folder = std::path::PathBuf::from(folder).join(backend);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("managed-preview-performance.json"), bytes).unwrap();
        std::fs::copy(
            root.path().join("exports/preview-performance.json"),
            folder.join("managed-preview-performance-default.json"),
        )
        .unwrap();
    }
    motion_host::session::close(id);
    println!(
        "managed recording: three commits; retained and invalid requests excluded; overflow=1"
    );
}
