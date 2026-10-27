//! Failed project switches use the actual worker, filesystem and process sink.
use super::*;

#[test]
fn rejected_project_schema_preserves_files_state_and_default_error_channels() {
    const CHILD: &str = "MOTION_PROJECT_SCHEMA_ERROR_CHILD";
    const TEST: &str = "engine::failure::project_tests::rejected_project_schema_preserves_files_state_and_default_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut initial = motion_core::Project::new(64, 64, 30, 120).unwrap();
        initial.layers.push(motion_core::Layer::solid(
            1,
            "Layer",
            [32.; 2],
            [32., 32., 0.],
            [1.; 4],
        ));
        let active = root.join("library/default");
        motion_core::storage::save(&active, &initial).unwrap();
        let original = std::fs::read(active.join("project.json")).unwrap();
        let engine =
            Engine::start(active.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        let before = engine.state().unwrap();
        for (name, directory, cause) in [
            ("old-version", "project-101", "unsupported project format"),
            ("missing-version", "project-102", "missing field `version`"),
            ("missing-setting", "project-103", "missing field `three_d`"),
        ] {
            let mut value = serde_json::to_value(&initial).unwrap();
            match name {
                "old-version" => {
                    value["version"] = serde_json::json!(motion_core::CURRENT_PROJECT_VERSION - 1)
                }
                "missing-version" => {
                    value.as_object_mut().unwrap().remove("version");
                }
                "missing-setting" => {
                    value["layers"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("three_d");
                }
                _ => unreachable!(),
            }
            let folder = root.join("library").join(directory);
            std::fs::create_dir(&folder).unwrap();
            let bytes = serde_json::to_vec(&value).unwrap();
            std::fs::write(folder.join("project.json"), &bytes).unwrap();
            let error = engine.open_project(directory.into()).unwrap_err();
            assert!(error.contains(cause), "{error}");
            if name == "old-version" {
                assert!(error.contains(&format!(
                    "version={}",
                    motion_core::CURRENT_PROJECT_VERSION - 1
                )));
                assert!(error.contains(&format!(
                    "expected={}",
                    motion_core::CURRENT_PROJECT_VERSION
                )));
            }
            let after = engine.state().unwrap();
            for key in ["project", "revision", "history", "canUndo", "canRedo"] {
                assert_eq!(after[key], before[key], "{name}: changed {key}");
            }
            assert!(!engine.failed());
            assert_eq!(std::fs::read(folder.join("project.json")).unwrap(), bytes);
            assert_eq!(
                std::fs::read(active.join("project.json")).unwrap(),
                original
            );
            assert!(!folder.join("project.json.tmp").exists());
        }
        // Explicitly repair our fixture and make a new request; no automatic retry.
        std::fs::write(root.join("library/project-103/project.json"), &original).unwrap();
        engine.open_project("project-103".into()).unwrap();
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
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
    let file = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("ERROR") && line.contains(" open_project "))
        .collect();
    assert_eq!(errors.len(), 3, "{file}");
    for cause in [
        "unsupported project format",
        "missing field `version`",
        "missing field `three_d`",
    ] {
        let error = errors.iter().find(|line| line.contains(cause)).unwrap();
        assert!(stderr.contains(cause), "{stderr}");
        assert!(!stdout.contains(cause) && !stdout.contains("logging initialized"));
        let request = error
            .split("error #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        assert!(error.contains("ms:"));
        assert!(
            file.find(&format!("start #{request} open_project"))
                .unwrap()
                < file.find(*error).unwrap()
        );
    }
}

#[test]
fn recovered_plugin_settings_reach_default_file_and_stderr_without_polling_noise() {
    const CHILD: &str = "MOTION_PLUGIN_SETTINGS_LOG_CHILD";
    const SITE: &str = "site=host.registry.load";
    const TEST: &str = "engine::failure::project_tests::recovered_plugin_settings_reach_default_file_and_stderr_without_polling_noise";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let mode = std::env::var("MOTION_PLUGIN_SETTINGS_CASE").unwrap();
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        let project_root = root.join("library/default");
        let plugins = root.join("library/plugins");
        let settings = plugins.join("disabled.json");
        motion_core::storage::save(&project_root, &project).unwrap();
        std::fs::create_dir(&plugins).unwrap();
        let original = match mode.as_str() {
            "missing" => None,
            "valid" => Some(b"[]".to_vec()),
            "damaged" => Some(b"[[\"unfinished settings".to_vec()),
            "oversized" => Some(vec![b' '; 512 * 1024 + 1]),
            "directory" => {
                std::fs::create_dir(&settings).unwrap();
                std::fs::write(settings.join("keep"), b"preserved").unwrap();
                None
            }
            _ => panic!("unknown controlled fixture"),
        };
        if let Some(bytes) = &original {
            std::fs::write(&settings, bytes).unwrap();
        }
        let engine = Engine::start(
            project_root.clone(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let catalogue = engine.plugin(r#"{"op":"catalogue"}"#.into()).unwrap();
        let expected_errors = usize::from(!matches!(mode.as_str(), "missing" | "valid"));
        assert_eq!(
            catalogue["errors"].as_array().unwrap().len(),
            expected_errors
        );
        assert!(!catalogue["packages"].as_array().unwrap().is_empty());
        let before = engine.state().unwrap();
        for _ in 0..120 {
            assert_eq!(engine.state().unwrap()["project"], before["project"]);
        }
        assert!(!engine.failed());
        assert_eq!(motion_core::storage::load(&project_root).unwrap(), project);
        if let Some(bytes) = &original {
            assert_eq!(std::fs::read(&settings).unwrap(), *bytes);
        }
        if mode == "missing" {
            assert!(!settings.exists());
        }
        if mode == "directory" {
            assert_eq!(std::fs::read(settings.join("keep")).unwrap(), b"preserved");
        }
        drop(engine);
        // Explicitly repair only our fixture and reopen: no diagnostic is retained.
        if mode == "directory" {
            std::fs::remove_file(settings.join("keep")).unwrap();
            std::fs::remove_dir(&settings).unwrap();
        }
        std::fs::write(&settings, b"[]").unwrap();
        let repaired =
            Engine::start(project_root, motion_desktop_media::platform(), 8 << 30).unwrap();
        assert!(
            repaired.plugin(r#"{"op":"catalogue"}"#.into()).unwrap()["errors"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        drop(repaired);
        return;
    }
    for mode in ["missing", "valid", "damaged", "oversized", "directory"] {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env("MOTION_PLUGIN_SETTINGS_CASE", mode)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        let expected = usize::from(!matches!(mode, "missing" | "valid"));
        assert_eq!(file.matches(SITE).count(), expected, "{mode}\n{file}");
        assert_eq!(stderr.matches(SITE).count(), expected, "{mode}\n{stderr}");
        assert!(!stdout.contains(SITE) && !stdout.contains("logging initialized"));
        if expected != 0 {
            let record = file.lines().find(|line| line.contains(SITE)).unwrap();
            for field in [
                "ERROR",
                "operation=registry_load",
                "elapsed_ms=",
                "diagnostics=1",
                "disabled.json",
                "file was preserved",
            ] {
                assert!(record.contains(field), "{mode}: missing {field}: {record}");
            }
            assert!(record.len() < 4096);
            assert!(file.find("worker startup phase=session").unwrap() < file.find(SITE).unwrap());
            assert!(stderr.contains("disabled.json") && stderr.contains("file was preserved"));
        }
    }
}

#[test]
fn real_save_publication_failure_reaches_reply_file_and_stderr() {
    const CHILD: &str = "MOTION_PROJECT_PUBLICATION_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        let project_root = root.join("project");
        motion_core::storage::save(&project_root, &project).unwrap();
        let engine = Engine::start(
            project_root.clone(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine.state().unwrap();
        let target = project_root.join("project.json");
        let original = std::fs::read(&target).unwrap();
        std::fs::remove_file(&target).unwrap();
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), b"preserved").unwrap();
        let error = engine.save().unwrap_err();
        for field in ["phase=publish", "published=false", "cause=", "project.json"] {
            assert!(error.contains(field), "{error}");
        }
        std::fs::write(root.join("reply.txt"), &error).unwrap();
        assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"preserved");
        assert!(!project_root.join("project.json.tmp").exists());
        assert!(!engine.failed());
        let after = engine.state().unwrap();
        for key in ["project", "revision", "history", "canUndo", "canRedo"] {
            assert_eq!(before[key], after[key], "changed {key}");
        }
        // Repair only the controlled fixture, then make an explicit new request.
        std::fs::remove_file(target.join("keep")).unwrap();
        std::fs::remove_dir(&target).unwrap();
        std::fs::write(&target, original).unwrap();
        engine.save().unwrap();
        assert_eq!(motion_core::storage::load(&project_root).unwrap(), project);
        drop(engine);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::failure::project_tests::real_save_publication_failure_reaches_reply_file_and_stderr"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let reply = std::fs::read_to_string(root.path().join("reply.txt")).unwrap();
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("phase=publish"))
        .collect();
    assert_eq!(errors.len(), 1, "{file}");
    let error = errors[0];
    assert!(error.contains("ERROR") && error.contains(" save ") && error.contains("ms:"));
    assert!(error.contains(&reply), "{error}\nreply={reply}");
    assert!(stderr.contains(&reply));
    let request = error
        .split("error #")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    assert!(file.find(&format!("start #{request} save")).unwrap() < file.find(error).unwrap());
    assert!(!stdout.contains(&reply) && !stdout.contains("logging initialized"));
}

#[test]
fn rejected_project_switch_rolls_back_directory_and_preserves_error_channels() {
    const CHILD: &str = "MOTION_PROJECT_ROLLBACK_LOG_CHILD";
    const CAUSE: &str = "invalid compiled node runtime";
    const MISSING: &str = "node resource phase=resolve";
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

        // The document is valid, but its project-owned runtime is genuinely
        // corrupt. Model/save/import succeed; replace_project rejects it before
        // switching the active session. No mocked error or decoder is used.
        let package = motion_effects::builtin::package().unwrap();
        let mut candidate = initial.clone();
        candidate.version = motion_model::CURRENT_PROJECT_VERSION;
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
        let bytes = [0, 0];
        let sha256 = motion_core::storage::resource_sha256(&bytes);
        let resource = motion_core::NodeResource {
            path: format!("assets/node-{sha256}.msfx"),
            bytes: 2,
            sha256,
        };
        effect.node_graph = Some(serde_json::json!({
            "format_version":2,"key":"cleanup-test","document":{},"options":{},
            "runtime":resource
        }));
        layer.effects.push(effect);
        candidate.layers.push(layer);
        candidate.rebuild_plugin_dependencies();
        candidate.validate().unwrap();
        let source = root.join("source");
        std::fs::create_dir(&source).unwrap();
        let mut stage = motion_core::storage::NodeResourceStage::new(&source).unwrap();
        stage.stage(&resource, &bytes).unwrap();
        stage.keep_resources();
        motion_core::storage::save(&source, &candidate).unwrap();
        let archive = root.join("controlled-project.msproj");
        motion_core::storage::export_package(&source, &candidate, &archive).unwrap();
        // A newly allocated project has no assets directory. Windows reports
        // PATH_NOT_FOUND (3), while a missing final file reports FILE_NOT_FOUND
        // (2); preserve and compare the actual platform cause, not a fixed code.
        let missing_cause = std::fs::canonicalize(root.join("absent-parent/node.msfx"))
            .unwrap_err()
            .to_string();

        for import in [false, true, false, true] {
            let result = if import {
                engine.import_project(archive.clone())
            } else {
                engine.new_project(serde_json::to_string(&candidate).unwrap())
            };
            let error = result.unwrap_err();
            if import {
                assert!(
                    error.contains(CAUSE) && error.contains("Could not find EOCD"),
                    "{error}"
                );
            } else {
                // JSON alone cannot supply an external file to a fresh root.
                assert!(
                    error.contains(MISSING)
                        && error.contains(&missing_cause)
                        && error.contains(&resource.path),
                    "{error}"
                );
            }
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
    if let Some(evidence) = std::env::var_os("MOTION_NODE_OWNED_EVIDENCE_DIR") {
        let evidence = PathBuf::from(evidence);
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(evidence.join("project-rollback-stdout.txt"), &output.stdout).unwrap();
        std::fs::write(evidence.join("project-rollback-stderr.txt"), &output.stderr).unwrap();
        if root.path().join("motion-studio.log").is_file() {
            std::fs::copy(
                root.path().join("motion-studio.log"),
                evidence.join("project-rollback-default.log"),
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
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains(CAUSE) || line.contains(MISSING))
        .collect();
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
    assert_eq!(stderr.matches(CAUSE).count(), 3);
    assert_eq!(stderr.matches(MISSING).count(), 2);
    for text in [file.as_str(), stderr.as_str(), stdout.as_str()] {
        assert!(!text.contains("private-project-title"));
    }
    assert!(
        !stdout.contains(CAUSE)
            && !stdout.contains(MISSING)
            && !stdout.contains("logging initialized")
    );
}
