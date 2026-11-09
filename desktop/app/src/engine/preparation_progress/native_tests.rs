use super::*;
use std::{fs, sync::mpsc::Receiver};
use winit::{
    event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows, window::Window,
};

struct Injection {
    missing_session: bool,
    entered: Sender<()>,
    release: Receiver<()>,
}
static NEXT: Mutex<Option<Injection>> = Mutex::new(None);

fn complete_panic_record(log: &str) -> bool {
    let Some((_, tail)) = log.split_once(" ERROR [motion-engine] motion_studio::logging: panic command=None:") else {
        return false;
    };
    // PanicInfo prints the source location and cause on separate lines. Stop
    // at the next timestamped record, so another Error cannot supply a missing
    // cause or stack for an incomplete panic-hook record.
    let record = tail.lines().take_while(|line| {
        !(line.as_bytes().first().is_some_and(u8::is_ascii_digit)
            && [" ERROR [", " WARN [", " INFO [", " DEBUG [", " TRACE ["]
                .iter().any(|level| line.contains(level)))
    }).collect::<Vec<_>>().join("\n");
    record.contains("panicked at desktop")
        && record.contains("preparation_progress")
        && record.contains("injected idle preparation owner panic")
        && record.contains("Backtrace::force_capture")
        && record.contains("engine::preparation_progress::advance")
        && record.contains("motion_host::ops::catch_panic")
}

#[test]
fn panic_record_requires_its_own_context_cause_location_and_forced_backtrace() {
    let record = "1794022793.506 ERROR [motion-engine] motion_studio::logging: panic command=None: panicked at desktop/app/src/engine/preparation_progress/native_tests.rs:26:17:\ninjected idle preparation owner panic\n   0: std::backtrace::Backtrace::force_capture\n   1: motion_studio::engine::preparation_progress::advance\n   2: motion_host::ops::catch_panic\n1794022793.507 ERROR [motion-engine] motion_studio::engine: pipeline preparation owner stopped session=1 cause=injected idle preparation owner panic";
    assert!(complete_panic_record(record));
    for marker in ["command=None", "panicked at desktop", "\ninjected idle preparation owner panic\n",
        "Backtrace::force_capture", "engine::preparation_progress::advance", "motion_host::ops::catch_panic"] {
        assert!(!complete_panic_record(&record.replacen(marker, "missing", 1)), "missing {marker}");
    }
}

fn arm(_: i64) -> motion_host::Result<Value> {
    let injection = NEXT.lock().unwrap().take().unwrap();
    BEFORE_ADVANCE.with_borrow_mut(|before| {
        assert!(before.is_none());
        *before = Some(Box::new(move |id| {
            injection.entered.send(()).unwrap();
            injection.release.recv_timeout(Duration::from_secs(5)).unwrap();
            if injection.missing_session {
                // Exercise the actual Session registry error, rather than
                // substituting a successful poll or constructing an Err.
                motion_host::session::close(id);
            } else {
                panic!("injected idle preparation owner panic");
            }
        }));
    });
    Ok(serde_json::json!({"armed": true}))
}

fn frame() -> UiFrame {
    UiFrame {
        views: None,
        paint: Default::default(),
        atlas: Some(AtlasUpload { pixels: vec![255; 4], width: 1, height: 1 }),
        logical_size: [64.; 2],
        composition: Some([0., 0., 64., 64.]),
        composition_image: None,
        inspection: Default::default(),
        window: 0,
        node_preview: None,
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires native Windows surfaces and GPU")]
fn native_idle_preparation_faults_preserve_the_primary_failure_and_release_the_owner() {
    const CASE: &str = "engine::preparation_progress::native_tests::native_idle_preparation_faults_preserve_the_primary_failure_and_release_the_owner";
    const CHILD: &str = "MOTION_IDLE_PREPARATION_FAULT_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CASE, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path()).env_remove("MOTION_LOG")
            .stdout(fs::File::create(root.path().join("stdout")).unwrap())
            .stderr(fs::File::create(root.path().join("stderr")).unwrap())
            .spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(90);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() { break status; }
            if Instant::now() >= deadline {
                child.kill().unwrap(); child.wait().unwrap();
                panic!("idle preparation fault child did not return: {}",
                    fs::read_to_string(root.path().join("stderr")).unwrap());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if let Some(destination) = std::env::var_os("MOTION_IDLE_PREPARATION_FAULT_EVIDENCE") {
            fs::create_dir_all(&destination).unwrap();
            for name in ["stdout", "stderr", "report.json", "motion-studio.log"] {
                let path = root.path().join(name);
                if path.exists() { fs::copy(path, PathBuf::from(&destination).join(name)).unwrap(); }
            }
        }
        let stdout = fs::read_to_string(root.path().join("stdout")).unwrap();
        let stderr = fs::read_to_string(root.path().join("stderr")).unwrap();
        assert!(status.success(), "{stdout}\n{stderr}");
        let report: Value = serde_json::from_slice(&fs::read(root.path().join("report.json")).unwrap()).unwrap();
        if report["cacheSupported"] == true {
            let log = fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
            assert!(log.contains("logging initialized level=INFO"));
            for cause in ["native session is closed", "injected idle preparation owner panic"] {
                let record = format!("cause={cause}");
                for text in [&log, &stderr] {
                    assert_eq!(text.lines().filter(|line| line.contains("pipeline preparation owner stopped session=")
                        && line.contains(&record)).count(), 1, "{text}");
                }
                assert!(!stdout.contains(cause));
            }
            assert!(complete_panic_record(&log), "complete panic-hook record missing: {log}");
            assert!(!log.lines().any(|line| line.contains("start #") && line.ends_with(" present")));
            assert_eq!(report["cases"].as_array().unwrap().len(), 2);
        }
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut cases = Vec::new();
    let mut supported = false;
    for missing_session in [true, false] {
        let case = root.join(if missing_session { "closed-session" } else { "panic" });
        let project = motion_core::Project::new(64, 64, 30, 60).unwrap();
        let project_root = case.join("project");
        motion_core::storage::save(&project_root, &project).unwrap();
        let saved = fs::read(project_root.join("project.json")).unwrap();
        let (cache_entered, cache_entry) = channel();
        let (cache_release, cache_gate) = channel();
        let cache_gate = Mutex::new(cache_gate);
        let calls = AtomicUsize::new(0);
        // SAFETY: this isolated test leaf receives only native cache work.
        let storage = unsafe { motion_host::session::DriverPipelineCacheStorage::new(
            case.join("cache"), Arc::new(move |path| {
                if calls.fetch_add(1, Ordering::AcqRel) == 0 {
                    cache_entered.send(()).map_err(std::io::Error::other)?;
                    cache_gate.lock().map_err(|_| std::io::Error::other("fault fixture gate poisoned"))?
                        .recv_timeout(Duration::from_secs(10)).map_err(std::io::Error::other)?;
                }
                fs::create_dir_all(path.parent().unwrap())
            })) };
        let engine = Engine::start_with_pipeline_cache(project_root.clone(),
            motion_desktop_media::platform(), 8 << 30, Some(storage)).unwrap();
        #[allow(deprecated)]
        let window = Arc::new(events.create_window(Window::default_attributes()
            .with_visible(false).with_inner_size(winit::dpi::PhysicalSize::new(64, 64))).unwrap());
        crate::native_test_support::receive(&mut events,
            &engine.attach_async(window, 64, 64).unwrap(), Duration::from_secs(30)).unwrap().unwrap();
        supported = engine.call(|reply| Command::TestStudy {
            operation: |id| motion_host::session::with_session(id, |s| Ok(serde_json::json!({
                "supported":s.graphics.as_ref().unwrap().renderer.driver_pipeline_cache_info().supported}))), reply
        }).unwrap()["supported"] == true;
        if !supported { break; }
        let (entered, entry) = channel();
        let (release, gate) = channel();
        assert!(NEXT.lock().unwrap().replace(Injection { missing_session, entered, release: gate }).is_none());
        engine.call(|reply| Command::TestStudy { operation: arm, reply }).unwrap();
        assert!(!crate::native_test_support::receive(&mut events,
            &engine.present_poll_async(frame(), false).unwrap(), Duration::from_secs(30)).unwrap().unwrap());
        cache_entry.recv_timeout(Duration::from_secs(5)).unwrap();
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(engine.preview_preparing());
        assert!(engine.in_flight().is_none());
        // Both commands are actually in the serial queue before the fatal
        // boundary is released. They must not run or become successful replies.
        let queued_state = engine.request(Command::State).unwrap();
        let queued_save = engine.request(Command::Save).unwrap();
        assert_eq!(engine.queue_depth(), 2);
        cache_release.send(()).unwrap();
        release.send(()).unwrap();
        let wait = crate::native_test_support::receive(&mut events, &queued_state, Duration::from_secs(30));
        assert!(matches!(wait, Err(std::sync::mpsc::RecvTimeoutError::Disconnected)), "{wait:?}");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !engine.worker.as_ref().unwrap().is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(engine.worker.as_ref().unwrap().is_finished());
        let primary = engine.stopped_error();
        let expected = if missing_session { "native session is closed" } else { "injected idle preparation owner panic" };
        assert_eq!(primary.to_string(), expected);
        if missing_session {
            assert!(matches!(primary.host(), Some(motion_host::Error::Message(message)) if message == expected));
        } else {
            assert!(matches!(&primary, Error::Message(message) if message.as_ref() == expected));
        }
        for reply in [&queued_state, &queued_save] {
            let error = engine.poll_reply(reply, "queued idle fault probe").unwrap().unwrap_err();
            assert!(matches!(&error, Error::ReplyDisconnected { cause, .. } if **cause == primary), "{error}");
        }
        assert_eq!(engine.state().unwrap_err(), primary);
        assert_eq!(engine.save().unwrap_err(), primary);
        assert!(!engine.preview_preparing());
        assert_eq!(engine.queue_depth(), 0);
        assert!(engine.in_flight().is_none());
        assert!(engine.factory.lock().unwrap().is_none());
        assert_eq!(motion_host::session::resource_info(&project_root).unwrap()["sessions"], 0);
        assert_eq!(fs::read(project_root.join("project.json")).unwrap(), saved);
        let reopened = Engine::start(project_root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        assert_eq!(reopened.state().unwrap()["project"], serde_json::to_value(&project).unwrap());
        drop(reopened);
        cases.push(serde_json::json!({"missingSession": missing_session, "error": primary,
            "queuedReplies": 2, "preparing": engine.preview_preparing(),
            "queueDepth": engine.queue_depth(), "resourcesReleased": true,
            "projectBytesUnchanged": true, "reopened": true}));
    }
    fs::write(root.join("report.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "cacheSupported": supported, "cases": cases})).unwrap()).unwrap();
}
