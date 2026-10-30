use super::*;

#[test]
fn automatic_checkpoints_require_a_destination_but_explicit_exports_keep_all_records() {
    let root = tempfile::tempdir().unwrap();
    let d = Diagnostics::new(None, None);
    for request in 0..128 {
        d.record(request, "worker", "edit", Duration::from_millis(70),
            None, None, Default::default());
    }
    assert!(d.state.lock().unwrap().last_checkpoint.is_none());
    assert_eq!(d.status()["droppedRecords"], 0);
    let path = root.path().join("explicit.json");
    d.export(path.clone()).unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    let report: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(report["totalStalls"], 128);
    assert_eq!(report["recent"].as_array().unwrap().len(), 128);
    assert_eq!(report["stalls"].as_array().unwrap().len(), 128);
    assert_eq!(report["recent"][0]["request"], 0);
    assert_eq!(report["recent"][127]["request"], 127);
    assert_eq!(report["reporting"]["droppedRecords"], 0);
    // An explicit checkpoint remains supported even without a local destination.
    d.checkpoint().unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();

    let local = Diagnostics::new(Some(root.path().join("automatic.json")), None);
    slow(&local);
    assert!(local.state.lock().unwrap().last_checkpoint.is_some());
    local.checkpoint().unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert!(root.path().join("automatic.json").is_file());
}

fn slow(d:&Diagnostics) {
    d.record(1,"worker","edit",Duration::from_millis(70),Some(Duration::from_millis(50)),Some(Duration::from_millis(20)),Default::default());
}
#[test]
fn recorder_is_bounded_and_keeps_queue_and_execution_distinct_without_payload_data() {
    let d=Diagnostics::new(None,None);
    let state=json!({"frame":21.,"root":"C:/private/customer", "project":{"name":"secret_customer_name","width":1920,"height":1080,
        "layers":[{"name":"private_layer","source":"private.mov","effects":[{"name":"secret_effect"}]}]},"api_key":"secret-key"});
    let context=Context::from_state(&state);
    assert_eq!(context.layers,1);assert_eq!(context.effects,1);
    for i in 0..3000 {d.record(i,"worker","edit",Duration::from_millis(70),Some(Duration::from_millis(50)),Some(Duration::from_millis(20)),context.clone());}
    let report=d.report();let bytes=serde_json::to_vec(&report).unwrap();let text=String::from_utf8(bytes.clone()).unwrap();
    assert!(report["stalls"].as_array().unwrap().len()<=STALL_LIMIT);
    assert!(report["recent"].as_array().unwrap().len()<=RECENT_LIMIT);
    assert!(bytes.len()<MAX_REPORT_BYTES);
    assert_eq!(report["stalls"][0]["queueWaitUs"],50000);
    assert_eq!(report["stalls"][0]["workUs"],20000);
    for secret in ["customer","private_layer","private.mov","secret_effect","secret-key"]{assert!(!text.contains(secret));}
    assert_eq!(report["reporting"]["enabled"],false);
    assert!(d.set_enabled(true).is_err());
}
#[test]
fn contended_recording_never_waits_for_the_report_lock() {
    let d=Diagnostics::new(None,None);
    let guard=d.state.lock().unwrap();
    slow(&d);
    assert_eq!(d.dropped_records.load(Ordering::Relaxed),1);
    drop(guard);
    assert_eq!(d.report()["totalStalls"],0);
}
#[test]
fn export_replaces_the_file_atomically_and_reports_write_failure_without_stopping_recorder() {
    let root=tempfile::tempdir().unwrap();let path=root.path().join("report.json");
    std::fs::write(&path,"old").unwrap();
    let d=Diagnostics::new(None,None);slow(&d);
    d.export(path.clone()).unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    let report:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(report["schemaVersion"],1);
    let bad=root.path().join("occupied");std::fs::create_dir(&bad).unwrap();
    assert!(d.export(bad).unwrap().recv_timeout(Duration::from_secs(5)).unwrap().is_err());
    slow(&d);assert!(d.report()["totalStalls"].as_u64().unwrap()>=2);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(),2);
}

#[test]
fn reporter_logs_before_snapshot_and_preserves_unobserved_write_failures() {
    const CHILD: &str = "MOTION_DIAGNOSTICS_WRITE_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let blocked = root.join("occupied");
        std::fs::write(&blocked, b"keep existing bytes").unwrap();
        let d = Diagnostics::new(Some(blocked.join("report.json")), None);
        // Stop the actual reporter inside report(), before serialization/IO.
        // Its start record must already be flushed at the default Info level.
        let held = d.state.lock().unwrap();
        let first = d.checkpoint().unwrap();
        let path = root.join("logs/motion-studio.log");
        let deadline = Instant::now() + Duration::from_secs(5);
        let observed = loop {
            if std::fs::read_to_string(&path).unwrap()
                .contains("diagnostics start #1 checkpoint") {
                break true;
            }
            if Instant::now() >= deadline { break false; }
            std::thread::sleep(Duration::from_millis(5));
        };
        // Always release before asserting, including a failing negative control.
        drop(held);
        assert!(observed, "reporter start was not flushed before snapshot");
        assert!(first.recv_timeout(Duration::from_secs(5)).unwrap().is_err());
        // Unlike the first job, automatic checkpoints have no reply recipient.
        slow(&d);
        let healthy = root.join("healthy.json");
        d.export(healthy.clone()).unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        assert_eq!(d.status()["localWriteErrors"], 2);
        assert_eq!(std::fs::read(&blocked).unwrap(), b"keep existing bytes");
        let report: Value = serde_json::from_slice(&std::fs::read(&healthy).unwrap()).unwrap();
        assert_eq!(report["schemaVersion"], 1);
        assert_eq!(report["reporting"]["localWriteErrors"], 2);
        // A following serial job proves both failure logs have completed and
        // the successful export did not terminate the reporter.
        d.export(healthy).unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "diagnostics::tests::reporter_logs_before_snapshot_and_preserves_unobserved_write_failures"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(), "{}\n{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let text = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    assert!(text.contains("INFO [motion-diagnostics]"));
    for request in [1, 2] {
        let marker = format!("diagnostics failed #{request} checkpoint ");
        let line = text.lines().find(|line| line.contains(&marker)).unwrap();
        assert!(line.contains("ERROR [motion-diagnostics]"));
        assert!(line.contains(&format!("path={} reason=", root.path().join("occupied").join("report.json").display())));
        assert!(line.split(&marker).nth(1).unwrap().split("ms ").next().unwrap().parse::<u128>().is_ok());
        assert!(String::from_utf8_lossy(&output.stderr).contains(&marker));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(&marker));
    }
    assert!(!text.contains("diagnostics failed #3 export"));
    assert!(!text.contains("diagnostics done"), "Debug must remain filtered by default");
}
#[derive(Default)]
struct Recording(Mutex<Vec<Vec<u8>>>);
impl sentry::Transport for Recording {
    fn send_envelope(&self,envelope:sentry::Envelope) {
        let mut bytes=vec![];envelope.to_writer(&mut bytes).unwrap();self.0.lock().unwrap().push(bytes);
    }
}

#[test]
fn reporter_shutdown_logs_incomplete_sdk_flush_without_calling_it_an_error() {
    const CHILD: &str = "MOTION_DIAGNOSTICS_SHUTDOWN_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        struct Unflushed;
        impl sentry::Transport for Unflushed {
            fn send_envelope(&self, _: sentry::Envelope) {}
            fn flush(&self, _: Duration) -> bool { false }
        }
        // Exercise the real SDK's false return with a controlled transport;
        // no network request or timing-dependent network timeout is needed.
        let factory: Arc<dyn sentry::TransportFactory> = Arc::new(Arc::new(Unflushed));
        let d = Diagnostics::with_transport(None,
            Some("https://public-key@invalid.example/7".into()), Some(factory));
        d.set_enabled(true).unwrap();
        slow(&d);
        d.checkpoint().unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        assert_eq!(d.status()["queuedUploads"], 1);
        drop(d);
        let path = root.join("logs/motion-studio.log");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !std::fs::read_to_string(&path).unwrap().contains("diagnostics reporter stopped") {
            assert!(Instant::now() < deadline, "reporter did not finish shutdown");
            std::thread::sleep(Duration::from_millis(5));
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "diagnostics::tests::reporter_shutdown_logs_incomplete_sdk_flush_without_calling_it_an_error"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(), "{}\n{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let text = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let started = text.find("diagnostics reporter started").unwrap();
    let stopping = text.find("diagnostics reporter stopping reason=").unwrap();
    let failed = text.find("diagnostics reporter upload flush incomplete wait_limit_ms=500 elapsed_ms=").unwrap();
    let stopped = text.find("diagnostics reporter stopped").unwrap();
    assert!(started < stopping && stopping < failed && failed < stopped);
    assert!(text.lines().any(|line| line.contains("WARN [motion-diagnostics]") && line.contains("upload flush incomplete")));
    assert!(!text.contains("ERROR"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("diagnostics reporter"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("diagnostics reporter"));
    assert!(!text.contains("public-key"));
}

#[test]
fn consent_gates_real_sdk_envelopes_and_attachments_and_upload_is_rate_limited() {
    let output=Arc::new(Recording::default());
    let factory:Arc<dyn sentry::TransportFactory>=Arc::new(output.clone());
    let d=Diagnostics::with_transport(None,Some("https://public-key@invalid.example/7".into()),Some(factory));
    slow(&d);
    d.checkpoint().unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert!(output.0.lock().unwrap().is_empty());
    d.set_enabled(true).unwrap();
    d.checkpoint().unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    let messages=output.0.lock().unwrap();assert_eq!(messages.len(),1);
    let body=String::from_utf8(messages[0].clone()).unwrap();
    assert!(body.contains("motion-studio-stalls.json"));assert!(body.contains("application/json"));
    assert!(body.contains("queueWaitUs"));assert!(!body.contains("server_name"));assert!(!body.contains("\"user\""));
    drop(messages);
    slow(&d);d.checkpoint().unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert_eq!(output.0.lock().unwrap().len(),1);
    d.set_enabled(false).unwrap();
    assert_eq!(d.status()["enabled"],false);
    assert_eq!(d.status()["queuedUploads"],1);
}
#[test]
fn fast_activity_does_not_count_as_a_stall_or_claim_a_measured_gpu_duration() {
    let d=Diagnostics::new(None,None);
    d.record(0,"response_poll","edit_async",Duration::from_millis(1),None,None,Default::default());
    let report=d.report();assert_eq!(report["totalStalls"],0);
    assert_eq!(report["recent"][0]["queueWaitUs"],Value::Null);
    assert_eq!(report["recent"][0]["workUs"],Value::Null);
    assert!(report["gpu"].is_null());
}

#[test]
fn actual_engine_queue_records_waiting_separately_and_reply_context_preserves_the_action() {
    let root = tempfile::tempdir().unwrap();
    let engine = crate::engine::Engine::start(root.path().join("project"), motion_desktop_media::platform(), 8 << 30).unwrap();
    // Recording intentionally drops an event rather than waiting for the report
    // lock. Observe a bounded series of real queued requests, not an assumption
    // that one event must survive a concurrent automatic checkpoint.
    for _ in 0..8 {
        let (entered, waiting) = std::sync::mpsc::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let first = engine.request(|reply| crate::engine::Command::TestWait { entered, release: blocked, reply }).unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        engine.diagnostics().set_context(Context { action: "mask", layers: 1, ..Default::default() });
        let state = engine.request(crate::engine::Command::State).unwrap();
        let (entered, barrier) = std::sync::mpsc::channel();
        let (finish, blocked) = std::sync::mpsc::channel();
        let last = engine.request(|reply| crate::engine::Command::TestWait { entered, release: blocked, reply }).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        release.send(()).unwrap();
        first.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        state.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        // This worker barrier is after State's timing record. Reading the report
        // now cannot itself race and cause the record being tested to be dropped.
        barrier.recv_timeout(Duration::from_secs(5)).unwrap();
        let report = engine.diagnostics().report();
        finish.send(()).unwrap();
        last.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        if let Some(record) = report["recent"].as_array().unwrap().iter().find(|event| {
            event["category"] == "worker" && event["operation"] == "state"
                && event["queueWaitUs"].as_u64().unwrap_or(0) >= 10_000
        }) {
            assert_eq!(record["context"]["action"], "mask");
            assert!(record["workUs"].is_u64());
            return;
        }
    }
    panic!("no real queued State timing was observed within eight attempts");
}

#[test]
fn in_flight_and_queue_depth_track_a_blocked_worker() {
    let root = tempfile::tempdir().unwrap();
    let engine = crate::engine::Engine::start(
        root.path().join("project"), motion_desktop_media::platform(), 8 << 30,
    ).unwrap();
    let (entered, waiting) = std::sync::mpsc::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let first = engine.request(|reply| crate::engine::Command::TestWait {
        entered, release: blocked, reply,
    }).unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    // Capture assertions before release, but always unblock before panicking so
    // Engine::drop cannot turn a failing regression test into a hung test suite.
    let flight = engine.in_flight();
    let state = engine.request(crate::engine::Command::State).unwrap();
    let depth = engine.queue_depth();
    release.send(()).unwrap();
    first.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    state.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert_eq!(flight.as_ref().map(|(op, _)| op.as_str()), Some("test_wait"));
    assert!(depth >= 1);
    // Replies are sent inside handle; the record and clear intentionally follow.
    let deadline = Instant::now() + Duration::from_secs(5);
    while engine.in_flight().is_some() && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(engine.queue_depth(), 0);
    assert!(engine.in_flight().is_none());
}

#[test]
fn timing_report_contention_cannot_erase_context_or_gpu_identity() {
    let d = Diagnostics::new(None, None);
    let held = d.state.lock().unwrap();
    d.set_context(Context { action: "edit", layers: 7, frame: 12.5, ..Default::default() });
    assert_eq!(d.context().action, "edit");
    assert_eq!(d.context().layers, 7);
    d.set_gpu(wgpu::AdapterInfo { name: "test adapter".into(), vendor: 0, device: 0,
        device_type: wgpu::DeviceType::Other, driver: String::new(), driver_info: String::new(),
        backend: wgpu::Backend::Noop, ..wgpu::AdapterInfo::new(wgpu::DeviceType::Other, wgpu::Backend::Noop) });
    drop(held);
    assert_eq!(d.report()["gpu"]["name"], "test adapter");
}

#[test]
fn oversized_report_replies_with_failure_and_reporter_remains_usable() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("report.json");
    let d = Diagnostics::new(None, None);
    let huge: &'static str = Box::leak("x".repeat(MAX_REPORT_BYTES).into_boxed_str());
    d.record(1, "worker", huge, Duration::from_millis(1), None, None, Default::default());
    let error = d.export(path.clone()).unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err();
    assert!(error.contains("byte budget"));
    assert_eq!(d.status()["localWriteErrors"], 1);
    assert!(!path.exists());
    d.state.lock().unwrap().recent.clear();
    d.export(path.clone()).unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert!(path.exists());
}

#[test]
fn poisoned_diagnostic_state_preserves_context_and_snapshot_with_visible_recovery() {
    const CHILD: &str = "MOTION_DIAGNOSTICS_POISON_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let diagnostics = Diagnostics::new(None,None);
        for target in [0,1] {
            let d = diagnostics.clone();
            assert!(std::thread::spawn(move || {
                if target == 0 { let _guard = d.context.lock().unwrap(); panic!("controlled context poison"); }
                else { let _guard = d.state.lock().unwrap(); panic!("controlled state poison"); }
            }).join().is_err());
        }
        for _ in 0..120 {
            diagnostics.set_context(Context { action:"observed_test",frame:7.,..Default::default() });
            assert_eq!(diagnostics.context().frame,7.);
            assert_eq!(diagnostics.context().action,"observed_test");
            assert_eq!(diagnostics.report()["schemaVersion"],1);
        }
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in ["diagnostics.context.read","diagnostics.context.write","diagnostics.state.report"] {
            let records = text.lines().filter(|l| l.contains(&format!("site={site} "))).collect::<Vec<_>>();
            assert_eq!(records.len(),2,"{site}: {text}");
            assert!(records[0].contains("WARN") && records[1].contains("ERROR"));
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","diagnostics::tests::poisoned_diagnostic_state_preserves_context_and_snapshot_with_visible_recovery"]).env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
    assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("site=diagnostics.state.report"));
}
