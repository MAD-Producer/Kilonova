use super::*;

fn candidate(root: &std::path::Path) -> motion_core::Project {
    let package = motion_effects::builtin::package().unwrap();
    let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
    let mut layer = motion_core::Layer::solid(
        1,
        "private-runtime-label",
        [32.; 2],
        [32., 32., 0.],
        [1.; 4],
    );
    let mut effect = motion_core::EffectInstance::new(
        1,
        "test.native-runtime-error",
        "1.0.0",
        &"0".repeat(64),
        &package.manifest.effects[0],
        [64.; 2],
    );
    let bytes = [0, 0];
    let sha256 = motion_core::storage::resource_sha256(&bytes);
    let resource = motion_core::NodeResource {
        path: format!("assets/node-{sha256}.msfx"),
        bytes: 2,
        sha256,
    };
    std::fs::create_dir(root).unwrap();
    let mut stage = motion_core::storage::NodeResourceStage::new(root).unwrap();
    stage.stage(&resource, &bytes).unwrap();
    stage.keep_resources();
    effect.node_graph = Some(
        serde_json::json!({"format_version":2,"key":"runtime-errors","document":{},"options":{},"runtime":resource}),
    );
    layer.effects.push(effect);
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    project
}

#[test]
fn actual_owned_zip_failure_reaches_worker_reply_default_logs_and_rollback_then_recovers() {
    const CHILD: &str = "MOTION_NODE_RUNTIME_NATIVE_ERROR_CHILD";
    const NAME: &str = "engine::failure::composition_tests::plan_error::node_runtime::actual_owned_zip_failure_reaches_worker_reply_default_logs_and_rollback_then_recovers";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let initial = motion_core::Project::new(64, 64, 30, 120).unwrap();
        let project_root = root.join("library/default");
        motion_core::storage::save(&project_root, &initial).unwrap();
        let saved = std::fs::read(project_root.join("project.json")).unwrap();
        let engine = Engine::start(
            project_root.clone(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine.state().unwrap();
        let source = root.join("source");
        let candidate = candidate(&source);
        motion_core::storage::save(&source, &candidate).unwrap();
        let archive = root.join("invalid-runtime.msproj");
        motion_core::storage::export_package(&source, &candidate, &archive).unwrap();
        let directory_entries = || {
            let mut entries: Vec<_> = std::fs::read_dir(root.join("library"))
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            entries.sort();
            entries
        };
        let initial_entries = directory_entries();
        let error = engine.import_project(archive).unwrap_err();
        assert!(
            error.starts_with("invalid compiled node runtime: invalid Zip archive:"),
            "{error}"
        );
        assert!(error.contains("Could not find EOCD"), "{error}");
        let after = engine.state().unwrap();
        for key in ["project", "revision", "history", "canUndo", "canRedo"] {
            assert_eq!(after[key], before[key], "failed request changed {key}");
        }
        assert_eq!(
            std::fs::read(project_root.join("project.json")).unwrap(),
            saved
        );
        assert!(!engine.failed());
        assert_eq!(directory_entries(), initial_entries);
        engine
            .new_project(serde_json::to_string(&initial).unwrap())
            .unwrap();
        let recovered = engine.state().unwrap();
        assert_eq!(recovered["project"], serde_json::to_value(initial).unwrap());
        assert!(!engine.failed());
        std::fs::write(root.join("reply.txt"), error).unwrap();
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
        std::fs::write(evidence.join("runtime-stdout.txt"), &output.stdout).unwrap();
        std::fs::write(evidence.join("runtime-stderr.txt"), &output.stderr).unwrap();
        if root.path().join("logs/motion-studio.log").is_file() {
            std::fs::copy(
                root.path().join("logs/motion-studio.log"),
                evidence.join("runtime-default.log"),
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
    let cause = std::fs::read_to_string(root.path().join("reply.txt")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("error #") && line.contains(" import_project "))
        .collect();
    assert_eq!(errors.len(), 1, "{file}");
    let request = errors[0]
        .split("error #")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    let start = format!("start #{request} import_project");
    assert!(file.find(&start).unwrap() < file.find(errors[0]).unwrap());
    for text in [&file, &stderr] {
        let matching: Vec<_> = text
            .lines()
            .filter(|line| line.contains(&format!("error #{request} import_project ")))
            .collect();
        assert_eq!(matching.len(), 1);
        assert!(matching[0].contains(&cause));
        assert!(matching[0].contains("ms:"));
        assert!(!text.contains("private-runtime-label"));
    }
    assert!(!stderr.contains(&start));
    assert!(!stderr.contains("logging initialized"));
    for forbidden in [
        &cause,
        &start,
        "error #",
        "logging initialized",
        "private-runtime-label",
    ] {
        assert!(!stdout.contains(forbidden));
    }
}
