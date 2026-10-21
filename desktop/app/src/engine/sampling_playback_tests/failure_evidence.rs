//! Read bounded test metadata without asking a stalled worker for state.
use super::*;

fn wait_failure_snapshot(
    engine: &Engine,
    mode: &str,
    frame: f64,
    polls: u32,
    error: &str,
) -> Value {
    // Never ask a stalled worker for state, or wait on its observation lock.
    let (flight, lock_state) = match engine.in_flight.try_lock() {
        Ok(guard) => (*guard, "available"),
        Err(std::sync::TryLockError::Poisoned(error)) => (*error.into_inner(), "poisoned"),
        Err(std::sync::TryLockError::WouldBlock) => (None, "busy"),
    };
    serde_json::json!({
        "schema_version":1,"phase":"present_reply","mode":mode,"frame":frame,
        "present_polls":polls,"cause":error,"queue_depth":engine.queue_depth(),
        "flight_lock":lock_state,"in_flight":flight.map(|f|serde_json::json!({
            "request":f.request,"operation":f.operation,"elapsed_ms":f.started.elapsed().as_millis()
        }))
    })
}

pub(super) fn report_wait_failure(
    engine: &Engine,
    root: &std::path::Path,
    mode: &str,
    frame: f64,
    polls: u32,
    error: std::sync::mpsc::RecvTimeoutError,
) -> String {
    let snapshot = wait_failure_snapshot(engine, mode, frame, polls, &error.to_string());
    let saved = serde_json::to_vec_pretty(&snapshot)
        .map_err(std::io::Error::other)
        .and_then(|bytes| std::fs::write(root.join("failure.json"), bytes));
    format!("native playback wait failed: {error}; context={snapshot}; save={saved:?}")
}

#[test]
fn stalled_worker_evidence_preserves_cause_and_avoids_busy_observation_locks() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        root.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let (entered, entry) = channel();
    let (release, gate) = channel();
    let reply = engine
        .request(|reply| Command::TestWait {
            entered,
            release: gate,
            reply,
        })
        .unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    let error = reply.recv_timeout(Duration::from_millis(20)).unwrap_err();
    assert_eq!(error, std::sync::mpsc::RecvTimeoutError::Timeout);
    let primary = error.to_string();
    let message = report_wait_failure(&engine, root.path(), "controlled", 17., 2, error);
    assert!(message.contains(&primary));
    assert!(message.contains("save=Ok(())"));
    let snapshot: Value =
        serde_json::from_slice(&std::fs::read(root.path().join("failure.json")).unwrap()).unwrap();
    assert_eq!(snapshot["mode"], "controlled");
    assert_eq!(snapshot["frame"], 17.);
    assert_eq!(snapshot["present_polls"], 2);
    assert_eq!(snapshot["cause"], primary);
    assert_eq!(snapshot["flight_lock"], "available");
    assert_eq!(snapshot["in_flight"]["operation"], "test_wait");
    assert!(snapshot["in_flight"]["request"].as_u64().unwrap() > 0);
    {
        let _guard = engine.in_flight.lock().unwrap();
        let busy = wait_failure_snapshot(&engine, "controlled", 17., 2, &primary);
        assert_eq!(busy["flight_lock"], "busy");
        assert!(busy["in_flight"].is_null());
    }
    let blocked = root.path().join("not-a-directory");
    std::fs::write(&blocked, b"held file").unwrap();
    let message = report_wait_failure(&engine, &blocked, "controlled", 17., 2, error);
    assert!(message.contains(&primary));
    assert!(message.contains("save=Err("));
    release.send(()).unwrap();
    reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert!(!engine.failed());
}
