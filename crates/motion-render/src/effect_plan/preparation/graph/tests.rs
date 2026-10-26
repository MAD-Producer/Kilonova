use super::*;
use motion_model::{EffectInstance, Layer, Project};

fn fixture(names: &[&str]) -> (PlanBuilder, Scene) {
    let package = motion_effects::builtin::package().unwrap();
    let mut project = Project::new(32, 24, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "semantic ownership", [32., 24.], [16., 12., 0.], [1.; 4]);
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
            [32., 24.],
        ));
    }
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    builder.set_pointwise_fusion(false);
    builder.synchronize(&scene).unwrap();
    (builder, scene)
}

#[test]
fn semantic_producers_and_recipes_exist_before_raster_and_follow_admitted_inputs() {
    let (builder, mut scene) = fixture(&["glow", "glow"]);
    for effect in &mut scene.effects {
        let radius = effect.param_ids.iter().position(|p| p == "radius").unwrap();
        effect.values[radius] = [2., 0., 0., 0.];
    }
    let preparation = builder.prepare_effect_inputs(&scene, &[0]).unwrap();
    let mut graph = preparation
        .semantic_frame(
            &scene,
            &builder.resolved,
            ChannelPrecision::Float32,
            &[1.],
            &[],
        )
        .unwrap();
    assert!(builder.frame.passes.is_empty() && builder.vector_cache.is_empty());
    assert_eq!(builder.cpu_preparation_info().vector_jobs, 0);
    let first = graph.layers[0].effects[0];
    let second = graph.layers[0].effects[1];
    assert_eq!(graph.nodes[first].effect, Some(0));
    assert_eq!(graph.nodes[second].input, Some(first));
    assert!(graph.output_region(second).is_err());
    scene.layers[0].source_rect = [99.; 4];
    scene.effects[1].values.fill([99.; 4]);
    assert_eq!(graph.output_region(first).unwrap(), [-2., -2., 36., 28.]);
    graph.admit(first, 0..0, [999.; 4], [999; 2]).unwrap();
    assert_eq!(graph.output_region(second).unwrap(), [-2., -2., 36., 28.]);
    graph
        .admit(second, 0..3, [-2., -2., 36., 28.], [36, 28])
        .unwrap();
    let value = graph.value_type(second).unwrap();
    assert_eq!(value.image.precision, ChannelPrecision::Float32);
    assert_eq!(value.rect.size, [36, 28]);
    assert_eq!(
        graph.region(graph.layers[0].output).unwrap(),
        [-2., -2., 36., 28.]
    );
}

#[test]
fn native_clocks_cannot_invent_or_discard_semantic_producers_and_recovery_is_exact() {
    let (mut builder, scene) = fixture(&["invert", "tint"]);
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    let hashes = builder.execution_code_cache_info().hashes;
    builder.pass_clocks.fill(None);
    let error = builder.lower_effect_execution(&scene, &[]).unwrap_err();
    assert!(
        error.to_string().contains("semantic effect ownership"),
        "{error:?}"
    );
    assert!(builder.effect_execution().is_none());
    assert_eq!(builder.execution_code_cache_info().hashes, hashes);
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    assert_eq!(
        builder
            .effect_execution()
            .unwrap()
            .plan()
            .nodes
            .iter()
            .filter(|n| n.origin.effect.is_some())
            .count(),
        2
    );
    builder.frame.passes.last_mut().unwrap().uniform.region[0] += 3.;
    let error = builder.lower_effect_execution(&scene, &[]).unwrap_err();
    assert!(
        error.to_string().contains("admitted semantic value"),
        "{error:?}"
    );
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    assert_eq!(
        builder.frame.passes.last().unwrap().uniform.region,
        [0., 0., 32., 24.]
    );
}

#[test]
fn backend_image_flags_cannot_replace_the_owned_raw_or_effected_selector() {
    let (mut builder, mut scene) = fixture(&["displacement_map"]);
    scene.effects[0].image_input = Some(EffectImageInput::Layer {
        layer: 1,
        stage: EffectImageStage::Source,
    });
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    let pass = builder
        .frame
        .passes
        .iter_mut()
        .find(|p| p.resource_input > 0)
        .unwrap();
    pass.uniform.params[30][1] = 1.;
    let error = builder.lower_effect_execution(&scene, &[]).unwrap_err();
    assert!(
        error.to_string().contains("sampled semantic input"),
        "{error:?}"
    );
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    assert_eq!(
        builder
            .frame
            .passes
            .iter()
            .find(|p| p.resource_input > 0)
            .unwrap()
            .uniform
            .params[30][1],
        0.
    );
}
