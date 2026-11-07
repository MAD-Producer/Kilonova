use super::*;
use std::sync::Arc;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn worker_native_validation_preserves_original_cause_and_thread_scope_ownership() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let uploads = Arc::new(crate::allocation_uploads::AllocationUploads::new(
        renderer.queue.clone(),
    ));
    let layout = crate::effect_gpu::bindings(&renderer.device, "worker compilation", 1);
    let mut gpu = crate::effect_gpu::EffectGpu::new(
        &renderer.device,
        &renderer.queue,
        &layout,
        Default::default(),
        uploads.clone(),
        renderer.image_capabilities.clone(),
        renderer.effect_gpu.builder.registry.clone(),
    )
    .unwrap();
    let mut project = motion_model::Project::new(16, 16, 30, 60).unwrap();
    let mut layer = motion_model::Layer::solid(
        1,
        "native worker",
        [16.; 2],
        [8., 8., 0.],
        [0.3, 0.5, 0.7, 1.],
    );
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|x| x.id == "tint")
        .unwrap();
    layer.effects.push(motion_model::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        layer.size,
    ));
    project.layers.push(layer);
    gpu.state.driver_pipeline_cache = crate::driver_pipeline_cache::DriverPipelineCache::new(
        &renderer.device,
        &renderer.adapter_info,
    )
    .unwrap()
    .cache;
    let mut scene = crate::Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    gpu.builder.build(&scene, &[0], 16, 16, true).unwrap();
    let original = gpu.builder.programs.first().unwrap().clone();
    let mut bad = original.clone();
    let mut shader = (*bad.shader).clone();
    shader.wgsl.push_str("\nfn worker_native_invalid( {");
    bad.shader = Arc::new(shader);
    let compiler = gpu.state.pipeline_compiler(&renderer.device);
    let expected_program = original.key.clone();
    // A live owner-thread scope must not capture the worker's validation error.
    let owner_scope = renderer
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let worker = std::thread::spawn(move || {
        let failure = compiler
            .compile(&bad, wgpu::TextureFormat::Rgba8Unorm, None)
            .unwrap_err();
        let RenderError::EffectPipeline(native) = &failure else {
            panic!("native validation cause was flattened: {failure}");
        };
        assert_eq!(native.program(), expected_program);
        assert!(matches!(native.native(), wgpu::Error::Validation { .. }));
        let cause = native
            .source()
            .unwrap()
            .downcast_ref::<wgpu::Error>()
            .unwrap();
        assert!(std::ptr::eq(cause, native.native()));
        assert!(native.related().is_empty());
        assert!(native.poll_error().is_none());
        let valid = compiler
            .compile(&original, wgpu::TextureFormat::Rgba8Unorm, None)
            .unwrap();
        (failure, valid)
    });
    let (failure, _valid) = worker.join().unwrap();
    assert!(
        failure.to_string().contains("worker_native_invalid"),
        "{failure}"
    );
    assert!(pollster::block_on(owner_scope.pop()).is_none());
    renderer.check_health().unwrap();
    renderer.queue.submit([]);
    uploads.submitted();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}
