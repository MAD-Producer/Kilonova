use super::*;
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_render::Scene;
use std::{error::Error, sync::Arc};

#[path = "../../../../../../../../crates/motion-effects/tests/support/scene_packages.rs"]
mod scene_fixtures;

fn retains_original(error: &(dyn Error + 'static), original: &motion_core::Error) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if error
            .downcast_ref::<motion_core::Error>()
            .is_some_and(|cause| std::ptr::eq(cause, original))
        {
            return true;
        }
        current = error.source();
    }
    false
}

#[test]
fn deferred_particle_history_refusal_reaches_default_reply_file_and_stderr_then_recovers() {
    const CHILD: &str = "MOTION_PARTICLE_HISTORY_CAUSE_CHILD";
    const NAME: &str = "engine::failure::composition_tests::plan_error::generator_settings::history::deferred_particle_history_refusal_reaches_default_reply_file_and_stderr_then_recovers";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let package = scene_fixtures::particle_package().unwrap();
        let mut project = Project::new(128, 128, 30, 120).unwrap();
        let mut layer = Layer::solid(7, "History cause", [128.; 2], [64., 64., 0.], [0.; 4]);
        layer.effects.push(EffectInstance::new(
            11,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            &package.manifest.effects[0],
            [128.; 2],
        ));
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        project.validate().unwrap();
        let mut scene = Scene::new(&project).unwrap();
        scene
            .sample(&project, 17.5, None, &ExpressionEvaluator)
            .unwrap();
        validate_settings(&scene.effects[0], RendererKind::ParticleEmitter).unwrap();
        let mut malformed = project.clone();
        malformed.layers[0].effects[0]
            .scene
            .as_mut()
            .unwrap()
            .source_layer = Some(999);
        // A controlled malformed draft reaches the real sampler. It does not
        // claim that project validation accepts an absent source layer.
        scene
            .sample(&malformed, 17.5, None, &ExpressionEvaluator)
            .unwrap();
        let cause = scene.effects[0]
            .particle_history_error
            .as_ref()
            .unwrap()
            .clone();
        assert!(matches!(cause.as_ref(), motion_core::Error::Missing(999)));
        let retained = scene.effects[0].clone();
        assert!(Arc::ptr_eq(
            &cause,
            retained.particle_history_error.as_ref().unwrap()
        ));
        let failure = validate_settings(&retained, RendererKind::ParticleEmitter).unwrap_err();
        let host = motion_host::Error::from(failure);
        assert!(retains_original(&host, &cause));
        let original = host.to_string();
        assert!(!std::fs::read_to_string(root.join("motion-studio.log"))
            .unwrap()
            .contains(&original));
        let (reply, receive) = channel::<Result<Value, String>>();
        // Exercise the actual responder/logger, separately from worker
        // scheduling and in-flight publication, which are not under test here.
        CURRENT_COMMAND.set(Some(Flight {
            request: 1851,
            operation: "test_particle_history",
            started: Instant::now(),
        }));
        respond(reply, Err(host));
        CURRENT_COMMAND.set(None);
        assert_eq!(
            receive
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap_err(),
            original
        );
        scene
            .sample(&project, 17.5, None, &ExpressionEvaluator)
            .unwrap();
        assert!(scene.effects[0].particle_history_error.is_none());
        validate_settings(&scene.effects[0], RendererKind::ParticleEmitter).unwrap();
        std::fs::write(root.join("cause.txt"), original).unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
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
    let cause = std::fs::read_to_string(root.path().join("cause.txt")).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let marker = "error #1851 test_particle_history ";
    for text in [&file, &stderr] {
        let lines: Vec<_> = text.lines().filter(|line| line.contains(marker)).collect();
        assert_eq!(lines.len(), 1, "{text}");
        for context in [
            &cause,
            "layer 7",
            "effect 11",
            "frame 17.5",
            "object 999",
            "ms:",
        ] {
            assert!(lines[0].contains(context), "missing {context}: {text}");
        }
    }
    for text in [&stderr, &stdout] {
        assert!(!text.contains("logging initialized"));
    }
    assert!(!stdout.contains(marker));
    assert!(!stdout.contains(&cause));
}
