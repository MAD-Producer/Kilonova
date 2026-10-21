//! Actual CPU bundle failure through the host API and default desktop log sink.
use super::*;
use motion_core::{EffectInstance, Layer, Project, MAIN_COMPOSITION};
use motion_host::{ops::composition::frame_bundle, session};
use std::{process::Command, sync::Arc};

#[test]
fn composition_bundle_failure_reaches_file_and_stderr_and_recovers() {
    const CHILD: &str = "MOTION_COMPOSITION_BUNDLE_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        fs::create_dir_all(root.join("project")).unwrap();
        let mut project = Project::new(64, 64, 30, 120).unwrap();
        let package = motion_effects::builtin::package().unwrap();
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|e| e.id == "gaussian_blur")
            .unwrap();
        let mut layer = Layer::solid(1, "Blur", [3840., 2160.], [32., 32., 0.], [1.; 4]);
        layer.effects.push(EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            layer.size,
        ));
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        project.validate().unwrap();
        let id = session::open(
            root.join("project"),
            project,
            Arc::new(motion_desktop_media::DesktopPlatform::new()),
        )
        .unwrap();
        let mut bytes = vec![];
        let error = frame_bundle(id, MAIN_COMPOSITION, 4.5, &mut bytes).unwrap_err();
        assert!(error.starts_with("composition comp-main:"), "{error}");
        assert!(error.contains("scratch"), "{error}");
        session::with_session(id, |s| s.effects.set_scratch_budget(384 * 1024 * 1024)).unwrap();
        assert!(frame_bundle(id, MAIN_COMPOSITION, 4.5, &mut bytes).unwrap() > 0);
        assert_eq!(
            &bytes[..4],
            &motion_render::composition_plan::MAGIC.to_le_bytes()
        );
        assert_eq!(
            frame_bundle(id, &"x".repeat(8192), 5., &mut bytes).unwrap_err(),
            "composition context mismatch"
        );
        session::close(id);
        flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "logging::composition_bundle_tests::composition_bundle_failure_reaches_file_and_stderr_and_recovers", "--nocapture"])
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
    for (text, errors, starts) in [(&file, 2, 3), (&stderr, 2, 0), (&stdout, 0, 0)] {
        assert_eq!(
            text.lines()
                .filter(
                    |line| line.contains("composition bundle failed site=host.composition.bundle")
                )
                .count(),
            errors,
            "{text}"
        );
        assert_eq!(
            text.lines()
                .filter(
                    |line| line.contains("composition bundle start site=host.composition.bundle")
                )
                .count(),
            starts,
            "{text}"
        );
    }
    assert!(!file.contains("composition bundle done")); // Debug, absent at default Info.
    assert!(file.contains("frame=4.5 phase=sample_or_pack elapsed_us="));
    assert!(stderr.contains("cause=composition comp-main:"));
    assert!(stderr.contains("scratch"));
    assert!(!file.contains(&"x".repeat(129)));
    assert!(!stderr.contains(&"x".repeat(129)));
}
