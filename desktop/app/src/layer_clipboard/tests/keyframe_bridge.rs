use super::*;
use crate::engine::{Command as Request, Engine};
use crate::layer_clipboard::controller::{Clipboard, Completion, Domain, Intent, Status};
use motion_core::{KeyframeSelection, KeyframeTarget, Property};

fn target(object: u64) -> KeyframeTarget {
    KeyframeTarget::Property {
        object,
        property: Property::Position,
        axis: None,
    }
}
fn selection(frames: Vec<u32>) -> Vec<KeyframeSelection> {
    vec![KeyframeSelection {
        target: target(7),
        frames,
    }]
}
fn document(root: &Path, count: u32) -> motion_core::Project {
    let mut project = motion_core::Project::new(64, 64, 30, 12000).unwrap();
    for id in [7, 9] {
        project.layers.push(motion_core::Layer::solid(
            id, "Layer", [32.; 2], [16.; 3], [1.; 4],
        ));
    }
    project.layers[0].transform.position.keys = (0..count)
        .map(|frame| motion_core::Keyframe {
            frame: i32::try_from(frame).unwrap(),
            value: [frame as f32 / 10.; 3],
            ease: motion_core::Ease::Linear,
            curve: None,
            spatial: None,
        })
        .collect();
    motion_core::storage::save(root, &project).unwrap();
    project
}
fn read_keys(engine: &Engine, frames: Vec<u32>) -> KeyframeRead {
    engine
        .call(|reply| Request::KeyframeClipboardRead {
            selections: selection(frames),
            reply,
        })
        .unwrap()
}
fn finish(clipboard: &mut Clipboard, engine: &Engine) -> Result<Completion, String> {
    let began = Instant::now();
    loop {
        if let Some(result) = clipboard.poll(engine, true) {
            return result;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "stuck at {:?}",
            clipboard.phase()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn ready(clipboard: &mut Clipboard, engine: &Engine) {
    let began = Instant::now();
    while clipboard.phase() != Some("ready") {
        assert!(clipboard.poll(engine, false).is_none());
        assert!(began.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn begin(
    clipboard: &mut Clipboard,
    engine: &Engine,
    intent: Intent,
    frames: Vec<u32>,
    targets: Vec<KeyframeTarget>,
) {
    assert!(clipboard
        .start_keyframes(engine, intent, selection(frames), targets)
        .unwrap());
}
fn edit(engine: &Engine, command: motion_core::Command) -> Value {
    engine
        .edit(serde_json::to_string(&command).unwrap())
        .unwrap()
}

#[test]
fn typed_transport_pastes_payload_larger_than_json_limit_and_undoes_once() {
    let root = tempfile::tempdir().unwrap();
    let original = document(root.path(), 6000);
    let engine = super::engine(root.path());
    let read = read_keys(&engine, (0..6000).collect());
    let clipboard = read.clipboard.unwrap();
    let command = motion_core::Command::PasteKeyframes {
        clipboard: Box::new((*clipboard).clone()),
        targets: vec![target(9)],
        at_frame: Some(6000),
    };
    let json = serde_json::to_string(&command).unwrap();
    assert!(
        json.len() > crate::engine::REQUEST_LIMIT,
        "actual payload={} limit={}",
        json.len(),
        crate::engine::REQUEST_LIMIT
    );
    assert_eq!(
        engine.edit(json).unwrap_err(),
        "edit request exceeds 256 KiB"
    );
    let result = engine
        .call(|reply| Request::ClipboardCommit {
            edit: Edit::PasteKeyframes {
                stamp: read.stamp,
                clipboard,
                targets: vec![target(9)],
                at_frame: Some(6000),
                cancel: Arc::new(AtomicBool::new(false)),
            },
            reply,
        })
        .unwrap();
    assert_eq!(result["clipboard_status"], "applied");
    let keys = result["project"]["layers"][1]["transform"]["position"]["keys"]
        .as_array()
        .unwrap();
    assert_eq!(keys.len(), 6000);
    assert_eq!(keys[0]["frame"], 6000);
    assert_eq!(keys[5999]["frame"], 11999);
    assert_eq!(
        engine.history(0).unwrap()["project"],
        serde_json::to_value(&original).unwrap()
    );
    let redone = engine.history(1).unwrap();
    assert_eq!(redone["project"], result["project"]);
    engine.save().unwrap();
    assert_eq!(
        serde_json::to_value(motion_core::storage::load(root.path()).unwrap()).unwrap(),
        redone["project"]
    );
}

#[test]
fn separate_caches_survive_failed_copy_and_share_one_pending_slot() {
    let root = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    document(root.path(), 2);
    let engine = super::engine(root.path());
    let mut clipboard = Clipboard::default();
    assert!(!clipboard
        .start_keyframes(
            &engine,
            Intent::Paste { in_place: true },
            vec![],
            vec![target(9)]
        )
        .unwrap());
    assert!(!clipboard
        .start_keyframes(&engine, Intent::Copy, vec![], vec![])
        .unwrap());
    assert!(clipboard
        .start(&engine, Intent::Copy, vec![7], cache.path().into())
        .unwrap());
    assert!(!clipboard
        .start_keyframes(&engine, Intent::Copy, selection(vec![0]), vec![])
        .unwrap());
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().domain,
        Domain::Layers
    );
    begin(&mut clipboard, &engine, Intent::Copy, vec![0, 1], vec![]);
    assert!(!clipboard
        .start(&engine, Intent::Cut, vec![7], cache.path().into())
        .unwrap());
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().status,
        Status::Copied
    );
    assert!(clipboard.available() && clipboard.keyframes_available());
    begin(&mut clipboard, &engine, Intent::Copy, vec![11999], vec![]);
    assert!(finish(&mut clipboard, &engine)
        .err()
        .unwrap()
        .contains("keyframe"));
    assert!(clipboard.available() && clipboard.keyframes_available());
    edit(
        &engine,
        motion_core::Command::SetVector {
            object: 7,
            property: Property::Position,
            frame: 0,
            value: [99.; 3],
        },
    );
    engine.seek(30.).unwrap();
    begin(
        &mut clipboard,
        &engine,
        Intent::Paste { in_place: false },
        vec![],
        vec![target(9)],
    );
    let pasted = finish(&mut clipboard, &engine).unwrap();
    assert_eq!(pasted.domain, Domain::Keyframes);
    let state = pasted.state.unwrap();
    let keys = &state["project"]["layers"][1]["transform"]["position"]["keys"];
    assert_eq!(keys[0]["frame"], 30);
    assert_eq!(keys[0]["value"], serde_json::json!([0., 0., 0.]));
    assert_eq!(keys[1]["frame"], 31);
    assert!(clipboard
        .start(
            &engine,
            Intent::Paste { in_place: true },
            vec![],
            cache.path().into()
        )
        .unwrap());
    let pasted_layer = finish(&mut clipboard, &engine).unwrap();
    assert_eq!(pasted_layer.domain, Domain::Layers);
    assert_eq!(
        pasted_layer.state.unwrap()["project"]["layers"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn cut_caches_before_commit_and_superseded_cut_never_deletes_changed_source() {
    let root = tempfile::tempdir().unwrap();
    document(root.path(), 2);
    let engine = super::engine(root.path());
    let mut clipboard = Clipboard::default();
    begin(&mut clipboard, &engine, Intent::Cut, vec![0, 1], vec![]);
    ready(&mut clipboard, &engine);
    assert!(clipboard.keyframes_available());
    assert_eq!(
        engine.state().unwrap()["project"]["layers"][0]["transform"]["position"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let changed = edit(
        &engine,
        motion_core::Command::SetVector {
            object: 7,
            property: Property::Position,
            frame: 0,
            value: [99.; 3],
        },
    );
    let completed = finish(&mut clipboard, &engine).unwrap();
    assert_eq!(completed.status, Status::Superseded);
    assert!(completed.state.is_none());
    let after = engine.state().unwrap();
    assert_eq!(after["project"], changed["project"]);
    assert_eq!(after["revision"], changed["revision"]);
    begin(
        &mut clipboard,
        &engine,
        Intent::Paste { in_place: true },
        vec![],
        vec![target(9)],
    );
    let pasted = finish(&mut clipboard, &engine).unwrap().state.unwrap();
    assert_eq!(
        pasted["project"]["layers"][1]["transform"]["position"]["keys"][0]["value"],
        serde_json::json!([0., 0., 0.])
    );
    begin(&mut clipboard, &engine, Intent::Cut, vec![0, 1], vec![]);
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().status,
        Status::Applied
    );
    assert!(
        engine.state().unwrap()["project"]["layers"][0]["transform"]["position"]["keys"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(engine.history(0).unwrap()["project"], pasted["project"]);
}

#[test]
fn cancellation_waits_for_reply_and_queued_commit_is_a_noop() {
    let root = tempfile::tempdir().unwrap();
    let original = document(root.path(), 2);
    let engine = super::engine(root.path());
    let mut clipboard = Clipboard::default();
    begin(&mut clipboard, &engine, Intent::Copy, vec![0, 1], vec![]);
    finish(&mut clipboard, &engine).unwrap();
    let (entered, ready_wait) = std::sync::mpsc::channel();
    let (release, proceed) = std::sync::mpsc::channel();
    let wait = engine
        .request(|reply| Request::TestWait {
            entered,
            release: proceed,
            reply,
        })
        .unwrap();
    ready_wait.recv_timeout(Duration::from_secs(10)).unwrap();
    begin(&mut clipboard, &engine, Intent::Cut, vec![0, 1], vec![]);
    clipboard.cancel();
    assert!(clipboard.poll(&engine, true).is_none());
    assert!(clipboard.busy());
    release.send(()).unwrap();
    wait.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().status,
        Status::Cancelled
    );
    begin(
        &mut clipboard,
        &engine,
        Intent::Paste { in_place: true },
        vec![],
        vec![target(9)],
    );
    ready(&mut clipboard, &engine);
    let (entered, ready_wait) = std::sync::mpsc::channel();
    let (release, proceed) = std::sync::mpsc::channel();
    let wait = engine
        .request(|reply| Request::TestWait {
            entered,
            release: proceed,
            reply,
        })
        .unwrap();
    ready_wait.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(clipboard.poll(&engine, true).is_none());
    assert!(clipboard.committing());
    clipboard.cancel();
    assert!(clipboard.poll(&engine, true).is_none());
    release.send(()).unwrap();
    wait.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().status,
        Status::Cancelled
    );
    assert_eq!(
        engine.state().unwrap()["project"],
        serde_json::to_value(original).unwrap()
    );
}

#[test]
fn late_cancellation_preserves_successful_ack_and_frozen_data_survives_source_engine() {
    let root = tempfile::tempdir().unwrap();
    document(root.path(), 2);
    let engine = super::engine(root.path());
    let mut clipboard = Clipboard::default();
    begin(&mut clipboard, &engine, Intent::Copy, vec![0, 1], vec![]);
    finish(&mut clipboard, &engine).unwrap();
    drop(engine);
    let destination = tempfile::tempdir().unwrap();
    document(destination.path(), 0);
    let engine = super::engine(destination.path());
    begin(
        &mut clipboard,
        &engine,
        Intent::Paste { in_place: true },
        vec![],
        vec![target(9)],
    );
    ready(&mut clipboard, &engine);
    assert!(clipboard.poll(&engine, true).is_none());
    assert!(clipboard.committing());
    let committed = engine.state().unwrap(); // FIFO barrier: commit has already finished.
    clipboard.cancel();
    let completed = finish(&mut clipboard, &engine).unwrap();
    assert_eq!(completed.status, Status::Applied);
    assert_eq!(completed.state.unwrap()["project"], committed["project"]);
    assert_eq!(
        committed["project"]["layers"][1]["transform"]["position"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn document_generation_rejects_cut_and_paste_even_when_root_and_revision_are_reused() {
    let root = tempfile::tempdir().unwrap();
    let original = document(root.path(), 2);
    let mut session = Session::new(
        original.clone(),
        root.path().into(),
        motion_desktop_media::platform(),
    )
    .unwrap();
    let read = read_keyframes(&session, selection(vec![0, 1])).unwrap();
    let clipboard = read.clipboard.unwrap();
    session
        .replace_project(
            motion_core::Engine::new(original.clone()).unwrap(),
            root.path().into(),
        )
        .unwrap();
    assert_eq!(read.stamp.revision, session.engine.revision());
    assert_ne!(read.stamp.generation, session.document_generation);
    for edit in [
        Edit::CutKeyframes {
            stamp: read.stamp.clone(),
            clipboard: clipboard.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
        },
        Edit::PasteKeyframes {
            stamp: read.stamp.clone(),
            clipboard,
            targets: vec![target(9)],
            at_frame: Some(30),
            cancel: Arc::new(AtomicBool::new(false)),
        },
    ] {
        assert_eq!(
            commit(&mut session, edit).unwrap()["clipboard_status"],
            "superseded"
        );
        assert_eq!(*session.engine.project(), original);
        assert_eq!(session.engine.revision(), 0);
    }
}

#[test]
fn typed_failures_and_queued_panic_reply_reach_default_error_outputs() {
    const NAME: &str = "layer_clipboard::tests::keyframe_bridge::typed_failures_and_queued_panic_reply_reach_default_error_outputs";
    const CHILD: &str = "MOTION_KEYFRAME_BRIDGE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let source = root.join("source");
        document(&source, 2);
        let engine = super::engine(&source);
        let failure = engine.call(|reply| Request::KeyframeClipboardRead {
            selections: vec![KeyframeSelection {
                target: target(404),
                frames: vec![0],
            }],
            reply,
        });
        let error = match failure {
            Err(error) => error,
            Ok(_) => panic!("missing source accepted"),
        };
        assert!(error.contains("phase=capture object=404"), "{error}");
        let read = read_keys(&engine, vec![0, 1]);
        let clipboard = read.clipboard.unwrap();
        let apply = |stamp| {
            engine.call(|reply| Request::ClipboardCommit {
                edit: Edit::PasteKeyframes {
                    stamp,
                    clipboard: clipboard.clone(),
                    targets: vec![target(9)],
                    at_frame: Some(30),
                    cancel: Arc::new(AtomicBool::new(false)),
                },
                reply,
            })
        };
        assert_eq!(apply(read.stamp).unwrap()["clipboard_status"], "applied");
        let read = engine
            .call(|reply| Request::KeyframeClipboardRead {
                selections: vec![],
                reply,
            })
            .unwrap();
        let before = engine.state().unwrap();
        assert!(apply(read.stamp)
            .unwrap_err()
            .contains("keyframe destination is occupied"));
        let after = engine.state().unwrap();
        assert_eq!(before["project"], after["project"]);
        assert_eq!(before["revision"], after["revision"]);

        let (entered, blocked) = std::sync::mpsc::channel();
        let (release, proceed) = std::sync::mpsc::channel();
        let wait = engine
            .request(|reply| Request::TestWait {
                entered,
                release: proceed,
                reply,
            })
            .unwrap();
        blocked.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(engine.in_flight().unwrap().0, "test_wait");
        crate::logging::flush();
        let waiting = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(waiting.lines().any(|line| line.contains("INFO")
            && line.contains("start #")
            && line.ends_with("test_wait")));
        let panic = engine
            .request(|reply| Request::TestPanic {
                during_prepare: false,
                reply,
            })
            .unwrap();
        let queued = engine
            .request(|reply| Request::KeyframeClipboardRead {
                selections: selection(vec![0]),
                reply,
            })
            .unwrap();
        release.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(20)).unwrap().unwrap();
        let primary = panic
            .recv_timeout(Duration::from_secs(20))
            .unwrap()
            .unwrap_err();
        // Queued commands intentionally disconnect when the serial worker exits.
        // Observe that boundary through the production poller, which preserves
        // the published primary failure rather than returning raw Disconnected.
        let began = Instant::now();
        let queued = loop {
            if let Some(result) = engine.poll_reply(&queued, "keyframe_clipboard_read") {
                break match result {
                    Err(error) => error,
                    Ok(_) => panic!("read after terminal panic succeeded"),
                };
            }
            assert!(began.elapsed() < Duration::from_secs(20));
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(primary.contains("injected worker panic"), "{primary}");
        assert_eq!(
            queued,
            format!("keyframe_clipboard_read response disconnected: {primary}")
        );
        drop(engine);
        crate::logging::flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_KEYFRAME_BRIDGE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(file.contains("logging initialized level=INFO"));
    assert!(file.lines().any(|line| line.contains("INFO")
        && line.contains("start #")
        && line.ends_with("keyframe_clipboard_read")));
    assert!(file.lines().any(|line| line.contains("INFO")
        && line.contains("start #")
        && line.ends_with("clipboard_commit")));
    assert!(!stderr.contains("start #"));
    for cause in [
        "phase=capture object=404",
        "keyframe destination is occupied",
        "injected worker panic",
    ] {
        assert!(file.contains(cause), "missing file cause: {cause}");
        assert!(stderr.contains(cause), "missing stderr cause: {cause}");
        assert!(!stdout.contains(cause), "stdout leaked {cause}");
    }
}
