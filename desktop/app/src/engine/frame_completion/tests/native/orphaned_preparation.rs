use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires native Windows surfaces and GPU")]
fn native_preparation_without_a_frame_reply_is_collected_while_the_owner_is_idle() {
    const CASE: &str = "engine::frame_completion::tests::native::orphaned_preparation::native_preparation_without_a_frame_reply_is_collected_while_the_owner_is_idle";
    const CHILD: &str = "MOTION_ORPHANED_PREPARATION_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CASE, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path()).env_remove("MOTION_LOG")
            .stdout(fs::File::create(root.path().join("stdout")).unwrap())
            .stderr(fs::File::create(root.path().join("stderr")).unwrap())
            .spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let exit = loop {
            if let Some(exit) = child.try_wait().unwrap() { break exit; }
            if Instant::now() >= deadline {
                child.kill().unwrap(); child.wait().unwrap();
                panic!("orphaned preparation child did not return");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if let Some(destination) = std::env::var_os("MOTION_ORPHANED_PREPARATION_EVIDENCE") {
            fs::create_dir_all(&destination).unwrap();
            for name in ["stdout", "stderr", "report.json", "motion-studio.log"] {
                let path = root.path().join(name);
                if path.exists() { fs::copy(path, PathBuf::from(&destination).join(name)).unwrap(); }
            }
        }
        let stdout = fs::read_to_string(root.path().join("stdout")).unwrap();
        let stderr = fs::read_to_string(root.path().join("stderr")).unwrap();
        assert!(exit.success(), "{stdout}\n{stderr}");
        assert!(!stdout.contains("orphaned cache preparation fixture"));
        let report: Value = serde_json::from_slice(&fs::read(root.path().join("report.json")).unwrap()).unwrap();
        if report["cacheSupported"] == true {
            let log = fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
            assert!(log.contains("logging initialized level=INFO"));
            assert!(log.contains("orphaned cache preparation fixture"));
            assert!(stderr.contains("orphaned cache preparation fixture"));
        }
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut cases = Vec::new();
    let mut supported = false;
    for failure in [false, true] {
        let case = root.join(if failure { "failed" } else { "completed" });
        let project = motion_core::Project::new(64, 64, 30, 60).unwrap();
        motion_core::storage::save(&case.join("project"), &project).unwrap();
        let (entered, entry) = channel();
        let (release, gate) = channel();
        let gate = Mutex::new(gate);
        let calls = AtomicUsize::new(0);
        let storage = unsafe { motion_host::session::DriverPipelineCacheStorage::new(
            case.join("cache"), Arc::new(move |path| {
                if calls.fetch_add(1, Ordering::AcqRel) == 0 {
                    entered.send(()).map_err(std::io::Error::other)?;
                    gate.lock().map_err(|_| std::io::Error::other("cache gate poisoned"))?
                        .recv_timeout(Duration::from_secs(10)).map_err(std::io::Error::other)?;
                    if failure { return Err(std::io::Error::other("orphaned cache preparation fixture")); }
                }
                fs::create_dir_all(path.parent().unwrap())
            })) };
        let engine = Engine::start_with_pipeline_cache(case.join("project"),
            motion_desktop_media::platform(), 8 << 30, Some(storage)).unwrap();
        #[allow(deprecated)]
        let window = Arc::new(events.create_window(Window::default_attributes()
            .with_visible(false).with_inner_size(winit::dpi::PhysicalSize::new(64, 64))).unwrap());
        receive(&mut events, &engine.attach_async(window, 64, 64).unwrap());
        supported = engine.call(|reply| Command::TestStudy {
            operation: |id| motion_host::session::with_session(id, |s| Ok(serde_json::json!({
                "supported":s.graphics.as_ref().unwrap().renderer.driver_pipeline_cache_info().supported}))), reply
        }).unwrap()["supported"] == true;
        if !supported { break; }
        assert!(!receive(&mut events, &engine.present_poll_async(frame(0), false).unwrap()));
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(engine.preview_preparing());
        let blocked = engine.state().unwrap();
        assert_eq!(blocked["desktop"]["frameCompletions"]["pending"], 0);
        let mut chrome = frame(0); chrome.composition = None;
        assert!(receive(&mut events, &engine.present_async(chrome).unwrap()));
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        // No engine request, state query, frame submission or UI input may
        // advance the owner here. Only read its shared atomic observation.
        while engine.preview_preparing() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!engine.preview_preparing(), "completed orphaned job left the UI preparing while the owner slept");
        let complete = engine.state().unwrap();
        assert_eq!(complete["desktop"]["frameCompletions"]["pending"], 0);
        assert_eq!(complete["revision"], blocked["revision"]);
        assert_eq!(complete["project"], blocked["project"]);
        assert_eq!(complete["preview"]["pipelinePreparation"]["driverCache"]["disabled"], failure);
        cases.push(serde_json::json!({"failure":failure,"clearedWithoutOwnerRequests":true,
            "pipelinePreparation":complete["preview"]["pipelinePreparation"]}));
    }
    fs::write(root.join("report.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "cacheSupported":supported,"cases":cases})).unwrap()).unwrap();
}
