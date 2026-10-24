//! CPU planning at the supported layer/effect limits; not a GPU benchmark.
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_render::{effect_plan::PlanBuilder, Scene};
use std::time::Instant;

#[test]
fn maximum_pointwise_chain_count_has_stable_bounded_physical_scratch() {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "tint")
        .unwrap();
    let mut project = Project::new(64, 64, 30, 1).unwrap();
    for layer in 0..motion_model::MAX_LAYERS {
        let mut value = Layer::solid(
            layer as u64 + 1,
            "bounded planning",
            [32.; 2],
            [32., 32., 0.],
            [1.; 4],
        );
        for effect in 0..motion_effects::MAX_EFFECTS_PER_LAYER {
            value.effects.push(EffectInstance::new(
                (layer * motion_effects::MAX_EFFECTS_PER_LAYER + effect + 1) as u64,
                &package.manifest.id,
                &package.manifest.version,
                &package.hash,
                definition,
                [32.; 2],
            ));
        }
        project.layers.push(value);
    }
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &ExpressionEvaluator)
        .unwrap();
    let mut builder =
        PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    let mut packed = Vec::new();
    for iteration in 0..2 {
        let began = Instant::now();
        let plan = builder.build(&scene, &[0], 64, 64, true).unwrap();
        eprintln!("scratch planning iteration={iteration} passes={} allocations={} bytes={} elapsed_us={}",
            plan.passes.len(), plan.scratch_allocations.len(), plan.scratch_bytes(), began.elapsed().as_micros());
        assert_eq!(plan.draws.len(), motion_model::MAX_LAYERS);
        assert_eq!(
            plan.passes.len(),
            motion_model::MAX_LAYERS * (1 + 3 * motion_effects::MAX_EFFECTS_PER_LAYER)
        );
        assert_eq!(plan.scratch_allocations.len(), 3);
        assert_eq!(plan.scratch_bytes(), 3 * 32 * 32 * 4);
        assert!(plan
            .scratch_bindings
            .iter()
            .all(|p| p.output as i32 != p.input && p.output as i32 != p.source));
        let mut current = vec![0; plan.buffer_bytes(&scene)];
        plan.write(&scene, &mut current).unwrap();
        if iteration == 0 {
            packed = current;
        } else {
            assert_eq!(packed, current);
        }
    }
}
