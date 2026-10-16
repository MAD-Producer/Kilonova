use super::*;

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
#[derive(Default)]
struct Recording(Mutex<Vec<Vec<u8>>>);
impl sentry::Transport for Recording {
    fn send_envelope(&self,envelope:sentry::Envelope) {
        let mut bytes=vec![];envelope.to_writer(&mut bytes).unwrap();self.0.lock().unwrap().push(bytes);
    }
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
