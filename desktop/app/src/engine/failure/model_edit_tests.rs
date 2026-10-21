//! Actual model refusals retain their replies and reach the production sinks.
use super::*;

#[test]
fn invalid_model_edits_keep_history_and_reach_default_error_channels() {
    const CHILD: &str = "MOTION_MODEL_EDIT_LOG_CHILD";
    const TEST: &str = "engine::failure::model_edit_tests::invalid_model_edits_keep_history_and_reach_default_error_channels";
    const PRIVATE: &str = "private-rename-before-refusal";
    let cases = [
        (
            serde_json::json!({"op":"mask","object":1,"action":{"kind":"remove","mask":9}}),
            "mask does not exist: mask=9",
            None,
        ),
        (
            serde_json::json!({"op":"mask","object":1,"action":{"kind":"reorder","masks":[1]}}),
            "mask order must include each existing ID once",
            None,
        ),
        (
            serde_json::json!({"op":"composition","action":{"kind":"precompose","objects":[],"name":"Unused"}}),
            "Select distinct non-camera layers",
            Some("invalid_selection"),
        ),
        (
            serde_json::json!({"op":"composition","action":{"kind":"reference","target":"comp-missing","at_frame":0}}),
            "Composition does not exist",
            Some("composition_missing"),
        ),
    ];
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project_root = root.join("project");
        let mut project = motion_core::Project::new(32, 32, 30, 90).unwrap();
        project.layers.push(motion_core::Layer::solid(
            1,
            "Source",
            [32.; 2],
            [16., 16., 0.],
            [1.; 4],
        ));
        motion_core::storage::save(&project_root, &project).unwrap();
        let saved = std::fs::read(project_root.join("project.json")).unwrap();
        let engine = Engine::start(
            project_root.clone(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        engine
            .edit(r#"{"op":"rename","object":1,"name":"First edit"}"#.into())
            .unwrap();
        engine
            .edit(r#"{"op":"rename","object":1,"name":"Second edit"}"#.into())
            .unwrap();
        engine.history(0).unwrap();
        let before = engine.state().unwrap();
        assert_eq!(before["canRedo"], true);
        for (command, cause, code) in &cases {
            let batch = serde_json::json!([
                {"op":"rename","object":1,"name":PRIVATE},
                command,
            ]);
            let error = engine.edit(batch.to_string()).unwrap_err();
            assert!(error.contains(cause), "{error}");
            if let Some(code) = code {
                let envelope = motion_host::ops::error_envelope(&error);
                assert_eq!(envelope["error_detail"]["code"], *code);
            }
            let after = engine.state().unwrap();
            for field in ["project", "revision", "history", "canUndo", "canRedo"] {
                assert_eq!(after[field], before[field], "{cause}: changed {field}");
            }
            assert!(!engine.failed());
        }
        for _ in 0..60 {
            assert_eq!(engine.state().unwrap()["project"], before["project"]);
        }
        let redone = engine.history(1).unwrap();
        assert_eq!(redone["project"]["layers"][0]["name"], "Second edit");
        let recovered = engine
            .edit(r#"{"op":"rename","object":1,"name":"Editable after refusal"}"#.into())
            .unwrap();
        assert_eq!(
            recovered["project"]["layers"][0]["name"],
            "Editable after refusal"
        );
        assert_eq!(
            std::fs::read(project_root.join("project.json")).unwrap(),
            saved
        );
        drop(engine);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
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
    for (_, cause, _) in cases {
        let errors: Vec<_> = file.lines().filter(|line| line.contains(cause)).collect();
        assert_eq!(errors.len(), 1, "{cause}: {file}");
        let error = errors[0];
        assert!(error.contains("ERROR") && error.contains(" edit ") && error.contains("ms:"));
        let request = error
            .split("error #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        assert!(file.find(&format!("start #{request} edit")).unwrap() < file.find(error).unwrap());
        assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
        assert_eq!(stderr.matches(cause).count(), 1, "{stderr}");
        assert!(!stdout.contains(cause));
    }
    assert!(!stderr.contains("start #"));
    assert!(!stdout.contains("start #") && !stdout.contains("logging initialized"));
    for text in [&file, &stderr, &stdout] {
        assert!(!text.contains(PRIVATE), "user text was logged: {text}");
    }
}
