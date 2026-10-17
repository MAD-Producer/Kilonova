//! Failed project switches use the actual worker, filesystem and process sink.
use super::*;

#[test]
fn rejected_project_switch_rolls_back_directory_and_preserves_error_channels() {
    const CHILD: &str = "MOTION_PROJECT_ROLLBACK_LOG_CHILD";
    const CAUSE: &str = "invalid compiled node runtime archive encoding";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
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

        // The document is valid, but its embedded runtime archive is genuinely
        // corrupt. Model/save/import succeed; replace_project rejects it before
        // switching the active session. No mocked error or decoder is used.
        let package = motion_effects::builtin::package().unwrap();
        let mut candidate = initial.clone();
        candidate.version = 17;
        let mut layer = motion_core::Layer::solid(
            1,
            "private-project-title",
            [32.; 2],
            [32., 32., 0.],
            [1.; 4],
        );
        let mut effect = motion_core::EffectInstance::new(
            1,
            "test.project-cleanup",
            "1.0.0",
            &"0".repeat(64),
            &package.manifest.effects[0],
            [64.; 2],
        );
        effect.node_graph = Some(serde_json::json!({
            "format_version":1,"key":"cleanup-test","document":{},"options":{},
            "runtime":{"package_hex":"zz"}
        }));
        layer.effects.push(effect);
        candidate.layers.push(layer);
        candidate.rebuild_plugin_dependencies();
        candidate.validate().unwrap();
        let source = root.join("source");
        motion_core::storage::save(&source, &candidate).unwrap();
        let archive = root.join("controlled-project.msproj");
        motion_core::storage::export_package(&source, &candidate, &archive).unwrap();

        for import in [false, true, false, true] {
            let result = if import {
                engine.import_project(archive.clone())
            } else {
                engine.new_project(serde_json::to_string(&candidate).unwrap())
            };
            assert!(result.unwrap_err().contains(CAUSE));
            let after = engine.state().unwrap();
            for key in ["project", "revision", "history", "canUndo", "canRedo"] {
                assert_eq!(after[key], before[key], "changed {key}");
            }
            assert!(!engine.failed());
            assert_eq!(
                std::fs::read(project_root.join("project.json")).unwrap(),
                saved
            );
            let leftovers: Vec<_> = std::fs::read_dir(root.join("library"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|name| {
                    name.starts_with("project-")
                        || name.starts_with("import-")
                        || name.starts_with(".motion-import-")
                })
                .collect();
            assert!(leftovers.is_empty(), "orphaned projects: {leftovers:?}");
        }
        // Success disarms ownership: the new active directory survives return.
        engine
            .new_project(serde_json::to_string(&initial).unwrap())
            .unwrap();
        let library = root.join("library");
        let active: Vec<_> = std::fs::read_dir(&library)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("project-")
            })
            .collect();
        assert_eq!(active.len(), 1);
        assert_eq!(motion_core::storage::load(&active[0]).unwrap(), initial);
        engine.import_project(archive).unwrap_err();
        assert!(active[0].join("project.json").is_file());
        drop(engine);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::failure::project_tests::rejected_project_switch_rolls_back_directory_and_preserves_error_channels"])
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
    let errors: Vec<_> = file.lines().filter(|line| line.contains(CAUSE)).collect();
    assert_eq!(errors.len(), 5, "{file}");
    for error in errors {
        assert!(
            error.contains("ERROR") && error.contains("error #") && error.contains("ms:"),
            "{error}"
        );
        let request = error
            .split("error #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let operation = if error.contains(" new_project ") {
            "new_project"
        } else {
            "import_project"
        };
        assert!(
            file.find(&format!("start #{request} {operation}")).unwrap()
                < file.find(error).unwrap()
        );
        assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    }
    assert_eq!(stderr.matches(CAUSE).count(), 5);
    for text in [file.as_str(), stderr.as_str(), stdout.as_str()] {
        assert!(!text.contains("private-project-title"));
    }
    assert!(!stdout.contains(CAUSE) && !stdout.contains("logging initialized"));
}
