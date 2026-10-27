use super::*;
use std::{
    fs,
    sync::Mutex,
    time::{Duration, Instant},
};
struct Gate {
    path: PathBuf,
    entered: Sender<()>,
    release: Receiver<()>,
    panic: bool,
}
static GATE: Mutex<Option<Gate>> = Mutex::new(None);
pub(super) fn before_import(job: &Job) {
    let gate = {
        let mut slot = GATE.lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|g| job.path.as_ref() == Some(&g.path))
        {
            slot.take()
        } else {
            None
        }
    };
    if let Some(gate) = gate {
        gate.entered.send(()).unwrap();
        gate.release.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(!gate.panic, "controlled image import panic");
    }
}
fn submit(tasks: &mut NodeTasks, editor: &NodeEditor, path: PathBuf) {
    tasks
        .submit(
            Action::ImportImage {
                snapshot: editor.snapshot(),
                path,
                position: [43., 57.],
            },
            Locale::En,
            false,
        )
        .unwrap();
}
fn drain(tasks: &mut NodeTasks, editor: &mut NodeEditor) {
    let began = Instant::now();
    while tasks.busy() {
        assert!(began.elapsed() < Duration::from_secs(10));
        tasks.poll(editor, Locale::En);
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn image_import_worker_observes_blocking_native_failures_cancel_stale_and_recovery() {
    const CHILD: &str = "MOTION_IMAGE_DROP_WORKER_CHILD";
    const NAME: &str = "node_tasks::image_import_tests::image_import_worker_observes_blocking_native_failures_cancel_stale_and_recovery";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut tasks = NodeTasks::start().unwrap();
        let mut editor = NodeEditor::default();
        let before = editor.snapshot();
        let mut causes = Vec::new();
        for (index, case) in ["cancel", "stale", "panic", "missing", "codec", "success"]
            .into_iter()
            .enumerate()
        {
            let path = root.join(format!("{case}.png"));
            if case != "missing" {
                fs::write(
                    &path,
                    if case == "codec" {
                        b"not PNG".to_vec()
                    } else {
                        crate::node_image_fixture::image().png().to_vec()
                    },
                )
                .unwrap();
            }
            let blocking = matches!(case, "cancel" | "stale" | "panic");
            let (entered, received) = mpsc::channel();
            let (release, release_rx) = mpsc::channel();
            if blocking {
                *GATE.lock().unwrap() = Some(Gate {
                    path: path.clone(),
                    entered,
                    release: release_rx,
                    panic: case == "panic",
                });
            }
            submit(&mut tasks, &editor, path.clone());
            if blocking {
                received.recv_timeout(Duration::from_secs(10)).unwrap();
                assert!(tasks.busy());
                crate::logging::flush();
                let log = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
                assert!(log.contains(&format!("node start #{} ImportImage", index + 1)));
                assert!(!log.contains(&format!("node done #{} ImportImage", index + 1)));
                let id = tasks.next;
                assert!(
                    tasks
                        .submit(
                            Action::ImportImage {
                                snapshot: editor.snapshot(),
                                path: path.clone(),
                                position: [0., 0.]
                            },
                            Locale::En,
                            false
                        )
                        .unwrap_err()
                        .contains("importing")
                );
                assert_eq!(id, tasks.next);
                if case == "cancel" {
                    tasks.cancel(&mut editor);
                }
                if case == "stale" {
                    editor.set_parameter_default("color", 0, 0.2).unwrap();
                }
                release.send(()).unwrap();
            }
            if case == "cancel" {
                let completion = tasks.receive.recv_timeout(Duration::from_secs(10)).unwrap();
                assert!(matches!(completion.outcome, Outcome::Cancelled));
                drop(completion);
            } else {
                drain(&mut tasks, &mut editor);
            }
            if case == "stale" {
                assert!(editor.undo());
                assert_eq!(editor.snapshot().document, before.document);
            } else if case == "success" {
                let after = editor.snapshot();
                assert_eq!(after.document.nodes.len(), before.document.nodes.len() + 1);
                assert_eq!(after.document.images.len(), 1);
                let node = after
                    .document
                    .nodes
                    .iter()
                    .find(|n| matches!(n.operation, motion_graph::NodeOp::ImageAsset { .. }))
                    .unwrap();
                assert_eq!(node.position, [43., 57.]);
                assert_eq!(node.display_name, "success");
                assert_eq!(after.preview_target.as_ref().unwrap().node, node.id);
                assert!(after.compile_preview().is_ok());
                assert!(editor.undo());
                assert_eq!(editor.snapshot().document, before.document);
                assert!(editor.redo());
                assert_eq!(editor.snapshot().document, after.document);
            } else {
                assert_eq!(editor.snapshot().document, before.document);
            }
            if matches!(case, "missing" | "codec") {
                causes.push((
                    index + 1,
                    motion_graph::GraphImage::import(&path)
                        .unwrap_err()
                        .to_string(),
                ));
            }
        }
        fs::write(
            root.join("causes.json"),
            serde_json::to_vec(&causes).unwrap(),
        )
        .unwrap();
        crate::logging::flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_NODE_IMAGE_DROP_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("worker"));
    fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
    fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let causes: Vec<(usize, String)> =
        serde_json::from_slice(&fs::read(root.join("causes.json")).unwrap()).unwrap();
    for (id, cause) in causes {
        let marker = format!("node error #{id} ImportImage");
        for sink in [&file, &stderr] {
            assert_eq!(sink.lines().filter(|l| l.contains(&marker)).count(), 1);
            assert!(sink.lines().any(|l| l.contains(&marker)
                && l.contains(&cause)
                && l.contains("phase=read_decode")));
        }
        assert!(
            file.find(&format!("node start #{id} ImportImage")).unwrap()
                < file.find(&marker).unwrap()
        );
        assert!(!stdout.contains(&marker) && !stdout.contains(&cause));
    }
    assert!(
        file.contains("node error #3 ImportImage")
            && stderr.contains("controlled image import panic")
    );
    for id in [1, 2, 6] {
        assert!(!file.contains(&format!("node error #{id} ImportImage")));
    }
    assert!(!stderr.contains("node start #") && !stdout.contains("node start #"));
}
