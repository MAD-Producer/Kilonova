use super::*;

#[test]
fn session_budget_refusal_reaches_real_export_worker_reply_file_and_stderr() {
    const CHILD: &str="MOTION_SESSION_CAPTURE_BUDGET_LOG_ROOT";
    const NAME: &str="engine::frame_capture::tests::session_budget_refusal_reaches_real_export_worker_reply_file_and_stderr";
    if let Some(root)=std::env::var_os(CHILD) {
        let root=PathBuf::from(root);
        let _logger=crate::logging::init(root.clone()).unwrap();
        let project=root.join("project");
        let mut p=motion_core::Project::new(32,24,30,6).unwrap();
        p.background=[0.,1.,0.,1.];
        motion_core::storage::save(&project,&p).unwrap();
        let engine=Engine::start(project,motion_desktop_media::platform(),8<<30).unwrap();
        let output=root.join("protected.png");
        std::fs::write(&output,b"previous publication").unwrap();
        let mut policy=motion_render::resource_policy::MemoryPolicy::default();
        policy.session_allocation_budget_bytes=1;
        engine.plugin(serde_json::json!({"op":"memory_policy","policy":policy}).to_string()).unwrap();
        let reply=engine.capture_async(output.clone(),ExportControl::default()).unwrap();
        let error=reply.recv_timeout(Duration::from_secs(30)).unwrap().unwrap_err();
        assert!(error.contains("PNG GPU initialization") && error.contains("required_bytes=4 budget_bytes=1"),"{error}");
        assert_eq!(std::fs::read(&output).unwrap(),b"previous publication");
        let info=engine.plugin("{\"op\":\"allocation_budget\"}".into()).unwrap();
        assert_eq!(info["allocationBudget"]["admittedBytes"],0);
        assert_eq!(info["allocationBudget"]["liveAllocations"],0);
        policy.session_allocation_budget_bytes=16<<20;
        engine.plugin(serde_json::json!({"op":"memory_policy","policy":policy}).to_string()).unwrap();
        engine.capture_async(output.clone(),ExportControl::default()).unwrap()
            .recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
        let image=image::open(&output).unwrap().into_rgba8();
        assert_eq!(image.dimensions(),(32,24));
        assert!(image.pixels().all(|p|p.0==[0,255,0,255]));
        drop(engine);
        crate::logging::flush();
        return;
    }
    let root=tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact",NAME,"--nocapture","--test-threads=1"])
        .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
    let file=std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr=String::from_utf8(output.stderr).unwrap();
    let stdout=String::from_utf8(output.stdout).unwrap();
    let cause="admitted render allocations exceed configured combined budget";
    let failure=file.lines().find(|line|line.contains("error #") && line.contains(" capture ") && line.contains(cause)).unwrap();
    for context in ["PNG export revision=0 frame=0","PNG GPU initialization","image key=Static(0)","phase=allocate","required_bytes=4 budget_bytes=1","ms:"] {
        assert!(failure.contains(context),"missing {context}: {failure}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(file.lines().any(|line|line.contains("start #") && line.ends_with(" capture")));
    assert_eq!(stderr.matches(cause).count(),1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause,"error #","logging initialized"] {
        assert!(!stdout.contains(marker),"logging leaked to stdout: {stdout}");
    }
}

#[test]
fn finished_worker_panic_clears_observation_before_returning_its_original_error() {
    let flight = Flight {
        request: 47,
        operation: "capture",
        started: Instant::now(),
    };
    let observation = Arc::new(Mutex::new(Some(flight)));
    let handle = std::thread::spawn(|| panic!("controlled completed PNG panic"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !handle.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let mut worker = Worker {
        active: Some((handle, ExportControl::default(), Some(flight))),
        flight: observation.clone(),
    };
    let (reply, _) = channel();
    let error = worker
        .start(i64::MAX, None, ExportControl::default(), reply)
        .unwrap_err();
    assert!(error.contains("request=Some(47)"), "{error}");
    assert!(error.contains("controlled completed PNG panic"), "{error}");
    assert!(worker.active.is_none());
    assert!(observation.lock().unwrap().is_none());
}

#[test]
fn shutdown_waits_for_blocked_png_source_before_clearing_in_flight() {
    let flight = Flight {
        request: 45,
        operation: "capture",
        started: Instant::now(),
    };
    let observation = Arc::new(Mutex::new(Some(flight)));
    let control = ExportControl::default();
    let checked = control.clone();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.bin");
    std::fs::write(&path, b"PNG source").unwrap();
    let source = std::fs::File::open(path).unwrap();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let handle = std::thread::spawn(move || {
        entered.send(()).unwrap();
        released.recv().unwrap();
        assert!(checked.check().is_err());
        assert_eq!(source.metadata().unwrap().len(), 10);
    });
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    let worker = Worker {
        active: Some((handle, control.clone(), Some(flight))),
        flight: observation.clone(),
    };
    let (exited, exit) = channel();
    let owner = std::thread::spawn(move || {
        drop(worker);
        exited.send(()).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while control.check().is_ok() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(matches!(
        exit.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert_eq!(observation.lock().unwrap().unwrap().request, 45);
    release.send(()).unwrap();
    exit.recv_timeout(Duration::from_secs(5)).unwrap();
    owner.join().unwrap();
    assert!(observation.lock().unwrap().is_none());
}

#[test]
fn shutdown_panic_preserves_context_in_default_file_and_stderr_without_stdout_noise() {
    const CHILD: &str = "MOTION_PNG_JOIN_LOG_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(root.into()).unwrap();
        let flight = Flight {
            request: 46,
            operation: "capture",
            started: Instant::now(),
        };
        let observation = Arc::new(Mutex::new(Some(flight)));
        let handle = std::thread::spawn(|| panic!("controlled PNG closure panic"));
        let worker = Worker {
            active: Some((handle, ExportControl::default(), Some(flight))),
            flight: observation.clone(),
        };
        drop(worker);
        assert!(observation.lock().unwrap().is_none());
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","engine::frame_capture::tests::shutdown_panic_preserves_context_in_default_file_and_stderr_without_stdout_noise","--nocapture","--test-threads=1"]).env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    assert!(
        text.contains("PNG export shutdown failed request=Some(46) phase=join elapsed_ms=Some(")
    );
    assert!(text.contains("cause=controlled PNG closure panic"));
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "ERROR motion_studio::engine::frame_capture: PNG export shutdown failed request=Some(46)"
    ));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PNG export shutdown"));
}

#[cfg(feature = "ffmpeg")]
#[test]
fn real_png_completion_and_cancelled_shutdown_preserve_output_transaction() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    motion_core::storage::save(&project, &motion_core::Project::new(32, 32, 30, 2).unwrap())
        .unwrap();
    for cancelled in [false, true] {
        let engine =
            Engine::start(project.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        let output = root.path().join(if cancelled {
            "cancelled.png"
        } else {
            "saved.png"
        });
        let control = ExportControl::default();
        if cancelled {
            control.cancel();
            std::fs::write(&output, b"prior output").unwrap();
        }
        let reply = engine.capture_async(output.clone(), control).unwrap();
        if cancelled {
            drop(engine);
            assert_eq!(
                reply
                    .recv_timeout(Duration::from_secs(15))
                    .unwrap()
                    .unwrap_err(),
                "PNG export cancelled"
            );
            assert_eq!(std::fs::read(output).unwrap(), b"prior output");
        } else {
            reply
                .recv_timeout(Duration::from_secs(15))
                .unwrap()
                .unwrap();
            drop(engine);
            let image = image::open(output).unwrap();
            assert_eq!((image.width(), image.height()), (32, 32));
        }
    }
}
