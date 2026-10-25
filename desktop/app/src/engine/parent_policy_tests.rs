use super::Engine;
use std::path::PathBuf;

#[test]
fn parent_policy_failures_reach_default_sinks_and_a_valid_local_bind_recovers() {
    const NAME: &str = "engine::parent_policy_tests::parent_policy_failures_reach_default_sinks_and_a_valid_local_bind_recovers";
    const CHILD: &str = "MOTION_PARENT_POLICY_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let directory = root.join("project");
        let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
        for id in [7, 42] {
            project.layers.push(motion_core::Layer::solid(
                id,
                "Parent policy fixture",
                [32.; 2],
                [100., 100., 0.],
                [1.; 4],
            ));
        }
        project.layers[1].transform.scale.value = [0., 100., 100.];
        motion_core::storage::save(&directory, &project).unwrap();
        let engine = Engine::start(
            directory,
            motion_desktop_media::platform(),
            8 * 1024 * 1024 * 1024,
        )
        .unwrap();
        let before = engine.state().unwrap();
        for (parent, frame, reason) in [
            (42, 23, "zero-scale"),
            (999, 23, "does not exist"),
            (42, 999, "frame"),
        ] {
            let error = engine
                .edit(
                    serde_json::json!({"op":"parent_with_mode", "object":7,
                "parent":parent, "frame":frame, "mode":"preserve_world"})
                    .to_string(),
                )
                .unwrap_err();
            assert!(error.contains(reason), "{error}");
            assert!(error.contains("object=7"), "{error}");
            assert!(error.contains(&format!("parent=Some({parent})")), "{error}");
            assert!(error.contains(&format!("frame={frame}")), "{error}");
            assert_eq!(engine.state().unwrap()["project"], before["project"]);
            assert_eq!(engine.state().unwrap()["revision"], before["revision"]);
        }
        let applied = engine
            .edit(
                serde_json::json!({"op":"parent_with_mode", "object":7,
            "parent":42, "frame":23, "mode":"preserve_local"})
                .to_string(),
            )
            .unwrap();
        assert_eq!(applied["project"]["layers"][0]["parent"]["object"], 42);
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PARENT_POLICY_LOG_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for reason in ["zero-scale", "object 999 does not exist", "frame=999"] {
        assert!(
            file.lines()
                .any(|line| line.contains(" ERROR ") && line.contains(reason)),
            "{file}"
        );
        assert!(stderr.contains(reason), "{stderr}");
        assert!(!stdout.contains(reason), "{stdout}");
    }
    assert_eq!(
        file.lines()
            .filter(|line| line.contains(" ERROR ") && line.contains("parent edit object=7"))
            .count(),
        3
    );
    assert!(file.lines().any(|line| line.contains(" INFO ")
        && line.contains("start #")
        && line.ends_with(" edit")));
    assert!(!stderr.contains("start #"));
    assert!(!stdout.contains("start #"));
}
