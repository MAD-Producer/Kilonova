use super::*;
use motion_model::{EffectInstance, Layer, Project};

fn fixture(count: u64) -> (PlanBuilder, Scene) {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "displacement_map")
        .unwrap();
    let mut project = Project::new(32, 24, 30, 60).unwrap();
    for id in 1..=count {
        let mut layer = Layer::solid(
            id,
            "Dependency storage",
            [32., 24.],
            [16., 12., 0.],
            [0.2, 0.4, 0.6, 0.8],
        );
        let mut effect = EffectInstance::new(
            id,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [32., 24.],
        );
        if id > 1 {
            effect.image_input = Some(EffectImageInput::Layer {
                layer: id - 1,
                stage: EffectImageStage::Effects,
            });
        }
        layer.effects.push(effect);
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    builder.synchronize(&scene).unwrap();
    (builder, scene)
}

#[test]
fn dependency_validation_has_a_fixed_allocation_count_for_growing_effect_chains() {
    let mut measurements = Vec::new();
    for layers in [2, 8, 64, motion_model::MAX_LAYERS as u64] {
        let (builder, scene) = fixture(layers);
        let plan = builder.prepare_effect_inputs(&scene, &[0]).unwrap();
        for repetition in 0..4 {
            let allocations =
                crate::test_allocations::count(|| plan.check_dependencies(&scene).unwrap());
            println!("dependency_validation layers={layers} repetition={repetition} allocations={allocations}");
            measurements.push((layers, allocations));
        }
    }
    assert!(
        measurements
            .iter()
            .all(|(_, allocations)| *allocations <= 8),
        "{measurements:?}"
    );
    assert!(
        measurements.windows(2).all(|pair| pair[0].1 == pair[1].1),
        "{measurements:?}"
    );
}

// Independent functional-graph oracle: with one effect per layer, only an
// Effects-stage reference adds an edge to another final effect. Walking the
// authored references detects a cycle without constructing preparation nodes.
fn authored_cycle(targets: &[Option<usize>]) -> bool {
    for start in 0..targets.len() {
        let mut seen = vec![false; targets.len()];
        let mut cursor = Some(start);
        while let Some(node) = cursor {
            if seen[node] {
                return true;
            }
            seen[node] = true;
            cursor = targets[node];
        }
    }
    false
}

#[test]
fn all_four_layer_reference_graphs_preserve_cycle_refusal_and_recovery() {
    let (builder, mut scene) = fixture(4);
    let mut refused = 0;
    for encoded in 0..625 {
        let mut value = encoded;
        let targets: Vec<_> = (0..4)
            .map(|_| {
                let choice = value % 5;
                value /= 5;
                (choice != 0).then(|| choice - 1)
            })
            .collect();
        for (effect, target) in scene.effects.iter_mut().zip(&targets) {
            effect.image_input = target.map(|target| EffectImageInput::Layer {
                layer: target as u64 + 1,
                stage: EffectImageStage::Effects,
            });
        }
        match builder.prepare_effect_inputs(&scene, &[0]) {
            Ok(_) => assert!(!authored_cycle(&targets), "missed cycle {targets:?}"),
            Err(error) => {
                assert!(authored_cycle(&targets), "false cycle {targets:?}: {error}");
                let mut cause = &error;
                while let RenderError::Diagnostic { cause: inner, .. } = cause {
                    cause = inner;
                }
                assert!(
                    matches!(
                        cause,
                        RenderError::ExecutionPlan(ExecutionPlanError::Cycle { .. })
                    ),
                    "{error:?}"
                );
                assert!(error.to_string().contains("phase=effect_ir_preparation"));
                refused += 1;
            }
        }
    }
    assert!(refused > 0 && refused < 625);
    for effect in &mut scene.effects {
        effect.image_input = None;
    }
    let plan = builder.prepare_effect_inputs(&scene, &[0]).unwrap();
    assert_eq!(
        crate::test_allocations::count(|| plan.check_dependencies(&scene).unwrap()),
        0,
        "ordinary image chains must retain the no-graph fast path"
    );
    println!("dependency_graphs cases=625 refused={refused} recovery_allocations=0");
}

#[test]
fn repeated_inputs_and_stacked_effects_keep_final_producers_and_candidate_refusals() {
    let (mut builder, mut scene) = fixture(4);
    // The first effect's chain input and explicit raw image are the same
    // producer. Later effects must still refer to the layer's final output.
    scene.effects[0].image_input = Some(EffectImageInput::Layer {
        layer: 1,
        stage: EffectImageStage::Source,
    });
    let mut extra = scene.effects[0].clone();
    extra.instance = 99;
    scene.effects.insert(1, extra);
    builder.synchronize(&scene).unwrap();
    builder.prepare_effect_inputs(&scene, &[0]).unwrap();
    scene.effects[1].image_input = Some(EffectImageInput::Layer {
        layer: 2,
        stage: EffectImageStage::Effects,
    });
    let error = builder.prepare_effect_inputs(&scene, &[0]).err().unwrap();
    let message = error.to_string();
    assert!(message.contains("layer=1 effect=Some(99)"), "{message}");
    assert!(message.contains("layer=2 effect=Some(2)"), "{message}");
    assert!(!message.contains("layer=1 effect=Some(1)"), "{message}");
    let mut cause = &error;
    while let RenderError::Diagnostic { cause: inner, .. } = cause {
        cause = inner;
    }
    assert!(matches!(
        cause,
        RenderError::ExecutionPlan(ExecutionPlanError::Cycle { total: 2, .. })
    ));

    scene.effects[1].enabled = false;
    builder.prepare_effect_inputs(&scene, &[0]).unwrap();
    scene.effects[1].enabled = true;
    let opacity = scene.effects[1]
        .param_ids
        .iter()
        .position(|id| id == "effect_opacity")
        .unwrap();
    scene.effects[1].values[opacity][0] = 0.;
    builder.prepare_effect_inputs(&scene, &[0]).unwrap();
    scene.effects[1].values[opacity][0] = 100.;
    let original = scene.effects[1].values[0][0];
    scene.effects[1].values[0][0] = f32::NAN;
    let refused = builder.prepare_effect_inputs(&scene, &[0]).unwrap();
    assert!(
        matches!(refused.inputs[1], Some(Err(_))),
        "invalid candidate's cause must survive"
    );
    scene.effects[1].values[0][0] = original;
    scene.effects[1].image_input = Some(EffectImageInput::Layer {
        layer: 2,
        stage: EffectImageStage::Source,
    });
    builder.prepare_effect_inputs(&scene, &[0]).unwrap();
}
