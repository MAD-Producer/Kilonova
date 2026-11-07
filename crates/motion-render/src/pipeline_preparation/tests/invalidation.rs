use super::*;
use motion_effects::{EffectPackage, Registry, image_contract::ChannelPrecision};

fn package(body: &str) -> Arc<EffectPackage> {
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let mut definition = manifest
        .effects
        .iter()
        .find(|d| d.id == "tint")
        .unwrap()
        .clone();
    manifest.id = "com.motionstudio.tests.cache-invalidation".into();
    definition.passes = vec![motion_effects::PassDefinition {
        shader: "shaders/color.wgsl".into(),
        entry: "main_fx".into(),
    }];
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    std::fs::write(
        directory.path().join("shaders/color.wgsl"),
        format!("fn main_fx(p:vec2<f32>)->vec4<f32> {{ {body} }}"),
    )
    .unwrap();
    manifest.effects = vec![definition];
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(directory.path()).unwrap())
            .unwrap(),
    )
}

fn input(package: &Arc<EffectPackage>) -> (Registry, Scene) {
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.packages.insert(
        (
            package.manifest.id.clone(),
            package.manifest.version.clone(),
            package.hash.clone(),
        ),
        package.clone(),
    );
    let mut project = motion_model::Project::new(16, 16, 30, 60).unwrap();
    let mut layer = motion_model::Layer::solid(1, "invalidation", [32.; 2], [8., 8., 0.], [1.; 4]);
    layer.effects.push(motion_model::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        layer.size,
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    (registry, scene)
}

fn prepared_capture(renderer: &mut Renderer, scene: &Scene) -> Vec<u8> {
    let before = renderer.effect_pipeline_builds();
    let job = renderer
        .prepare_effect_pipeline_job(vec![scene.clone()], 16, 16, CONTEXT)
        .unwrap();
    let prepared = run(job).unwrap();
    assert!(prepared.pipeline_count() > 0);
    assert!(
        renderer.effect_pipeline_builds() > before,
        "changed native program/format reused an old pipeline"
    );
    assert!(matches!(
        renderer
            .install_prepared_effect_pipelines(prepared, CONTEXT)
            .unwrap(),
        PipelinePreparationInstall::Installed { .. }
    ));
    let builds = renderer.effect_pipeline_builds();
    let foreground = renderer.effect_pipeline_cache_info().foreground_builds;
    let target = renderer
        .sdr_capture_target(16, 16, crate::OutputTransform::SDR_CLIP)
        .unwrap();
    let pixels = renderer.capture_sdr(scene, &target).unwrap().0.pixels;
    assert_eq!(
        renderer.effect_pipeline_builds(),
        builds,
        "capture compiled after preparation"
    );
    assert_eq!(
        renderer.effect_pipeline_cache_info().foreground_builds,
        foreground
    );
    pixels
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn restored_driver_cache_rejects_changed_package_code_and_working_format() {
    let first = package("return vec4<f32>(1.0, 0.0, 0.0, 1.0);");
    let second = package("return vec4<f32>(0.0, 1.0, 0.0, 1.0);");
    assert_eq!(first.manifest.id, second.manifest.id);
    assert_eq!(first.manifest.version, second.manifest.version);
    assert_ne!(first.hash, second.hash);
    let (registry, scene) = input(&first);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("private.cache");
    let mut cold = pollster::block_on(Renderer::headless()).unwrap();
    cold.set_effect_registry(registry.clone());
    let red = prepared_capture(&mut cold, &scene);
    assert!(red.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
    let supported = cold.driver_pipeline_cache_info().supported;
    assert_eq!(cold.save_driver_pipeline_cache(&path).unwrap(), supported);
    let mut warm = pollster::block_on(Renderer::headless()).unwrap();
    warm.set_effect_registry(registry);
    // SAFETY: the real driver produced the file above in our private directory.
    let load = unsafe { warm.restore_driver_pipeline_cache(&path) }.unwrap();
    assert_eq!(
        load,
        if supported {
            crate::DriverPipelineCacheLoad::Restored
        } else {
            crate::DriverPipelineCacheLoad::Unsupported
        }
    );
    assert_eq!(prepared_capture(&mut warm, &scene), red);

    // Complete a real old-package batch, then replace its registry before install.
    let job = warm
        .prepare_effect_pipeline_job(vec![scene], 16, 16, CONTEXT)
        .unwrap();
    let old = run(job).unwrap();
    let ticket = old.ticket.clone();
    let (registry, changed) = input(&second);
    warm.set_effect_registry(registry.clone());
    assert!(ticket.cancelled());
    assert_eq!(
        warm.install_prepared_effect_pipelines(old, CONTEXT)
            .unwrap(),
        PipelinePreparationInstall::Superseded
    );
    let green = prepared_capture(&mut warm, &changed);
    assert!(green.chunks_exact(4).all(|p| p == [0, 255, 0, 255]));
    assert_ne!(green, red);
    let mut oracle = pollster::block_on(Renderer::headless()).unwrap();
    oracle.set_effect_registry(registry.clone());
    let target = oracle
        .sdr_capture_target(16, 16, crate::OutputTransform::SDR_CLIP)
        .unwrap();
    assert_eq!(
        oracle.capture_sdr(&changed, &target).unwrap().0.pixels,
        green
    );

    let job = warm
        .prepare_effect_pipeline_job(vec![changed.clone()], 16, 16, CONTEXT)
        .unwrap();
    let old = run(job).unwrap();
    let ticket = old.ticket.clone();
    let change = warm
        .prepare_working_precision(ChannelPrecision::Float16)
        .unwrap()
        .unwrap();
    warm.commit_working_precision(change).unwrap();
    assert!(ticket.cancelled());
    assert_eq!(
        warm.install_prepared_effect_pipelines(old, CONTEXT)
            .unwrap(),
        PipelinePreparationInstall::Superseded
    );
    let float_green = prepared_capture(&mut warm, &changed);
    let mut float_oracle = pollster::block_on(Renderer::headless_with_working_precision(
        ChannelPrecision::Float16,
        crate::AllocationBudget::default(),
    ))
    .unwrap();
    float_oracle.set_effect_registry(registry);
    let target = float_oracle
        .sdr_capture_target(16, 16, crate::OutputTransform::SDR_CLIP)
        .unwrap();
    assert_eq!(
        float_oracle
            .capture_sdr(&changed, &target)
            .unwrap()
            .0
            .pixels,
        float_green
    );
    assert!(float_green.chunks_exact(4).all(|p| p == [0, 255, 0, 255]));
    for renderer in [&cold, &warm, &oracle, &float_oracle] {
        renderer.check_health().unwrap();
    }
    if let Some(directory) = std::env::var_os("MOTION_INVALIDATION_EVIDENCE") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("invalidation.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "adapter":warm.adapter_info.name,"backend":format!("{:?}",warm.adapter_info.backend),
            "driver":warm.adapter_info.driver,"driverCacheSupported":supported,"load":load,
            "firstHash":first.hash,"secondHash":second.hash,"pixels":16*16,
            "allPixelsMatchColdOracle":true,"stalePackageRejected":true,"staleFormatRejected":true,
            "foregroundBuilds":warm.effect_pipeline_cache_info().foreground_builds
        })).unwrap()).unwrap();
        std::fs::write(directory.join("red.rgba"), red).unwrap();
        std::fs::write(directory.join("green.rgba"), green).unwrap();
        std::fs::write(directory.join("float-green.rgba"), float_green).unwrap();
    }
}
