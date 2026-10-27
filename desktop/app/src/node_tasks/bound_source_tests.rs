use super::*;
use std::{
    path::Path,
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};

struct Gate {
    key: String,
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    panic: bool,
}
static GATE: Mutex<Option<Gate>> = Mutex::new(None);
pub(super) fn before_load(job: &Job) {
    let gate = {
        let mut slot = GATE.lock().unwrap();
        if slot.as_ref().is_some_and(|g| {
            job.snapshot
                .layer_target
                .as_ref()
                .is_some_and(|t| t.key == g.key)
        }) {
            slot.take()
        } else {
            None
        }
    };
    if let Some(gate) = gate {
        gate.entered.send(()).unwrap();
        gate.release.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(!gate.panic, "controlled bound node source panic");
    }
}
fn target(root: &Path, key: &str) -> crate::node_layer::Target {
    crate::node_layer::Target {
        root: root.into(),
        composition: "main".into(),
        object: 7,
        effect: 9,
        key: key.into(),
        label: "private-bound-source-label".into(),
        creating: false,
    }
}
fn source(
    root: &Path,
    key: &str,
    kind: &str,
) -> (serde_json::Value, crate::node_layer::Target, GraphDocument) {
    std::fs::create_dir(root).unwrap();
    let document = crate::node_image_fixture::graph(crate::node_image_fixture::image());
    let snapshot = NodeEditor::new(document.clone()).snapshot();
    let target = target(root, key);
    let mut source = crate::node_layer::source(&snapshot, &target).unwrap();
    if kind == "bad_png" {
        let bytes = b"independent invalid PNG fixture";
        let sha256 = motion_core::storage::resource_sha256(bytes);
        let resource = motion_core::NodeResource {
            path: format!("assets/node-{sha256}.png"),
            bytes: bytes.len() as u64,
            sha256: sha256.clone(),
        };
        source["document"]["images"] =
            serde_json::json!({sha256.clone():{"width":2,"height":2,"resource":resource}});
        source["document"]["nodes"][0]["operation"]["image"] = sha256.into();
        let mut stage = motion_core::storage::NodeResourceStage::new(root).unwrap();
        stage.stage(&resource, bytes).unwrap();
        stage.keep_resources();
    } else if kind != "missing" {
        let mut stage = motion_core::storage::NodeResourceStage::new(root).unwrap();
        for resource in motion_core::node_resources::source_resources(&source).unwrap() {
            let image = document.images.get(&resource.sha256).unwrap();
            stage.stage(&resource, image.png()).unwrap();
        }
        stage.keep_resources();
    }
    (source, target, document)
}
fn poll(tasks: &mut NodeTasks, editor: &mut NodeEditor) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while tasks.busy() {
        assert!(
            Instant::now() < deadline,
            "bound source job never completed"
        );
        tasks.poll(editor, Locale::En);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn cancelled_node_jobs_and_unconsumed_results_share_one_bounded_capacity() {
    let root = tempfile::tempdir().unwrap();
    let (source, target, document) =
        source(&root.path().join("source"), "queue-bound-limit", "success");
    let mut editor = NodeEditor::default();
    let before = editor.snapshot();
    let mut tasks = NodeTasks::start().unwrap();
    let (entered, entered_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    *GATE.lock().unwrap() = Some(Gate {
        key: target.key.clone(),
        entered,
        release: release_rx,
        panic: false,
    });
    tasks
        .open_bound(before.clone(), source.clone(), target.clone(), Locale::En)
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    tasks.cancel(&mut editor);
    // The worker is still inside the first request. Clearing UI pending state
    // cannot free any of the snapshots retained by the worker or its queue.
    for _ in 1..16 {
        tasks
            .open_bound(before.clone(), source.clone(), target.clone(), Locale::En)
            .unwrap();
        tasks.cancel(&mut editor);
    }
    let next = tasks.next;
    assert!(tasks.pending.is_empty());
    assert_eq!(tasks.queued.load(Ordering::Acquire), 16);
    assert_eq!(
        tasks
            .open_bound(before.clone(), source.clone(), target.clone(), Locale::En)
            .unwrap_err(),
        "node operation queue is full"
    );
    assert_eq!(tasks.next, next);
    assert_eq!(editor.snapshot().document, before.document);
    release.send(()).unwrap();
    let completions: Vec<_> = (0..16)
        .map(|_| {
            let completion = tasks.receive.recv_timeout(Duration::from_secs(10)).unwrap();
            assert!(matches!(completion.outcome, Outcome::Cancelled));
            completion
        })
        .collect();
    // Receiving alone does not release owned results: capacity is returned
    // only when each completion (and its retained snapshot) is dropped.
    assert_eq!(tasks.queued.load(Ordering::Acquire), 16);
    assert!(tasks
        .open_bound(before.clone(), source.clone(), target.clone(), Locale::En)
        .is_err());
    assert_eq!(tasks.next, next);
    drop(completions);
    assert_eq!(tasks.queued.load(Ordering::Acquire), 0);
    tasks
        .open_bound(before, source, target, Locale::En)
        .unwrap();
    poll(&mut tasks, &mut editor);
    assert_eq!(tasks.queued.load(Ordering::Acquire), 0);
    assert_eq!(editor.snapshot().document, document);
}

#[test]
fn bound_image_loading_publishes_state_before_blocking_and_preserves_errors_and_stale_edits() {
    const CHILD: &str = "MOTION_BOUND_SOURCE_PROOF_CHILD";
    const NAME: &str = "node_tasks::bound_source_tests::bound_image_loading_publishes_state_before_blocking_and_preserves_errors_and_stale_edits";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut tasks = NodeTasks::start().unwrap();
        for kind in [
            "cancel",
            "edited",
            "target_changed",
            "panic",
            "missing",
            "bad_png",
            "wrong_dimensions",
            "oversized_error",
            "success",
        ] {
            let (mut source, target, document) = source(&root.join(kind), kind, kind);
            if kind == "oversized_error" {
                source["options"]["max_nodes"] =
                    format!("{}bound-error-tail", "bound-error-start".repeat(256)).into();
            }
            if kind == "wrong_dimensions" {
                let id = document.images.keys().next().unwrap();
                source["document"]["images"][id]["width"] = 1.into();
            }
            if matches!(kind, "missing" | "bad_png") {
                let error = crate::node_layer::load_document(&source, &target.root).unwrap_err();
                let cause = std::error::Error::source(&error).unwrap();
                if kind == "missing" {
                    assert!(
                        matches!(cause.downcast_ref::<motion_core::Error>().unwrap(), motion_core::Error::NodeResourceIo {phase:"resolve",source,..} if source.kind() == std::io::ErrorKind::NotFound)
                    );
                } else {
                    assert!(
                        matches!(cause.downcast_ref::<motion_graph::Error>().unwrap(), motion_graph::Error::Effect {cause,..} if matches!(cause.as_ref(),motion_effects::Error::ImageResource {phase:"node_image_decode",..}))
                    );
                }
            }
            let mut editor = NodeEditor::default();
            let before = editor.snapshot();
            let block = matches!(kind, "cancel" | "edited" | "target_changed" | "panic");
            let (entered, entered_rx) = mpsc::channel();
            let (release, release_rx) = mpsc::channel();
            if block {
                *GATE.lock().unwrap() = Some(Gate {
                    key: kind.into(),
                    entered,
                    release: release_rx,
                    panic: kind == "panic",
                });
            }
            let id = tasks.next;
            tasks
                .open_bound(before.clone(), source.clone(), target.clone(), Locale::Zh)
                .unwrap();
            if block {
                entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                assert!(tasks.busy());
                assert_eq!(tasks.pending.get(&id).unwrap().0, Kind::BoundOpen);
                assert_eq!(editor.snapshot().document, before.document);
                let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
                assert!(file.contains(&format!(
                    "node start #{id} BoundOpen epoch={} revision={} object=Some(7) effect=Some(9)",
                    before.epoch, before.revision
                )));
                if kind == "cancel" {
                    tasks.cancel(&mut editor);
                }
                if kind == "edited" {
                    editor
                        .add_node(motion_graph::NodeOp::Float { value: 0.7 }, [0., 0.])
                        .unwrap();
                }
                if kind == "target_changed" {
                    let state = serde_json::json!({"root":target.root,"project":{"composition_id":"main","layers":[{"id":7,"effects":[{"id":9,"node_graph":{}}]}]}});
                    tasks.reconcile_bound_open(&mut editor, &state, Locale::Zh);
                    assert!(!tasks.busy());
                }
                release.send(()).unwrap();
            }
            let retained = editor.snapshot();
            if kind == "oversized_error" {
                let completion = tasks.receive.recv_timeout(Duration::from_secs(10)).unwrap();
                assert_eq!(completion.id, id);
                let Outcome::Failed(error) = completion.outcome else {
                    panic!("invalid options did not fail")
                };
                assert!(error.len() > 1024 && error.contains("bound-error-tail"));
                tasks.pending.remove(&id);
                tasks.bound_open = None;
            } else if matches!(kind, "cancel" | "target_changed") {
                let completion = tasks.receive.recv_timeout(Duration::from_secs(10)).unwrap();
                assert_eq!(completion.id, id);
                assert!(matches!(completion.outcome, Outcome::Cancelled));
            } else {
                poll(&mut tasks, &mut editor);
            }
            if kind == "success" {
                assert_eq!(editor.snapshot().document, document);
                assert_eq!(editor.layer_target.as_ref().unwrap().object, 7);
                assert_ne!(editor.snapshot().epoch, before.epoch);
            } else {
                assert_eq!(
                    editor.snapshot().document,
                    retained.document,
                    "{kind} replaced editor"
                );
                assert_eq!(editor.snapshot().epoch, retained.epoch);
                assert_eq!(editor.snapshot().revision, retained.revision);
                assert!(editor.layer_target.is_none());
            }
        }
        // A real cleanup IO failure must not replace the failed operation's
        // primary cause or delete substituted/shared content.
        let cleanup = root.join("cleanup");
        std::fs::create_dir(&cleanup).unwrap();
        let record = |bytes: &[u8]| {
            let sha256 = motion_core::storage::resource_sha256(bytes);
            motion_core::NodeResource {
                path: format!("assets/node-{sha256}.png"),
                bytes: bytes.len() as u64,
                sha256,
            }
        };
        let owned = record(b"owned");
        let shared = record(b"shared");
        let primary: motion_core::Result<()> = (|| {
            let mut stage = motion_core::storage::NodeResourceStage::new(&cleanup)?;
            stage.stage(&owned, b"owned")?;
            std::fs::remove_file(cleanup.join(&owned.path))?;
            std::fs::create_dir(cleanup.join(&owned.path))?;
            std::fs::write(cleanup.join(&shared.path), b"broken")?;
            stage.stage(&shared, b"shared")
        })();
        assert!(primary
            .unwrap_err()
            .to_string()
            .contains("node resource content mismatch"));
        assert!(cleanup.join(&owned.path).is_dir());
        assert_eq!(
            std::fs::read(cleanup.join(&shared.path)).unwrap(),
            b"broken"
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    if let Some(evidence) = std::env::var_os("MOTION_NODE_OWNED_EVIDENCE_DIR") {
        let evidence = PathBuf::from(evidence);
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(evidence.join("bound-source-stdout.txt"), &output.stdout).unwrap();
        std::fs::write(evidence.join("bound-source-stderr.txt"), &output.stderr).unwrap();
        if root.path().join("logs/motion-studio.log").is_file() {
            std::fs::copy(
                root.path().join("logs/motion-studio.log"),
                evidence.join("bound-source-default.log"),
            )
            .unwrap();
        }
    }
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|l| l.contains("node error #") && l.contains(" BoundOpen "))
        .collect();
    assert_eq!(errors.len(), 5, "{file}");
    for error in errors {
        let request = error
            .split("node error #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        assert!(
            file.find(&format!("node start #{request} BoundOpen"))
                .unwrap()
                < file.find(error).unwrap()
        );
        assert!(error.contains("epoch=") && error.contains("revision=") && error.contains("ms "));
        assert!(stderr.contains(error.split("motion_studio::node_tasks: ").nth(1).unwrap()));
    }
    for cause in [
        "controlled bound node source panic",
        "node resource phase=resolve",
        "node source image phase=decode",
        "node source image metadata does not match its pixels: path=assets/node-",
    ] {
        assert!(file.contains(cause) && stderr.contains(cause));
        assert!(!stdout.contains(cause));
    }
    assert!(file.contains("node_image_decode"));
    assert!(file.contains("bound-error-start") && stderr.contains("bound-error-start"));
    assert!(
        !file.contains("bound-error-tail")
            && !stderr.contains("bound-error-tail")
            && !stdout.contains("bound-error-start")
    );
    assert_eq!(
        file.matches("node resource cleanup failed phase=remove")
            .count(),
        1
    );
    assert_eq!(
        stderr
            .matches("node resource cleanup failed phase=remove")
            .count(),
        1
    );
    assert!(!stdout.contains("node resource cleanup failed"));
    assert!(!stderr.contains("node start #") && !stderr.contains("logging initialized"));
    for text in [&file, &stderr, &stdout] {
        assert!(!text.contains("private-bound-source-label"));
    }
    assert!(!stdout.contains("node error #") && !stdout.contains("node start #"));
}
