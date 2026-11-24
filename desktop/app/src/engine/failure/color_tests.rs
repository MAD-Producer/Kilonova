//! Actual serial worker/default logging, without a window or GPU surface.
use super::*;
use std::fs;
#[path = "../../../../../crates/motion-core/tests/support/color_project.rs"]
mod fixture;
pub(super) mod runtime_platform;

#[test]
fn color_preflight_and_missing_resources_reach_default_error_channels_without_changes() {
    const CHILD: &str = "MOTION_COLOR_CONTRACT_LOG_CHILD";
    const TEST: &str = "engine::failure::color_tests::color_preflight_and_missing_resources_reach_default_error_channels_without_changes";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let initial = motion_core::Project::new(64, 64, 30, 120).unwrap();
        let active = root.join("library/default");
        motion_core::storage::save(&active, &initial).unwrap();
        let original = fs::read(active.join("project.json")).unwrap();
        let missing_manifest = root.join("missing-runtime/ocio-runtime.json");
        let platform = Arc::new(runtime_platform::RuntimePlatform {
            manifest: missing_manifest.clone(),
            before_manifest: None,
            desktop: motion_desktop_media::platform(),
        });
        let engine = Engine::start(active.clone(), platform, 8 << 30).unwrap();
        let before = engine.state().unwrap();
        let mut invalid = fixture::settings();
        invalid.selection.config_key[0] ^= 1;
        for (index, settings) in [fixture::settings(), invalid].into_iter().enumerate() {
            let text = serde_json::to_string(&motion_core::Command::SetColorManagement {
                settings: fixture::managed(settings),
            })
            .unwrap();
            let error = engine.edit(text).unwrap_err();
            if index == 0 {
                assert!(matches!(
                    error.host(),
                    Some(motion_host::Error::ColorPreparation(
                        motion_host::color_preparation::Error::Resources(_)
                    ))
                ));
            } else {
                assert!(
                    error.to_string().contains("selected content identity"),
                    "{error}"
                );
            }
            let after = engine.state().unwrap();
            for key in ["project", "revision", "history", "canUndo", "canRedo"] {
                assert_eq!(after[key], before[key], "{key}");
            }
            assert!(!engine.failed());
        }
        let managed = fixture::project();
        let missing = root.join("library/project-101");
        fixture::write_resources(&missing, &managed);
        motion_core::storage::save(&missing, &managed).unwrap();
        fs::remove_file(missing.join(&managed.color_management.resources().unwrap()[0].path))
            .unwrap();
        let saved = fs::read(missing.join("project.json")).unwrap();
        let error = engine.open_project("project-101".into()).unwrap_err();
        assert!(error.to_string().contains("phase=resolve"), "{error}");
        fixture::write_resources(&missing, &managed);
        let error = engine.open_project("project-101".into()).unwrap_err();
        let Some(motion_host::Error::ColorPreparation(
            motion_host::color_preparation::Error::Provider { source, .. },
        )) = error.host()
        else {
            panic!("original native preparation cause: {error:?}")
        };
        let motion_host::Error::NativeColor(motion_effects::native_color::Error::Io {
            phase,
            path,
            source,
        }) = source.as_ref()
        else {
            panic!("original native IO: {source:?}")
        };
        assert_eq!(*phase, "manifest");
        assert_eq!(path, &missing_manifest.display().to_string());
        assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        let after = engine.state().unwrap();
        for key in ["project", "revision", "history", "canUndo", "canRedo"] {
            assert_eq!(after[key], before[key], "{key}");
        }
        assert_eq!(fs::read(missing.join("project.json")).unwrap(), saved);
        assert_eq!(fs::read(active.join("project.json")).unwrap(), original);
        // Explicit user edit recovers through the same worker, without restart.
        engine
            .edit(r#"{"op":"set_color","object":0,"value":[0.1,0.2,0.3,1]}"#.into())
            .unwrap();
        assert_eq!(
            engine.state().unwrap()["project"]["background"],
            serde_json::json!([0.1_f32, 0.2_f32, 0.3_f32, 1.])
        );
        drop(engine);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout_path = root.path().join("child.stdout");
    let stderr_path = root.path().join("child.stderr");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .stdout(fs::File::create(&stdout_path).unwrap())
        .stderr(fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(20) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("color child exceeded 20s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("ERROR") && line.contains("error #"))
        .collect();
    assert_eq!(errors.len(), 4, "{file}");
    for (operation, cause) in [
        ("edit", "phase=resources"),
        ("edit", "selected content identity"),
        ("open_project", "phase=resolve"),
        ("open_project", "phase=manifest"),
    ] {
        let error = errors
            .iter()
            .find(|line| line.contains(cause) && line.contains(&format!(" {operation} ")))
            .unwrap();
        assert!(stderr.contains(cause), "{stderr}");
        assert!(!stdout.contains(cause), "{stdout}");
        let request = error
            .split("error #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        assert!(
            file.find(&format!("start #{request} {operation}")).unwrap()
                < file.find(*error).unwrap()
        );
        assert!(error.contains("ms:"));
    }
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("motion-studio.log"), file).unwrap();
        fs::write(directory.join("child.stdout"), stdout).unwrap();
        fs::write(directory.join("child.stderr"), stderr).unwrap();
    }
}
