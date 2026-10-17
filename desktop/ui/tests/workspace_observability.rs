use motion_ui::{
    dock::{DockId, Node},
    workspace::Workspace,
};
use std::{path::Path, sync::Mutex};

struct Capture(Mutex<Vec<(log::Level, String)>>);
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if text.starts_with("UI workspace load degraded ") {
            self.0.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}
fn snapshot(workspace: &Workspace) -> serde_json::Value {
    serde_json::to_value(workspace).unwrap()
}
fn assert_default(path: &Path) {
    assert_eq!(
        snapshot(&Workspace::load(path)),
        snapshot(&Workspace::default())
    );
}

#[test]
fn framework_workspace_failures_are_bounded_visible_and_keep_default_layout_fallback() {
    const TEST: &str =
        "framework_workspace_failures_are_bounded_visible_and_keep_default_layout_fallback";
    const CHILD: &str = "MOTION_FRAMEWORK_WORKSPACE_TEST";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["read", "size", "json", "validation"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, mode)
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains("site=workspace.framework.load")
            );
        }
        return;
    };
    let capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let root = std::env::temp_dir().join(format!(
        "motion-original-workspace-observation-{}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let missing = root.join("missing.json");
    for _ in 0..120 {
        assert_default(&missing);
    }
    let mut valid = Workspace::default();
    valid.root = Node::dock(DockId(4242));
    let valid_path = root.join("valid.json");
    valid.save(&valid_path).unwrap();
    for _ in 0..120 {
        assert_eq!(snapshot(&Workspace::load(&valid_path)), snapshot(&valid));
    }
    assert!(capture.0.lock().unwrap().is_empty());
    let path = root.join("bad.json");
    let bytes = match mode.as_str() {
        "read" => {
            std::fs::create_dir(&path).unwrap();
            None
        }
        "size" => Some(vec![b' '; 16 * 1024 + 1]),
        "json" => Some(b"invalid original fixture".to_vec()),
        "validation" => {
            let mut invalid = Workspace::default();
            invalid.root = Node::dock(DockId(1));
            invalid.floating.push(DockId(1));
            Some(serde_json::to_vec(&invalid).unwrap())
        }
        _ => unreachable!(),
    };
    if let Some(bytes) = &bytes {
        std::fs::write(&path, bytes).unwrap();
    }
    for _ in 0..120 {
        assert_default(&path);
    }
    {
        let records = capture.0.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, log::Level::Warn);
        assert_eq!(records[1].0, log::Level::Error);
        assert!(records[0].1.contains("count=1 "));
        assert!(records[1].1.contains("count=60 "));
        assert!(records
            .iter()
            .all(|(_, s)| s.contains(&format!("phase={mode} "))
                && s.contains("reason=")
                && s.contains("fallback=default_layout")));
    }
    if let Some(bytes) = &bytes {
        assert_eq!(&std::fs::read(&path).unwrap(), bytes);
    } else {
        assert!(path.is_dir());
    }
    // A normal first-run miss resets the process-wide load episode.
    assert_default(&missing);
    for _ in 0..59 {
        assert_default(&path);
    }
    assert_eq!(capture.0.lock().unwrap().len(), 3);
    assert_default(&path);
    assert_eq!(capture.0.lock().unwrap().len(), 4);
    assert_eq!(snapshot(&Workspace::load(&valid_path)), snapshot(&valid));
    let long = root.join("sensitive-original-敏".repeat(2000));
    assert_default(&long);
    let records = capture.0.lock().unwrap();
    assert_eq!(records.len(), 5);
    assert!(records[4].1.chars().count() < 850);
    assert!(!records[4].1.contains(&long.to_string_lossy().to_string()));
    drop(records);
    // Only this test's uniquely created directory is removed.
    let resolved = root.canonicalize().unwrap();
    let temporary_root = std::env::temp_dir().canonicalize().unwrap();
    assert!(resolved.is_absolute());
    assert_eq!(resolved.parent(), Some(temporary_root.as_path()));
    std::fs::remove_dir_all(resolved).unwrap();
}
