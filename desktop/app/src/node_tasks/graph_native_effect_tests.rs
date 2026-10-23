use super::*;
use crate::node_editor::Request;
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
fn corrupt_export_reaches_default_sinks_and_explicit_clean_destination_recovers() {
    const CHILD: &str = "MOTION_GRAPH_EFFECT_EXPORT_TEST_CHILD";
    const NAME: &str = "node_tasks::graph_native_effect_tests::corrupt_export_reaches_default_sinks_and_explicit_clean_destination_recovers";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _guard = crate::logging::init(root.join("logs")).unwrap();
        let output = root.join("actual.msfx");
        let sibling = output.with_extension("msgraph");
        fs::write(&output, b"actual corrupt archive").unwrap();
        fs::write(&sibling, b"previous source").unwrap();
        let original_cause = match motion_effects::EffectPackage::open(&output) {
            Err(cause) => cause,
            Ok(_) => panic!("corrupt fixture unexpectedly opened as an effect package"),
        };
        assert!(matches!(original_cause, motion_effects::Error::Zip(_)));
        let expected = format!(
            "Publication: existing destination is not a valid effect package: {original_cause}"
        );
        fs::write(root.join("expected-cause.txt"), &expected).unwrap();
        let mut editor = NodeEditor::default();
        let before = editor.snapshot();
        let mut tasks = NodeTasks::start().unwrap();
        tasks
            .submit_to(
                editor.request(Request::ExportMsfx).unwrap(),
                output.clone(),
                Locale::En,
            )
            .unwrap();
        drain(&mut tasks, &mut editor);
        let after = editor.snapshot();
        assert_eq!(after.document, before.document);
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.epoch, before.epoch);
        assert_eq!(after.source_path, before.source_path);
        assert_eq!(fs::read(&output).unwrap(), b"actual corrupt archive");
        assert_eq!(fs::read(&sibling).unwrap(), b"previous source");
        // Removing the bad destination is an explicit user action in this
        // fixture; the product does not retry or overwrite it automatically.
        fs::remove_file(&output).unwrap();
        tasks
            .submit_to(
                editor.request(Request::ExportMsfx).unwrap(),
                output.clone(),
                Locale::En,
            )
            .unwrap();
        drain(&mut tasks, &mut editor);
        assert_eq!(editor.document(), &before.document);
        assert_eq!(
            editor.snapshot().source_path.as_ref(),
            Some(&sibling.canonicalize().unwrap())
        );
        assert_eq!(GraphDocument::load(&sibling).unwrap(), before.document);
        motion_effects::EffectPackage::open(&output).unwrap();
        crate::logging::flush();
        let text = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(text.contains(&expected));
        assert!(text.contains("node exported #2 "));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{stderr}\n{stdout}");
    let text = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let expected = fs::read_to_string(root.path().join("expected-cause.txt")).unwrap();
    assert!(text.contains("logging initialized level=INFO"));
    assert!(
        text.find("node start #1 Export ").unwrap() < text.find("node error #1 Export ").unwrap()
    );
    for sink in [&text, &stderr] {
        let lines: Vec<_> = sink
            .lines()
            .filter(|line| line.contains("node error #1 Export "))
            .collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains(&expected));
        assert!(
            lines[0].contains("ms epoch=")
                && lines[0].contains("revision=")
                && lines[0].contains("actual.msfx")
        );
        assert!(
            !sink.contains("node error #2 ")
                && !sink.contains("panic command=")
                && !sink.contains("node worker failed")
        );
    }
    assert_eq!(
        text.lines()
            .filter(|line| line.contains("node start #1 Export "))
            .count(),
        1
    );
    assert!(text.contains("node start #2 Export ") && text.contains("node exported #2 "));
    assert!(!stderr.contains("node start #") && !stderr.contains("node exported #"));
    assert!(
        !stdout.contains(&expected)
            && !stdout.contains("node error #")
            && !stdout.contains("node start #")
    );
}
