use super::*;

#[test]
fn node_admission_refusal_preserves_installed_graph_and_default_log_channels() {
    const CHILD: &str = "MOTION_NODE_ADMISSION_CHILD";
    const NAME:&str="engine::node_admission_tests::node_admission_refusal_preserves_installed_graph_and_default_log_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut events = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        #[allow(deprecated)]
        let window = Arc::new(
            events
                .create_window(winit::window::Window::default_attributes().with_visible(false))
                .unwrap(),
        );
        let project = root.join("project");
        motion_core::storage::save(
            &project,
            &motion_core::Project::new(64, 64, 30, 60).unwrap(),
        )
        .unwrap();
        let engine = Engine::start(project, motion_desktop_media::platform(), 8 << 30).unwrap();
        let attached = engine.attach_async(window, 64, 64).unwrap();
        crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(60))
            .unwrap()
            .unwrap();
        let mut editor = crate::node_editor::NodeEditor::default();
        let original = editor.snapshot();
        engine.node_expect(original.preview_stamp()).unwrap();
        let install = engine
            .node_install(original.clone(), original.compile().unwrap())
            .unwrap();
        assert!(crate::native_test_support::receive(
            &mut events,
            &install,
            Duration::from_secs(60)
        )
        .unwrap()
        .unwrap());
        let capture = |events: &mut winit::event_loop::EventLoop<()>, path: PathBuf| {
            let reply = engine
                .request(|reply| Command::NodeCapture {
                    seconds: 0.,
                    path,
                    reply,
                })
                .unwrap();
            crate::native_test_support::receive(events, &reply, Duration::from_secs(60))
                .unwrap()
                .unwrap()
        };
        let before = capture(&mut events, root.join("before.png"));
        let state = engine.state().unwrap();
        let occupied = engine
            .plugin("{\"op\":\"allocation_budget\"}".into())
            .unwrap()["allocationBudget"]["admittedBytes"]
            .as_u64()
            .unwrap();
        let mut policy = motion_render::resource_policy::MemoryPolicy::default();
        policy.session_allocation_budget_bytes = occupied;
        engine
            .plugin(serde_json::json!({"op":"memory_policy","policy":policy}).to_string())
            .unwrap();
        editor.set_parameter_default("color", 0, 0.9).unwrap();
        let changed = editor.snapshot();
        engine.node_expect(changed.preview_stamp()).unwrap();
        let install = engine
            .node_install(changed.clone(), changed.compile().unwrap())
            .unwrap();
        let error =
            crate::native_test_support::receive(&mut events, &install, Duration::from_secs(60))
                .unwrap()
                .unwrap_err();
        assert!(
            error.contains("node graph")
                && error.contains("independent node preview")
                && error.contains("phase=allocate"),
            "{error}"
        );
        assert!(
            error.contains(&format!("budget_bytes={occupied}")),
            "{error}"
        );
        let refused = engine.state().unwrap();
        assert_eq!(
            state["desktop"]["nodePreview"]["installs"],
            refused["desktop"]["nodePreview"]["installs"]
        );
        assert_eq!(state["revision"], refused["revision"]);
        policy.session_allocation_budget_bytes = 64 << 20;
        engine
            .plugin(serde_json::json!({"op":"memory_policy","policy":policy}).to_string())
            .unwrap();
        let retained = capture(&mut events, root.join("retained.png"));
        assert_eq!(before["fingerprint"], retained["fingerprint"]);
        let install = engine
            .node_install(changed.clone(), changed.compile().unwrap())
            .unwrap();
        assert!(crate::native_test_support::receive(
            &mut events,
            &install,
            Duration::from_secs(60)
        )
        .unwrap()
        .unwrap());
        let after = capture(&mut events, root.join("recovered.png"));
        assert_ne!(before["fingerprint"], after["fingerprint"]);
        assert_eq!(
            image::open(root.join("recovered.png"))
                .unwrap()
                .into_rgba8()
                .dimensions(),
            (
                crate::node_preview::RASTER_SIZE,
                crate::node_preview::RASTER_SIZE
            )
        );
        assert_eq!(
            engine.state().unwrap()["desktop"]["nodePreview"]["installs"],
            2
        );
        let peaks = engine.allocation_frame_peaks().unwrap();
        assert!(peaks.active.is_empty());
        assert_eq!(peaks.recent.len(), 3);
        assert!(!peaks.complete_coverage && !peaks.is_physical_vram);
        for frame in peaks.recent {
            assert_eq!(frame.operation, motion_render::FrameOperation::NodeCapture);
            assert_eq!(frame.outcome, Some(motion_render::AllocationFrameOutcome::Rendered));
            assert!(frame.request > 0 && frame.submissions >= 1);
            assert!(frame.peak_admitted_bytes >= frame.start_admitted_bytes);
            assert!(frame.end_admitted_bytes.is_some());
            assert_eq!(frame.unconfirmed_submissions, 0);
        }
        drop(engine);
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cause = "admitted render allocations exceed configured combined budget";
    let failure = file
        .lines()
        .find(|line| {
            line.contains("error #") && line.contains(" node_install ") && line.contains(cause)
        })
        .unwrap();
    for marker in [
        "node graph",
        "independent node preview",
        "phase=allocate",
        "required_bytes=",
        "budget_bytes=",
        "ms:",
    ] {
        assert!(failure.contains(marker), "missing {marker}: {failure}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(file
        .lines()
        .any(|line| line.contains("start #") && line.ends_with(" node_install")));
    assert_eq!(stderr.matches(cause).count(), 1);
    for marker in [cause, "error #", "logging initialized"] {
        assert!(!stdout.contains(marker), "logging leaked to stdout");
    }
}
