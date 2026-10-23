use super::*;

mod generator_settings;
mod vector_geometry;
mod mask_geometry;

#[test]
fn cached_plan_failure_reaches_original_reply_file_and_stderr_at_default_level() {
    const CHILD: &str = "MOTION_NATIVE_PLAN_ERROR_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::plan_error::cached_plan_failure_reaches_original_reply_file_and_stderr_at_default_level";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let package = motion_effects::builtin::package().unwrap();
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|effect| effect.id == "tint")
            .unwrap();
        let mut project = motion_core::Project::new(8, 8, 30, 1).unwrap();
        let mut layer = motion_core::Layer::solid(1, "Plan error", [8.; 2], [4., 4., 0.], [1.; 4]);
        layer.effects.push(motion_core::EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [8.; 2],
        ));
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        let mut scene = motion_render::Scene::new(&project).unwrap();
        scene
            .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let hash = scene.effects[0].hash.clone();
        scene.effects[0].hash = "f".repeat(64);
        let mut builder = motion_render::effect_plan::PlanBuilder::new(
            motion_effects::Registry::new_with_builtins().unwrap(),
        )
        .unwrap();
        for request in [206, 207] {
            let failure = builder.build(&scene, &[0], 8, 8, true).err().unwrap();
            let original = failure.to_string();
            let motion_render::RenderError::Diagnostic { cause, .. } = &failure else {
                panic!("{failure:?}")
            };
            let motion_render::RenderError::Shared(cause) = cause.as_ref() else {
                panic!("{cause:?}")
            };
            assert!(matches!(
                cause.as_ref(),
                motion_render::RenderError::Effects(_)
            ));
            let (reply, receive) = channel::<Result<Value, String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_effect_plan",
                started: Instant::now(),
            }));
            respond(reply, Err(failure.into()));
            CURRENT_COMMAND.set(None);
            assert_eq!(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err(),
                original
            );
        }
        scene.effects[0].hash = hash;
        assert!(builder
            .build(&scene, &[0], 8, 8, true)
            .unwrap()
            .diagnostics
            .is_empty());
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME])
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
    for request in [206, 207] {
        let marker = format!("error #{request} test_effect_plan ");
        let records = file
            .lines()
            .filter(|line| line.contains(&marker))
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        let record = records[0];
        for context in ["layer 1, effect 1", "missing plugin", "ms:"] {
            assert!(record.contains(context), "missing {context}: {record}");
        }
        assert!(stderr.contains(record.split("motion_studio::engine: ").nth(1).unwrap()));
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert_eq!(stderr.matches("missing plugin").count(), 2);
    assert!(!stderr.contains("logging initialized"));
    for marker in [
        "missing plugin",
        "error #206",
        "error #207",
        "logging initialized",
    ] {
        assert!(
            !stdout.contains(marker),
            "log output leaked to stdout: {stdout}"
        );
    }
}
