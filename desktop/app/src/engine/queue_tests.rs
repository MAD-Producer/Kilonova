use super::*;

fn engine(root: &std::path::Path) -> Arc<Engine> {
    Arc::new(Engine::start(root.to_owned(), motion_desktop_media::platform(), 8 << 30).unwrap())
}

fn blocked(engine: &Engine) -> (Sender<()>, std::sync::mpsc::Receiver<Result<Value, String>>) {
    let (entered, waiting) = channel();
    let (release, gate) = channel();
    let reply = engine
        .request(|reply| Command::TestWait {
            entered,
            release: gate,
            reply,
        })
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    (release, reply)
}

fn add() -> String {
    serde_json::json!({"op":"add_shape","id":1,"name":"Accepted","shape":"rectangle","size":[20,20],"position":[32,32,0]}).to_string()
}

#[test]
fn idle_shutdown_wakes_the_receiver_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..8 {
        let engine = engine(&dir.path().join(index.to_string()));
        engine.request_shutdown().unwrap();
        engine.request_shutdown().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !engine.worker.as_ref().unwrap().is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(engine.queue_depth(), 0);
    }
}

#[test]
fn poisoned_node_expectations_report_through_the_command_reply_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir.path().join("project"));
    std::thread::scope(|scope| {
        assert!(scope
            .spawn(|| {
                let _guard = engine.pending_node_expect.lock().unwrap();
                panic!("injected node expectation poison");
            })
            .join()
            .is_err());
    });
    assert!(engine
        .node_expect((1, 1))
        .unwrap_err()
        .contains("node expectation lock poisoned"));
    let error = engine.state().unwrap_err();
    assert!(error.contains("node expectation lock poisoned"), "{error}");
    assert!(engine.state().is_err());
}

#[test]
fn saturation_is_nonblocking_preserves_accepted_edits_and_coalesces_node_expectations() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir.path().join("project"));
    let (release, blocked) = blocked(&engine);
    for revision in 0..10_000 {
        engine.node_expect((revision, 7)).unwrap();
    }
    assert_eq!(engine.queue_depth(), 0);
    assert_eq!(
        *engine.pending_node_expect.lock().unwrap(),
        Some((9_999, 7))
    );
    let mut pending = Vec::new();
    for _ in 0..COMMAND_QUEUE_CAPACITY - 1 {
        pending.push(engine.request(Command::State).unwrap());
    }
    pending.push(engine.edit_async(add()).unwrap());
    assert_eq!(engine.queue_depth(), COMMAND_QUEUE_CAPACITY);
    let began = Instant::now();
    let error = engine
        .edit_async(serde_json::json!({"op":"rename","object":1,"name":"Rejected"}).to_string())
        .unwrap_err();
    assert!(began.elapsed() < Duration::from_secs(1));
    assert!(error.contains("command was not queued"));
    assert_eq!(engine.queue_depth(), COMMAND_QUEUE_CAPACITY);
    release.send(()).unwrap();
    blocked
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    for reply in pending {
        reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    }
    assert_eq!(
        engine.state().unwrap()["project"]["layers"][0]["name"],
        "Accepted"
    );
    assert!(engine.pending_node_expect.lock().unwrap().is_none());
    assert!(engine.history(0).unwrap()["project"]["layers"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn explicit_shutdown_drains_full_queue_and_releases_project_with_another_arc_alive() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let engine = engine(&root);
    let mcp_reader = engine.clone();
    let (release, blocked) = blocked(&engine);
    let mut pending = Vec::new();
    for _ in 0..COMMAND_QUEUE_CAPACITY - 2 {
        pending.push(engine.request(Command::State).unwrap());
    }
    pending.push(engine.edit_async(add()).unwrap());
    pending.push(engine.request(Command::Save).unwrap());
    assert_eq!(engine.queue_depth(), COMMAND_QUEUE_CAPACITY);
    engine.request_shutdown().unwrap();
    engine.request_shutdown().unwrap();
    assert!(engine.state().unwrap_err().contains("shutdown requested"));
    release.send(()).unwrap();
    blocked
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    for reply in pending {
        reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !engine.worker.as_ref().unwrap().is_finished() {
        assert!(Instant::now() < deadline, "worker did not drain and stop");
        std::thread::yield_now();
    }
    assert_eq!(engine.queue_depth(), 0);
    let reopened = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
    assert_eq!(
        reopened.state().unwrap()["project"]["layers"][0]["name"],
        "Accepted"
    );
    assert!(mcp_reader.state().is_err());
}
