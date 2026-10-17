use super::*;
use std::{
    fs,
    time::{Duration, Instant},
};

fn wait_stopped(engine: &Engine) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !engine.worker.as_ref().unwrap().is_finished() {
        assert!(Instant::now() < deadline, "panic cleanup did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn add(engine: &Engine) {
    engine.edit(serde_json::json!({"op":"add_shape","id":1,"name":"Unsaved", "shape":"rectangle","size":[20,20],"position":[32,32,0]}).to_string()).unwrap();
}

fn fail_shell(shell: &crate::Shell) {
    assert!(shell
        .engine
        .call_with_timeout(
            |reply| Command::TestPanic {
                during_prepare: false,
                reply
            },
            Duration::from_secs(5),
        )
        .is_err());
    wait_stopped(&shell.engine);
}

#[test]
fn stopped_renderer_can_close_without_attachment_or_a_gpu_modal() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell =
        crate::Shell::new(folder.path().join("project"), None, Some(crate::Locale::En)).unwrap();
    fail_shell(&shell);
    assert!(!shell.attached);
    shell.actions.push_back(crate::panels::Action::Save);
    let (sender, reply) = channel();
    drop(sender);
    shell.rendering.insert(0, reply);
    let (sender, reply) = channel();
    drop(sender);
    shell.editing = Some((Instant::now(), reply));
    let (sender, reply) = channel();
    drop(sender);
    shell.resizing = Some(reply);
    shell.present_due = true;
    shell.begin_close();
    assert!(shell.closing);
    assert!(shell.actions.is_empty());
    assert!(shell.rendering.is_empty());
    assert!(shell.editing.is_none());
    assert!(shell.resizing.is_none());
    assert!(!shell.present_due);
}

#[test]
fn stopped_renderer_preserves_node_graph_on_cancel_and_save_failure() {
    use crate::node_tasks::CloseChoice;
    for choice in [CloseChoice::Cancel, CloseChoice::Save, CloseChoice::Discard] {
        let folder = tempfile::tempdir().unwrap();
        let mut shell =
            crate::Shell::new(folder.path().join("project"), None, Some(crate::Locale::Zh))
                .unwrap();
        shell
            .panels
            .node_editor
            .add_node(motion_graph::NodeOp::Float { value: 2. }, [0., 0.])
            .unwrap();
        let expected = shell.panels.node_editor.snapshot().document;
        fail_shell(&shell);
        if choice == CloseChoice::Save {
            // Cancelling the file picker and failed writes both leave data intact.
            shell.close_failed_session(choice, None).unwrap();
            assert!(!shell.closing);
            assert!(shell
                .close_failed_session(choice, Some(folder.path().to_owned()))
                .is_err());
            assert!(!shell.closing);
        }
        let path = folder.path().join("recovered.msgraph");
        shell
            .close_failed_session(choice, Some(path.clone()))
            .unwrap();
        assert_eq!(shell.panels.node_editor.snapshot().document, expected);
        assert_eq!(shell.closing, choice != CloseChoice::Cancel);
        assert!(shell.node_close_prompt.is_none());
        if choice == CloseChoice::Save {
            assert_eq!(motion_graph::GraphDocument::load(&path).unwrap(), expected);
            assert!(!shell.panels.node_editor.dirty());
        }
    }
}

#[test]
fn panic_saves_last_valid_project_releases_session_lock_and_reports_the_failure() {
    for during_prepare in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        let engine =
            Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        engine.save().unwrap();
        let saved = fs::read(root.join("project.json")).unwrap();
        add(&engine);
        let before = engine.state().unwrap()["project"].clone();
        assert_eq!(
            motion_host::session::resource_info(&root).unwrap()["sessions"],
            1
        );
        let error = engine
            .call_with_timeout(
                |reply| Command::TestPanic {
                    during_prepare,
                    reply,
                },
                Duration::from_secs(5),
            )
            .unwrap_err();
        assert!(error.contains("native panic"), "{error}");
        assert!(error.contains("recovery JSON:"));
        wait_stopped(&engine);
        assert_eq!(engine.state().unwrap_err(), error);
        assert_eq!(engine.save().unwrap_err(), error);
        assert_eq!(engine.node_preview_available().err().unwrap(), error);
        assert!(engine.factory.lock().unwrap().is_none());
        assert_eq!(
            motion_host::session::resource_info(&root).unwrap()["sessions"],
            0
        );
        assert_eq!(fs::read(root.join("project.json")).unwrap(), saved);
        let recoveries: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .filter_map(|entry| {
                let path = entry.unwrap().path();
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".motion-recovery-")
                    .then_some(path)
            })
            .collect();
        assert_eq!(recoveries.len(), 1);
        let recovered: motion_core::Project =
            serde_json::from_slice(&fs::read(&recoveries[0]).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(&recovered).unwrap(), before);
        recovered.validate().unwrap();
        // The previous Engine can remain alive: both Session and lock are gone.
        let reopened =
            Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        assert!(reopened.state().unwrap()["project"]["layers"]
            .as_array()
            .unwrap()
            .is_empty());
        drop(reopened);
        motion_core::storage::save(&root, &recovered).unwrap();
        let restored = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
        assert_eq!(restored.state().unwrap()["project"], before);
    }
}

#[test]
fn queued_requests_disconnect_after_panic_and_ordinary_errors_keep_the_worker_alive() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("project");
    let engine = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
    assert!(engine.seek(f64::NAN).is_err());
    assert!(engine.state().is_ok());
    let delay = engine
        .request(|reply| Command::TestDelay {
            delay: Duration::from_millis(60),
            reply,
        })
        .unwrap();
    let failed = engine
        .request(|reply| Command::TestPanic {
            during_prepare: false,
            reply,
        })
        .unwrap();
    let queued = engine.request(Command::State).unwrap();
    delay.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    let error = failed
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap_err();
    assert!(error.contains("native panic"));
    assert!(matches!(
        queued.recv_timeout(Duration::from_secs(5)),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
    ));
    wait_stopped(&engine);
    assert_eq!(engine.edit_async("[]".into()).err().unwrap(), error);
}

#[test]
fn recovery_io_failure_is_reported_and_still_releases_the_session() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("project");
    let engine = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    engine.save().unwrap();
    let saved = fs::read(root.join("project.json")).unwrap();
    let blocked = folder.path().join("blocked");
    fs::write(&blocked, b"block recovery directory").unwrap();
    let error = engine
        .call_with_timeout(
            |reply| Command::TestRecoveryPanic {
                root: blocked,
                reply,
            },
            Duration::from_secs(5),
        )
        .unwrap_err();
    assert!(error.contains("recovery unavailable"));
    wait_stopped(&engine);
    assert_eq!(fs::read(root.join("project.json")).unwrap(), saved);
    assert_eq!(
        motion_host::session::resource_info(folder.path()).unwrap()["sessions"],
        0
    );
}

#[test]
fn reply_timeout_does_not_retry_or_turn_a_late_success_into_a_failed_worker() {
    let folder = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        folder.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let before = engine.state().unwrap();
    let error = engine
        .call_with_timeout(
            |reply| Command::TestDelay {
                delay: Duration::from_millis(60),
                reply,
            },
            Duration::from_millis(1),
        )
        .unwrap_err();
    assert!(error.contains("still pending"));
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    assert!(engine.failure.lock().unwrap().is_none());
}

#[test]
fn recovery_writer_is_bounded_and_reports_partial_writes() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("bounded");
    let mut file = fs::File::create(&path).unwrap();
    let mut writer = RecoveryWriter {
        file: &mut file,
        remaining: 4,
    };
    writer.write_all(b"1234").unwrap();
    assert!(writer.write_all(b"5").is_err());
    assert_eq!(fs::read(path).unwrap(), b"1234");
}

#[cfg(target_os = "windows")]
#[test]
fn attached_hidden_surface_and_factory_release_after_worker_panic() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        event_loop
            .create_window(winit::window::Window::default_attributes().with_visible(false))
            .unwrap(),
    );
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("surface");
    let engine = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    engine
        .attach_async(window, 64, 64)
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    assert_eq!(
        motion_host::session::resource_info(&root).unwrap()["graphics"],
        1
    );
    assert!(engine.node_preview_available().is_ok());
    // Reuse the process's one native event loop for concurrent scope coverage.
    crate::engine::scope_tests::exercise(&engine, folder.path());
    let error = engine
        .call_with_timeout(
            |reply| Command::TestPanic {
                during_prepare: false,
                reply,
            },
            Duration::from_secs(10),
        )
        .unwrap_err();
    assert!(error.contains("native panic"));
    wait_stopped(&engine);
    let resources = motion_host::session::resource_info(&root).unwrap();
    assert_eq!(resources["sessions"], 0);
    assert_eq!(resources["graphics"], 0);
    assert!(engine.factory.lock().unwrap().is_none());
}

#[test]
fn reply_poll_distinguishes_pending_success_failure_and_disconnection() {
    let folder = tempfile::tempdir().unwrap();
    let engine = Engine::start(folder.path().join("project"), motion_desktop_media::platform(), 8 << 30).unwrap();
    for operation in ["attach", "present", "resize", "freeze_movie"] {
        let (send, receive) = channel::<Result<u64, String>>();
        assert!(engine.poll_reply(&receive, operation).is_none());
        send.send(Ok(7)).unwrap();
        assert_eq!(engine.poll_reply(&receive, operation), Some(Ok(7)));
        send.send(Err("original failure".into())).unwrap();
        assert_eq!(engine.poll_reply(&receive, operation), Some(Err("original failure".into())));
        drop(send);
        let error = engine.poll_reply(&receive, operation).unwrap().unwrap_err();
        assert!(error.contains(operation));
        assert!(error.contains("disconnected"));
    }
    assert!(!engine.failed());
}

#[test]
fn impossible_memory_budget_is_rejected_before_creating_or_locking_the_project() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("not-created");
    let error = match Engine::start(project.clone(),motion_desktop_media::platform(),u64::MAX) {
        Ok(_) => panic!("out-of-range memory budget was accepted"),
        Err(error) => error,
    };
    assert!(error.contains("signed 64-bit preview budget"));
    assert!(!project.exists());
    let engine = Engine::start(project.clone(),motion_desktop_media::platform(),8<<30).unwrap();
    assert!(engine.state().is_ok());
    drop(engine);
    assert!(Engine::start(project,motion_desktop_media::platform(),8<<30).is_ok());
}
