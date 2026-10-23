use motion_core::{EffectInstance, Layer, Project};
use motion_effects::{EffectPackage, Registry};
use motion_render::{Renderer, Scene};
use std::sync::Arc;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn repeated_registry_clone_keeps_preflighted_pipelines_but_replacements_and_disable_refresh() {
    let package = motion_effects::builtin::package().unwrap();
    let def = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "tint")
        .unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "effect", [64.; 2], [32., 32., 0.], [1., 0., 0., 1.]);
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        def,
        [64.; 2],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut registry = Registry::new_with_builtins().unwrap();
    renderer.set_effect_registry(registry.clone());
    renderer.preflight_effects(&scene, 64, 64).unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    let first = renderer.capture(&scene, &target).unwrap().0;
    let built = renderer.effect_pipeline_builds();
    assert!(built > 0);
    registry
        .diagnostics
        .push("non-rendering registry diagnostic".into());
    for _ in 0..3 {
        renderer.set_effect_registry(registry.clone());
        assert_eq!(first, renderer.capture(&scene, &target).unwrap().0);
        assert_eq!(built, renderer.effect_pipeline_builds());
    }
    let key = (
        package.manifest.id.clone(),
        package.manifest.version.clone(),
        package.hash.clone(),
    );
    let replacement = Arc::new(EffectPackage::from_bytes(package.bytes.clone()).unwrap());
    registry.packages.insert(key.clone(), replacement);
    renderer.set_effect_registry(registry.clone());
    assert_eq!(first, renderer.capture(&scene, &target).unwrap().0);
    assert!(renderer.effect_pipeline_builds() > built);
    let replacement_builds = renderer.effect_pipeline_builds();
    registry.disabled.insert(key.clone());
    renderer.set_effect_registry(registry.clone());
    assert!(renderer.capture(&scene, &target).is_err());
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    renderer
        .encode_preview(&scene, &target.view, 64, 64, &mut encoder, None)
        .unwrap();
    renderer.queue.submit(Some(encoder.finish()));
    let bypass = renderer.read_target(&target).unwrap();
    assert_ne!(first, bypass);
    registry.disabled.remove(&key);
    renderer.set_effect_registry(registry);
    assert_eq!(first, renderer.capture(&scene, &target).unwrap().0);
    assert!(renderer.effect_pipeline_builds() > replacement_builds);
}
