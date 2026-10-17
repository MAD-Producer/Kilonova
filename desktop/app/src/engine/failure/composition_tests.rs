//! Real command failures through the desktop worker and process logger.
use super::*;
use motion_core::composition::{Composition, MAX_COMPOSITIONS};

#[test]
fn capacity_failure_reaches_reply_file_and_stderr_at_default_level() {
    const CHILD: &str = "MOTION_COMPOSITION_BOUNDARY_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            1,
            "Selected",
            [32.; 2],
            [32., 32., 0.],
            [1.; 4],
        ));
        project.compositions = (1..MAX_COMPOSITIONS)
            .map(|id| Composition {
                id: format!("comp-{id}"),
                name: format!("Library {id}"),
                width: 64,
                height: 64,
                fps: 30,
                frames: 120,
                background: [0.; 4],
                camera: motion_core::Camera::new(64, 64),
                layers: vec![],
                expressions: vec![],
            })
            .collect();
        let project_root = root.join("project");
        motion_core::storage::save(&project_root, &project).unwrap();
        let saved = std::fs::read(project_root.join("project.json")).unwrap();
        let engine = Engine::start(
            project_root.clone(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine.state().unwrap();
        let commands = serde_json::json!([
            {"op":"rename", "object":1, "name":"private-command-text-must-not-be-logged"},
            {"op":"composition", "composition":"comp-main",
             "action":{"kind":"precompose", "objects":[1], "name":"Nested"}}
        ])
        .to_string();
        for _ in 0..2 {
            let error = engine.edit(commands.clone()).unwrap_err();
            let reply = motion_host::ops::error_envelope(&error);
            assert_eq!(reply["error_detail"]["code"], "resource_limit");
            assert_eq!(reply["error_detail"]["composition"], "comp-main");
            assert!(error.contains("Composition limit exceeded"));
            let after = engine.state().unwrap();
            for field in ["project", "revision", "history", "canUndo", "canRedo"] {
                assert_eq!(after[field], before[field], "changed {field}");
            }
            assert!(!engine.failed());
        }
        assert_eq!(
            std::fs::read(project_root.join("project.json")).unwrap(),
            saved
        );
        let after = engine
            .edit(r#"{"op":"rename","object":1,"name":"Still editable"}"#.into())
            .unwrap();
        assert_eq!(after["revision"], before["revision"].as_u64().unwrap() + 1);
        let deadline = Instant::now() + Duration::from_secs(5);
        while engine.in_flight.lock().unwrap().is_some() {
            assert!(
                Instant::now() < deadline,
                "completed command remained in flight"
            );
            std::thread::yield_now();
        }
        drop(engine);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::failure::composition_tests::capacity_failure_reaches_reply_file_and_stderr_at_default_level"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("Composition limit exceeded"))
        .collect();
    assert_eq!(errors.len(), 2, "{file}");
    for error in errors {
        assert!(error.contains("ERROR") && error.contains("error #") && error.contains(" edit "));
        assert!(
            error.contains("ms:")
                && error.contains("resource_limit")
                && error.contains("comp-main")
        );
        let request = error
            .split("error #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let start = format!("start #{request} edit");
        assert!(file.find(&start).unwrap() < file.find(error).unwrap());
        assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    }
    assert_eq!(stderr.matches("Composition limit exceeded").count(), 2);
    assert!(!stderr.contains("start #"));
    assert!(!file.contains("private-command-text-must-not-be-logged"));
    assert!(!stderr.contains("private-command-text-must-not-be-logged"));
    for marker in [
        "Composition limit exceeded",
        "resource_limit",
        "start #",
        "logging initialized",
    ] {
        assert!(
            !stdout.contains(marker),
            "log records leaked to stdout: {stdout}"
        );
    }
}
