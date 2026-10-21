use super::*;

#[test]
fn effect_parameter_refusal_reaches_original_reply_file_and_stderr_at_default_level() {
    const CHILD: &str = "MOTION_EFFECT_PARAMETER_ADMISSION_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::effect_admission::effect_parameter_refusal_reaches_original_reply_file_and_stderr_at_default_level";
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
        let mut layer =
            motion_core::Layer::solid(1, "Admission test", [8.; 2], [4., 4., 0.], [1.; 4]);
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
        let mut scene = motion_render::Scene::new(&project);
        scene
            .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let mut builder = motion_render::effect_plan::PlanBuilder::new(
            motion_effects::Registry::new_with_builtins().unwrap(),
        )
        .unwrap();
        let plan = builder.build(&scene, &[0], 8, 8, true).unwrap();
        let scratch = motion_render::effect_plan::scratch_capacity_bytes(&plan.scratch_sizes);
        let required = SYSTEM_BYTES + scratch + motion_effects::shader::UNIFORM_BYTES as u64;
        let budget = motion_render::AllocationBudget::new(required - 1).unwrap();
        let mut renderer = pollster::block_on(
            motion_render::Renderer::headless_with_allocation_budget(budget.clone()),
        )
        .unwrap();
        let failure = renderer.preflight_effects(&scene, 8, 8).unwrap_err();
        assert_eq!(
            failure.resource_rejection(),
            Some(&motion_render::ResourceRejection::Budget {
                scope: motion_render::ResourceBudgetScope::RenderAllocations,
                required_bytes: required,
                budget_bytes: required - 1
            })
        );
        let original = failure.to_string();
        assert!(original.contains("effect parameters pass=0 phase=allocate"));
        assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + scratch);
        let (reply, receive) = channel::<Result<Value, String>>();
        CURRENT_COMMAND.set(Some(Flight {
            request: 204,
            operation: "test_effect_prepare",
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
        budget.set_limit(SYSTEM_BYTES + (1 << 20)).unwrap();
        renderer.preflight_effects(&scene, 8, 8).unwrap();
        renderer.check_health().unwrap();
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
    let cause = "admitted render allocations exceed configured combined budget";
    let error = file
        .lines()
        .find(|line| line.contains("error #204 test_effect_prepare "))
        .unwrap();
    for context in [
        cause,
        "effect parameters pass=0 phase=allocate",
        "label=reusable effect parameters",
        "required_bytes=",
        "budget_bytes=",
        "ms:",
    ] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #204", "logging initialized"] {
        assert!(
            !stdout.contains(marker),
            "log output leaked to stdout: {stdout}"
        );
    }
}
