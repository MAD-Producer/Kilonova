//! Actual native errors at default Info: bound log payloads while preserving
//! full replies, original project bytes and recovery through the worker.
use super::*;

#[test]
fn long_command_and_startup_errors_are_bounded_without_truncating_replies() {
    const CHILD: &str = "MOTION_BOUNDED_COMMAND_ERROR_CHILD";
    const COMMAND_TAIL: &str = "private-command-tail";
    const PROJECT_TAIL: &str = "private-project-tail";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        let engine = crate::engine::Engine::start(
            root.join("project"),
            motion_desktop_media::platform(),
            crate::total_memory(),
        )
        .unwrap();
        let before = engine.state().unwrap();
        let operation = format!("invalid-{}-{COMMAND_TAIL}", "隐".repeat(3_000));
        let reply = engine
            .edit(serde_json::json!({"op":operation}).to_string())
            .unwrap_err();
        assert!(reply.contains(&operation));
        assert!(reply.chars().count() > 2_048);
        let after = engine.state().unwrap();
        assert_eq!(after["project"], before["project"]);
        assert_eq!(after["revision"], before["revision"]);
        let object = before["project"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|layer| layer["id"].as_u64())
            .max()
            .unwrap_or(0)
            + 1;
        let recovered = engine
            .edit(
                serde_json::to_string(&motion_core::Command::Add {
                    layer: motion_core::Layer::solid(
                        object,
                        "Recovered",
                        [16.; 2],
                        [0.; 3],
                        [1.; 4],
                    ),
                })
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            recovered["revision"].as_u64().unwrap(),
            before["revision"].as_u64().unwrap() + 1
        );
        assert_eq!(
            recovered["project"]["layers"].as_array().unwrap().len(),
            before["project"]["layers"].as_array().unwrap().len() + 1
        );
        drop(engine);

        let broken = root.join("broken-project");
        fs::create_dir(&broken).unwrap();
        let mut project =
            serde_json::to_value(motion_core::Project::new(64, 64, 30, 60).unwrap()).unwrap();
        let field = format!("invalid-{}-{PROJECT_TAIL}", "隐".repeat(3_000));
        project
            .as_object_mut()
            .unwrap()
            .insert(field.clone(), serde_json::json!(true));
        let original = serde_json::to_vec(&project).unwrap();
        fs::write(broken.join("project.json"), &original).unwrap();
        let reply = crate::engine::Engine::start(
            broken.clone(),
            motion_desktop_media::platform(),
            crate::total_memory(),
        )
        .err()
        .expect("invalid project unexpectedly opened");
        assert!(reply.contains(&field));
        assert!(reply.chars().count() > 2_048);
        assert_eq!(fs::read(broken.join("project.json")).unwrap(), original);
        flush();
        return;
    }

    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "logging::command_error_tests::long_command_and_startup_errors_are_bounded_without_truncating_replies", "--nocapture"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for text in [&file, &stderr] {
        let commands: Vec<_> = text
            .lines()
            .filter(|line| line.contains("error #") && line.contains(" edit "))
            .collect();
        assert_eq!(commands.len(), 1);
        let cause = commands[0].split_once("ms: ").unwrap().1;
        assert!(cause.contains("unknown variant"));
        assert_eq!(cause.chars().count(), 2_048);
        let startups: Vec<_> = text
            .lines()
            .filter(|line| line.contains("worker startup session failed: "))
            .collect();
        assert_eq!(startups.len(), 1);
        let cause = startups[0]
            .split_once("worker startup session failed: ")
            .unwrap()
            .1;
        assert!(cause.contains("unknown field"));
        assert_eq!(cause.chars().count(), 2_048);
        assert!(!text.contains(COMMAND_TAIL));
        assert!(!text.contains(PROJECT_TAIL));
    }
    assert!(file.contains("start #") && file.contains(" edit"));
    assert!(!stdout.contains("error #"));
    assert!(!stdout.contains("worker startup"));
    assert!(!stdout.contains(COMMAND_TAIL));
    assert!(!stdout.contains(PROJECT_TAIL));
}
