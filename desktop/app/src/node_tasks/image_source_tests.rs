//! Real source IO/codec failures through the existing serial worker and sinks.
use super::*;
use crate::{node_editor::Request, node_image_fixture as fixture};
use std::{
    fs,
    time::{Duration, Instant},
};

fn drain(tasks: &mut NodeTasks, editor: &mut NodeEditor) {
    let began = Instant::now();
    while tasks.busy() {
        assert!(began.elapsed() < Duration::from_secs(10));
        tasks.poll(editor, Locale::En);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn image_source_worker_preserves_default_error_sinks_state_and_explicit_recovery() {
    const CHILD: &str = "MOTION_NODE_IMAGE_SOURCE_CHILD";
    const NAME: &str = "node_tasks::image_source_tests::image_source_worker_preserves_default_error_sinks_state_and_explicit_recovery";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _guard = crate::logging::init(root.join("logs")).unwrap();
        let graph = fixture::graph(fixture::image());
        let image = graph.images.keys().next().unwrap();
        let mut editor = NodeEditor::default();
        let before = editor.snapshot();
        let mut tasks = NodeTasks::start().unwrap();
        let mut causes = Vec::new();
        for (index, payload) in ["YnJva2VuIHBpY3R1cmU=", "invalid-base64!"]
            .into_iter()
            .enumerate()
        {
            let path = root.join(format!("bad-{index}.msgraph"));
            let mut source = serde_json::to_value(&graph).unwrap();
            source["images"][image]["png"] = payload.into();
            fs::write(&path, source.to_string()).unwrap();
            let error = GraphDocument::load(&path).unwrap_err();
            assert_eq!(error.diagnostic().port, Some(format!("images.{image}")));
            causes.push(error.to_string());
            tasks
                .submit_to(editor.request(Request::Open).unwrap(), path, Locale::En)
                .unwrap();
            drain(&mut tasks, &mut editor);
            let after = editor.snapshot();
            assert_eq!(after.document, before.document);
            assert_eq!(after.epoch, before.epoch);
            assert_eq!(after.revision, before.revision);
            assert_eq!(after.source_path, before.source_path);
        }
        // The user explicitly opens a new, valid source; no product retry.
        let recovered = root.join("recovered.msgraph");
        graph.save(&recovered).unwrap();
        tasks
            .submit_to(
                editor.request(Request::Open).unwrap(),
                recovered,
                Locale::En,
            )
            .unwrap();
        drain(&mut tasks, &mut editor);
        assert_eq!(editor.snapshot().document, graph);
        assert_ne!(editor.snapshot().epoch, before.epoch);
        fs::write(
            root.join("causes.json"),
            serde_json::to_vec(&causes).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("recovery.json"),
            serde_json::json!({"opened": true, "images": editor.snapshot().document.images.len()})
                .to_string(),
        )
        .unwrap();
        crate::logging::flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = if let Some(parent) = std::env::var_os("MOTION_NODE_IMAGE_EVIDENCE_DIR") {
        PathBuf::from(parent).join(format!(
            "worker-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    } else {
        temporary.path().join("worker")
    };
    fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
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
    let causes: Vec<String> =
        serde_json::from_slice(&fs::read(root.join("causes.json")).unwrap()).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    for (index, cause) in causes.iter().enumerate() {
        let number = index + 1;
        let marker = format!("node error #{number} Open ");
        let start = format!("node start #{number} Open ");
        assert!(file.find(&start).unwrap() < file.find(&marker).unwrap());
        for sink in [&file, &stderr] {
            let errors: Vec<_> = sink.lines().filter(|line| line.contains(&marker)).collect();
            assert_eq!(errors.len(), 1);
            assert!(errors[0].contains(cause));
            assert!(errors[0].contains(&format!("bad-{index}.msgraph")));
            assert!(errors[0].contains("ms epoch=") && errors[0].contains("revision="));
            assert!(!sink.contains("YnJva2VuIHBpY3R1cmU=") && !sink.contains("invalid-base64!"));
        }
        assert!(!stdout.contains(cause) && !stdout.contains(&marker));
    }
    assert!(file.contains("node start #3 Open "));
    assert!(!file.contains("node error #3") && !stderr.contains("node error #3"));
    assert!(!stderr.contains("node start #") && !stdout.contains("node start #"));
    assert!(!stderr.contains("node worker failed") && !stderr.contains("panicked"));
}
