use super::*;

#[test]
fn refused_expression_edits_reach_actual_worker_default_channels_and_recover() {
    const CHILD: &str = "MOTION_EXPRESSION_CAUSE_LOG_CHILD";
    const TEST: &str = "engine::failure::model_edit_tests::expression_causes::refused_expression_edits_reach_actual_worker_default_channels_and_recover";
    fn command(source: &str) -> String {
        serde_json::to_string(&motion_core::Command::SetExpression {
            expression: motion_core::PropertyExpression {
                target: motion_core::ExpressionTarget::Property {
                    object: 7, property: motion_core::Property::Position, axis: Some(motion_core::Axis::X),
                },
                source: source.into(), enabled: true, seed: 42,
                profile: motion_core::EXPRESSION_PROFILE.into(),
            },
            frame: 0,
        }).unwrap()
    }
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project_root = root.join("project");
        let mut project = motion_core::Project::new(32, 32, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(7, "Expression owner", [20.; 2], [10., 20., 0.], [1.; 4]));
        motion_core::storage::save(&project_root, &project).unwrap();
        let saved = std::fs::read(project_root.join("project.json")).unwrap();
        let engine = Engine::start(project_root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        let before = engine.state().unwrap();
        let mut causes = Vec::new();
        for (source, reason) in [("value +", "JavaScript"), ("[1,2]", "scalar")] {
            let failure = engine.edit(command(source)).unwrap_err();
            assert!(failure.contains(reason) && failure.contains("at frame 0:"), "{failure}");
            assert!(failure.contains("\"object\":7"), "{failure}");
            let after = engine.state().unwrap();
            for field in ["project", "revision", "history", "canUndo", "canRedo"] {
                assert_eq!(after[field], before[field], "{reason}: changed {field}");
            }
            assert!(!engine.failed());
            causes.push(failure);
        }
        engine.edit(command("value + time")).unwrap();
        engine.seek(5.5).unwrap();
        assert!(!engine.failed());
        assert_eq!(std::fs::read(project_root.join("project.json")).unwrap(), saved);
        std::fs::write(root.join("primary-causes.json"), serde_json::to_vec(&causes).unwrap()).unwrap();
        drop(engine);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let causes: Vec<String> = serde_json::from_slice(&std::fs::read(root.path().join("primary-causes.json")).unwrap()).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(causes.len(), 2);
    assert!(file.contains("logging initialized level=INFO"));
    for cause in causes {
        let errors: Vec<_> = file.lines().filter(|line| line.contains("ERROR") && line.contains(&cause)).collect();
        assert_eq!(errors.len(), 1, "{cause}: {file}");
        let error = errors[0];
        assert!(error.contains(" edit ") && error.contains("ms:") && error.contains("at frame 0:"));
        let request = error.split("error #").nth(1).unwrap().split_whitespace().next().unwrap();
        assert!(file.find(&format!("start #{request} edit")).unwrap() < file.find(error).unwrap());
        assert_eq!(stderr.matches(&cause).count(), 1, "{stderr}");
        assert!(!stdout.contains(&cause));
    }
    assert_eq!(file.lines().filter(|line| line.contains(" ERROR ")).count(), 2, "{file}");
    assert_eq!(stderr.lines().filter(|line| line.contains("ERROR")).count(), 2, "{stderr}");
    assert!(!stderr.contains("start #"));
    assert!(!stdout.contains("start #") && !stdout.contains("logging initialized"));
}
