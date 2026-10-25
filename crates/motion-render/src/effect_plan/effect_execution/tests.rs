use super::*;
use motion_model::{EffectInstance, Layer, Project};

fn effect(id: u64, name: &str) -> EffectInstance {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == name)
        .unwrap();
    EffectInstance::new(
        id,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [32., 24.],
    )
}
fn fixture(fused: bool) -> (PlanBuilder, Scene) {
    let mut project = Project::new(32, 24, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "effect IR", [32., 24.], [16., 12., 0.], [1.; 4]);
    layer.effects = vec![effect(11, "invert"), effect(12, "tint")];
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    builder.set_pointwise_fusion(fused);
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    (builder, scene)
}

#[test]
fn effect_ir_owns_individual_operations_and_lowered_tasks_consume_checked_order() {
    for fused in [false, true] {
        let (builder, _) = fixture(fused);
        let execution = builder.effect_execution().unwrap();
        assert_eq!(execution.plan().nodes.len(), 3);
        assert_eq!(execution.checked().total_cost().passes, 7);
        assert_eq!(
            execution
                .plan()
                .nodes
                .iter()
                .filter(|n| n.origin.effect.is_some())
                .count(),
            2
        );
        assert!(execution.allows_group(&[0, 1]));
        assert!(!execution.allows_group(&[1, 0]));
        let tasks = execution.native_tasks();
        assert_eq!(tasks.len(), if fused { 1 } else { 2 });
        assert_eq!(tasks.iter().flat_map(|t| t.members.iter()).count(), 2);
        assert_eq!(
            tasks
                .iter()
                .flat_map(|t| t.passes.clone())
                .collect::<Vec<_>>(),
            (0..builder.frame.passes.len()).collect::<Vec<_>>()
        );
        assert_eq!(
            builder.effect_pass_order(0),
            (0..builder.frame.passes.len()).collect::<Vec<_>>()
        );
        assert_eq!(builder.frame.passes.len(), if fused { 3 } else { 7 });
        assert_eq!(execution.bindings()[1].original_passes, 0..4);
        assert_eq!(execution.bindings()[2].original_passes, 4..7);
        assert_eq!(execution.bindings()[2].output_region, [0., 0., 32., 24.]);
    }
}

#[test]
fn numeric_rebuilds_reuse_actual_code_hashes_and_public_code_mutation_invalidates_one() {
    let (mut builder, mut scene) = fixture(true);
    let identities = |builder: &PlanBuilder| {
        builder
            .effect_execution()
            .unwrap()
            .plan()
            .nodes
            .iter()
            .map(|n| (n.origin.effect, n.code_identity))
            .collect::<BTreeMap<_, _>>()
    };
    let before = identities(&builder);
    let hashes = builder.execution_code_cache_info().hashes;
    scene.effects[1].values[0][0] = 0.4;
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    assert_eq!(builder.execution_code_cache_info().hashes, hashes);
    assert_eq!(identities(&builder), before);
    let program = builder.resolved[0].as_ref().unwrap().programs[0];
    Arc::make_mut(&mut builder.programs[program as usize].shader)
        .wgsl
        .push_str("\n// changed code identity\n");
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    let after = identities(&builder);
    assert_ne!(before[&Some(11)], after[&Some(11)]);
    assert_eq!(before[&Some(12)], after[&Some(12)]);
    assert_eq!(builder.execution_code_cache_info().hashes, hashes + 1);
}

#[test]
fn code_identity_metadata_is_bounded_and_does_not_retain_shader_owners() {
    let (builder, _) = fixture(false);
    let program = &builder.programs[3];
    let owners = Arc::strong_count(&program.shader);
    let mut codes = CodeIdentities::default();
    for id in 0..MAX_CODE_IDENTITIES as u32 + 3 {
        codes.get(id, program);
    }
    assert_eq!(codes.info().entries, MAX_CODE_IDENTITIES);
    assert_eq!(Arc::strong_count(&program.shader), owners);
    let hashes = codes.info().hashes;
    codes.get(MAX_CODE_IDENTITIES as u32 + 2, program);
    assert_eq!(codes.info().hashes, hashes);
    codes.clear();
    assert_eq!(codes.info().entries, 0);
}

#[test]
fn noncanonical_boundaries_are_rejected_before_new_compiles_and_recover() {
    let (mut builder, scene) = fixture(false);
    let compiles = builder.pointwise_fusion_report().portable_compiles;
    let last = builder.frame.passes.len() - 1;
    builder.frame.passes[last].uniform.output_mode[1] = 1.;
    let error = builder.lower_effect_execution(&scene, &[]).unwrap_err();
    assert!(error.to_string().contains("phase=effect_ir"));
    assert!(error.to_string().contains("canonical linear premultiplied"));
    assert!(builder.effect_execution().is_none());
    assert_eq!(
        builder.pointwise_fusion_report().portable_compiles,
        compiles
    );
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    assert!(builder.effect_execution().is_some());
}

#[test]
fn effect_cycle_is_diagnosed_before_native_preparation_and_healthy_input_recovers() {
    let mut project = Project::new(32, 24, 30, 60).unwrap();
    for id in 1..=2 {
        let mut layer = Layer::solid(id, "cycle", [32., 24.], [16., 12., 0.], [1.; 4]);
        layer.effects.push(effect(id * 10 + 1, "displacement_map"));
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    for (index, e) in scene.effects.iter_mut().enumerate() {
        e.image_input = Some(motion_model::EffectImageInput::Layer {
            layer: if index == 0 { 2 } else { 1 },
            stage: motion_model::EffectImageStage::Effects,
        });
    }
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    let error = builder.build(&scene, &[0], 32, 24, true).unwrap_err();
    assert!(error.to_string().contains("phase=effect_ir"));
    assert!(error.to_string().contains("layer=1 effect=Some(11)"));
    assert!(error.to_string().contains("layer=2 effect=Some(21)"));
    let mut cause = &error;
    while let RenderError::Diagnostic { cause: source, .. }
    | RenderError::Context { cause: source, .. } = cause
    {
        cause = source;
    }
    assert!(
        matches!(
            cause,
            RenderError::ExecutionPlan(ExecutionPlanError::Cycle { total: 2, .. })
        ),
        "{error:?}"
    );
    assert!(builder.effect_execution().is_none());
    scene.effects[1].image_input = None;
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    let execution = builder.effect_execution().unwrap();
    let order: Vec<_> = execution
        .snapshot_order()
        .iter()
        .map(|&i| builder.frame.draws[i].layer)
        .collect();
    assert_eq!(order, [2]);
    assert_eq!(
        execution
            .plan()
            .nodes
            .iter()
            .filter(|n| n.origin.effect.is_some())
            .count(),
        2
    );
}

#[test]
fn complete_effect_graph_negotiates_caps_types_and_logical_budgets() {
    let (builder, _) = fixture(false);
    let plan = builder.effect_execution().unwrap().plan();
    let mut limits = native_effect_limits(builder.device_dimension);
    limits.nodes = 2;
    assert!(matches!(
        plan.check(&native_execution_capabilities(), limits),
        Err(ExecutionPlanError::Budget {
            resource: "nodes",
            required: 3,
            maximum: 2,
            ..
        })
    ));
    assert!(matches!(
        plan.check(
            &BTreeSet::new(),
            native_effect_limits(builder.device_dimension)
        ),
        Err(ExecutionPlanError::UnsupportedCapability { .. })
    ));
    let mut wrong = plan.clone();
    wrong.nodes[2].inputs[0].expected.image.precision = ChannelPrecision::Float16;
    assert!(matches!(
        wrong.check(
            &native_execution_capabilities(),
            native_effect_limits(builder.device_dimension)
        ),
        Err(ExecutionPlanError::TypeMismatch { .. })
    ));
}

#[test]
fn virtual_source_views_do_not_inherit_physical_texture_limits() {
    let mut project = Project::new(32, 24, 30, 60).unwrap();
    project.layers.push(Layer::solid(
        1,
        "large ordinary layer",
        [32768.; 2],
        [16., 12., 0.],
        [1.; 4],
    ));
    let mut small = Layer::solid(
        2,
        "small effected layer",
        [32., 24.],
        [16., 12., 0.],
        [1.; 4],
    );
    small.effects.push(effect(21, "invert"));
    project.layers.push(small);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    builder.device_dimension = 64;
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    let source = &builder.effect_execution().unwrap().plan().nodes[0];
    assert_eq!(source.output_type.rect.size, [32768; 2]);
    assert_eq!(source.cost.passes, 0);
    assert!(
        builder
            .frame
            .scratch_allocations
            .iter()
            .all(|a| a.width <= 64 && a.height <= 64)
    );
    // A genuinely materialized image still obeys the physical device bound.
    let mut large = scene.clone();
    large.layers[1].source_rect = [0., 0., 128., 128.];
    large.layers[1].source_size = [128.; 2];
    assert!(builder.build(&large, &[0], 32, 24, true).is_err());
}

#[test]
fn invalid_origins_are_bounded_and_rejected_before_code_preparation() {
    let (mut builder, mut scene) = fixture(false);
    let healthy = scene.composition_id.clone();
    for invalid in [
        String::new(),
        " \n ".into(),
        "ok\nbad".into(),
        "x".repeat(129),
        "私".repeat(65536),
    ] {
        builder.code_identities.clear();
        let hashes = builder.execution_code_cache_info().hashes;
        scene.composition_id = invalid;
        let error = builder.lower_effect_execution(&scene, &[]).unwrap_err();
        assert!(error.to_string().contains("phase=effect_ir"));
        assert!(!error.to_string().contains('\n'));
        assert!(error.to_string().len() < 1024);
        assert!(builder.effect_execution().is_none());
        assert_eq!(builder.execution_code_cache_info().hashes, hashes);
        let mut cause = &error;
        while let RenderError::Diagnostic { cause: source, .. }
        | RenderError::Context { cause: source, .. } = cause
        {
            cause = source;
        }
        assert!(matches!(
            cause,
            RenderError::ExecutionPlan(ExecutionPlanError::InvalidOrigin { .. })
        ));
    }
    scene.composition_id = healthy;
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    assert!(builder.effect_execution().is_some());
}
