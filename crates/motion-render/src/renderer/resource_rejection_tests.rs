use super::*;
use motion_core::{EffectInstance, Layer, Project};

#[test]
fn resource_pressure_does_not_poison_effect_programs_and_the_same_frame_can_retry() {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "ascii")
        .unwrap();
    assert!(!definition.resources.is_empty());
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "ASCII", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64.; 2],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    let baseline = renderer.texture_bytes;
    // Inject logical pressure without allocating 128 MiB of GPU textures.
    renderer.texture_bytes = TEXTURE_BUDGET;
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let error = renderer
        .encode_preview(&scene, &target.view, 64, 64, &mut encoder, None)
        .unwrap_err();
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::Budget {
            scope: ResourceBudgetScope::SourceTextures,
            ..
        })
    ));
    assert!(renderer.effect_gpu.builder.program_errors.is_empty());
    assert!(renderer.effect_gpu.state.failed_program.is_none());
    drop(encoder);
    renderer.texture_bytes = baseline;
    let actual = renderer.capture(&scene, &target).unwrap().0;
    let mut reference = pollster::block_on(Renderer::headless()).unwrap();
    let target = reference.capture_target(64, 64).unwrap();
    assert_eq!(actual, reference.capture(&scene, &target).unwrap().0);
    assert!(renderer.effect_gpu.builder.frame.diagnostics.is_empty());
}

#[test]
fn nested_diagnostics_preserve_the_childs_budget_refusal() {
    use motion_core::{parse_commands, Engine, MAIN_COMPOSITION};
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "ascii")
        .unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project
        .layers
        .push(Layer::solid(1, "nested", [64.; 2], [32., 32., 0.], [1.; 4]));
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply_batch(
            parse_commands(
                &serde_json::json!({"op":"composition","composition":MAIN_COMPOSITION,
        "action":{"kind":"precompose","objects":[1],"name":"inside"}})
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    let mut project = engine.project().clone();
    let child_id = project.compositions[0].id.clone();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    renderer.capture(&scene, &target).unwrap();
    // Keep the existing child target so the refusal originates in child preparation.
    let baseline = renderer.texture_bytes;
    project.compositions[0].layers[0]
        .effects
        .push(EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [64.; 2],
        ));
    project.rebuild_plugin_dependencies();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.texture_bytes = TEXTURE_BUDGET;
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let error = renderer
        .encode_preview(&scene, &target.view, 64, 64, &mut encoder, None)
        .unwrap_err();
    assert!(error.to_string().contains(&child_id));
    assert!(error.to_string().contains("effect resources"));
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::Budget {
            scope: ResourceBudgetScope::SourceTextures,
            ..
        })
    ));
    drop(encoder);
    renderer.texture_bytes = baseline;
    renderer.capture(&scene, &target).unwrap();
    renderer.check_health().unwrap();
}
