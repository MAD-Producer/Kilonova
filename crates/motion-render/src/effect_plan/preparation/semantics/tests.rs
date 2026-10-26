use super::*;
use motion_model::{EffectInstance, Layer, Project};

fn fixture(names: &[&str]) -> (PlanBuilder, Scene) {
    let package = motion_effects::builtin::package().unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "semantic region", [32., 24.], [32., 32., 0.], [1.; 4]);
    for (i, name) in names.iter().enumerate() {
        let def = package
            .manifest
            .effects
            .iter()
            .find(|d| d.id == *name)
            .unwrap();
        layer.effects.push(EffectInstance::new(
            i as u64 + 11,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            def,
            layer.size,
        ));
    }
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    (
        PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap(),
        scene,
    )
}

fn set(scene: &mut Scene, effect: usize, id: &str, value: f32) {
    let effect = &mut scene.effects[effect];
    let index = effect.param_ids.iter().position(|p| p == id).unwrap();
    effect.values[index][0] = value;
}

#[test]
fn native_edge_modes_follow_parameter_identity_instead_of_sorted_storage_position() {
    let (mut builder, mut scene) = fixture(&["glow"]);
    set(&mut scene, 0, "edge", 3.);
    set(&mut scene, 0, "radius", 5.);
    set(&mut scene, 0, "effect_opacity", 37.);
    builder.build(&scene, &[0], 64, 64, true).unwrap();
    let passes: Vec<_> = builder
        .frame
        .passes
        .iter()
        .filter(|p| builder.programs[p.program as usize].package.is_some())
        .collect();
    assert!(!passes.is_empty());
    for pass in &passes {
        assert_eq!(pass.uniform.mode[0], 3., "mirror must not become clamp");
    }
    assert_eq!(passes.last().unwrap().uniform.output_mode[2], 0.37);
    assert_eq!(passes[0].uniform.region, [-5., -5., 42., 34.]);
}

#[test]
fn numeric_rebuilds_reuse_rules_and_bind_new_values_without_new_code() {
    let (mut builder, mut scene) = fixture(&["glow"]);
    set(&mut scene, 0, "radius", 5.);
    builder.build(&scene, &[0], 64, 64, true).unwrap();
    let rules = builder.resolved[0]
        .as_ref()
        .unwrap()
        .semantics
        .clone()
        .unwrap();
    let programs = builder.programs.len();
    let hashes = builder.execution_code_cache_info().hashes;
    set(&mut scene, 0, "radius", 7.5);
    builder.build(&scene, &[0], 64, 64, true).unwrap();
    assert!(Arc::ptr_eq(
        &rules,
        builder.resolved[0]
            .as_ref()
            .unwrap()
            .semantics
            .as_ref()
            .unwrap()
    ));
    assert_eq!(builder.programs.len(), programs);
    assert_eq!(builder.execution_code_cache_info().hashes, hashes);
    let pass = builder
        .frame
        .passes
        .iter()
        .find(|p| builder.programs[p.program as usize].package.is_some())
        .unwrap();
    assert_eq!(pass.uniform.region, [-7.5, -7.5, 47., 39.]);
}

#[test]
fn owned_values_and_region_rules_survive_mutation_of_the_sampled_input() {
    let (mut builder, mut scene) = fixture(&["glow"]);
    set(&mut scene, 0, "radius", 5.);
    builder.synchronize(&scene).unwrap();
    let mut plan = builder.prepare_effect_inputs(&scene, &[0]).unwrap();
    let effect = plan.inputs[0].take().unwrap().unwrap();
    set(&mut scene, 0, "radius", 50.);
    assert_eq!(
        effect.output_region([10., 20., 32., 24.]).unwrap(),
        [5., 15., 42., 34.]
    );
    // The rule uses the supplied accepted input, rather than retaining one
    // prospective downstream rectangle from an earlier admission attempt.
    assert_eq!(
        effect.output_region([-20., 30., 12., 8.]).unwrap(),
        [-25., 25., 22., 18.]
    );
}

#[test]
fn preview_refusal_rebinds_downstream_region_to_the_actual_accepted_input_and_recovers() {
    let (mut builder, mut scene) = fixture(&["glow", "glow"]);
    builder.device_dimension = 64;
    set(&mut scene, 0, "radius", 64.);
    set(&mut scene, 1, "radius", 2.);
    builder.build_preview(&scene, &[0], 64, 64).unwrap();
    assert_eq!(builder.frame.diagnostics.len(), 1);
    let pass = builder
        .frame
        .passes
        .iter()
        .find(|p| builder.programs[p.program as usize].package.is_some())
        .unwrap();
    assert_eq!(pass.uniform.region, [-2., -2., 36., 28.]);
    assert_eq!(pass.uniform.source_region, [0., 0., 32., 24.]);
    set(&mut scene, 0, "radius", 3.);
    builder.build_preview(&scene, &[0], 64, 64).unwrap();
    assert!(builder.frame.diagnostics.is_empty());
    assert_eq!(
        builder.frame.passes.last().unwrap().uniform.region,
        [-5., -5., 42., 34.]
    );
}

#[test]
fn input_dependent_recipe_errors_remain_errors_at_the_actual_region_boundary() {
    let package = motion_effects::builtin::package().unwrap();
    let mut def = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "glow")
        .unwrap()
        .clone();
    def.padding = BoundsExpr::Add {
        a: Box::new(BoundsExpr::Constant { value: 2. }),
        b: Box::new(BoundsExpr::Multiply {
            a: Box::new(BoundsExpr::Constant { value: -1. }),
            b: Box::new(BoundsExpr::InputSize { component: 0 }),
        }),
    };
    let rules = DefinitionSemantics::new(&def).unwrap();
    assert_eq!(
        rules
            .output_region(&[[0.; 4]; 32], [0., 0., 1., 1.])
            .unwrap(),
        [-1., -1., 3., 3.]
    );
    let error = rules
        .output_region(&[[0.; 4]; 32], [0., 0., 32., 24.])
        .unwrap_err();
    assert!(
        matches!(&error, RenderError::Invalid(message) if message == "negative output padding")
    );
    assert_eq!(error.to_string(), "GPU validation: negative output padding");
}

#[test]
fn rectangle_rules_keep_input_origin_size_and_vector_parameter_components_distinct() {
    let package = motion_effects::builtin::package().unwrap();
    let mut def = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "glow")
        .unwrap()
        .clone();
    def.output_bounds = Some(OutputBounds {
        x: BoundsExpr::Add {
            a: Box::new(BoundsExpr::InputOrigin { component: 0 }),
            b: Box::new(BoundsExpr::Parameter {
                id: "radius".into(),
                component: 0,
            }),
        },
        y: BoundsExpr::Add {
            a: Box::new(BoundsExpr::InputOrigin { component: 1 }),
            b: Box::new(BoundsExpr::Parameter {
                id: "color".into(),
                component: 1,
            }),
        },
        width: BoundsExpr::Multiply {
            a: Box::new(BoundsExpr::InputSize { component: 0 }),
            b: Box::new(BoundsExpr::Constant { value: 2. }),
        },
        height: BoundsExpr::InputSize { component: 1 },
    });
    let rules = DefinitionSemantics::new(&def).unwrap();
    let mut values = [[0.; 4]; 32];
    values[def.params.iter().position(|p| p.id == "radius").unwrap()][0] = 5.;
    values[def.params.iter().position(|p| p.id == "color").unwrap()][1] = 0.5;
    assert!(rules.direct_output());
    assert_eq!(
        rules.output_region(&values, [10., 20., 32., 24.]).unwrap(),
        [15., 20.5, 64., 24.]
    );
    assert_eq!(
        rules.output_region(&values, [-30., 7., 8., 4.]).unwrap(),
        [-25., 7.5, 16., 4.]
    );
}
