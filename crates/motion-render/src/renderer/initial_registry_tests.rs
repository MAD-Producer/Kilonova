use super::*;
use motion_effects::{builtin, Registry};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn host_registry_install_preserves_pixels_and_disabled_state_without_archive_owners() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new_profiled_with_precision_changes_and_registry(
        &instance, None, TARGET_FORMAT, false, Default::default(), Registry::default(),
    )).unwrap();
    assert!(renderer.effect_gpu.builder.registry.packages.is_empty());
    let mut project = Project::new(16, 16, 30, 60).unwrap();
    project.layers.push(motion_model::Layer::solid(1, "registry probe", [16.; 2], [8., 8., 0.], [0.8, 0.2, 0.1, 1.]));
    let target = renderer.capture_target(16, 16).unwrap();
    let sample = |project: &Project| {
        let mut scene = Scene::new(project).unwrap();
        scene.sample(project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
        scene
    };
    let plain = renderer.capture(&sample(&project), &target).unwrap().0;
    let package = builtin::package().unwrap();
    let definition = package.manifest.effects.iter().find(|d| d.id == "invert").unwrap();
    project.layers[0].effects.push(motion_model::EffectInstance::new(
        1, &package.manifest.id, &package.manifest.version, &package.hash, definition, [16.; 2],
    ));
    project.rebuild_plugin_dependencies();
    let key = (package.manifest.id.clone(), package.manifest.version.clone(), package.hash.clone());
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    renderer.set_effect_registry(registry.clone());
    assert_eq!(renderer.effect_gpu.builder.registry.packages.len(), 1);
    assert!(Arc::ptr_eq(&renderer.effect_gpu.builder.registry.packages[&key], &package));
    let inverted = renderer.capture(&sample(&project), &target).unwrap().0;
    assert_ne!(plain, inverted, "installed package must execute native effect pixels");
    assert_eq!(&inverted[(8 * 16 + 8) * 4..][..4], &[51, 204, 229, 255]);
    registry.disabled.insert(key.clone());
    renderer.set_effect_registry(registry);
    assert!(renderer.effect_gpu.builder.registry.disabled.contains(&key));
    // A disabled dependency is a precise render error, not a silent bypass.
    let error = renderer.capture(&sample(&project), &target).unwrap_err();
    assert!(error.to_string().contains("layer 1, effect 1"));
    assert!(error.to_string().contains("is disabled"));
    project.layers[0].effects.clear();
    project.rebuild_plugin_dependencies();
    assert_eq!(renderer.capture(&sample(&project), &target).unwrap().0, plain);
    assert_eq!(renderer.effect_gpu.builder.registry.packages.len(), 1);
}
