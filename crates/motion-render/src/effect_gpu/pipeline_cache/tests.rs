use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_pipeline_retirement_bounds_and_exact_static_proof() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let uploads = Arc::new(crate::allocation_uploads::AllocationUploads::new(
        renderer.queue.clone(),
    ));
    let layout = bindings(&renderer.device, "pipeline cache fixture", 1);
    let mut gpu = EffectGpu::new(
        &renderer.device,
        &renderer.queue,
        &layout,
        Default::default(),
        uploads.clone(),
        renderer.image_capabilities.clone(),
    )
    .unwrap();
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "tint")
        .unwrap();
    let mut project = motion_model::Project::new(16, 16, 30, 60).unwrap();
    let mut layer = motion_model::Layer::solid(1, "proof", [16.; 2], [8., 8., 0.], [1.; 4]);
    layer.effects.push(motion_model::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        layer.size,
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    gpu.builder.build(&scene, &[0], 16, 16, true).unwrap();
    gpu.state
        .prepare(
            &renderer.device,
            &renderer.queue,
            &layout,
            &gpu.builder,
            &scene,
            0,
            u64::MAX,
        )
        .unwrap();
    assert!(gpu.state.pipeline_builds > 0);
    let (&key, entry) = gpu.state.pipelines.iter().next().unwrap();
    let pipeline = entry.pipeline.clone();
    let shader = entry.shader.clone();
    gpu.state.pipelines.clear();
    let owners = Arc::strong_count(&shader);
    for id in 0..MAX_PREVIOUS_PIPELINES + 2 {
        gpu.state
            .insert_pipeline((id as u32, key.1), &shader, pipeline.clone());
    }
    gpu.state.retire_pipelines();
    let cache = gpu.state.pipeline_cache_info();
    assert_eq!(cache.previous_entries, MAX_PREVIOUS_PIPELINES);
    assert_eq!(
        cache.previous_wgsl_bytes,
        MAX_PREVIOUS_PIPELINES * shader.wgsl.len()
    );
    assert_eq!(
        Arc::strong_count(&shader),
        owners,
        "retirement must release compiled shader/package owners"
    );
    gpu.state.retire_pipelines();
    assert_eq!(
        gpu.state.pipeline_cache_info().previous_entries,
        MAX_PREVIOUS_PIPELINES,
        "registry changes without prepared programs must preserve the last proof"
    );
    let mut fresh = Arc::new((*shader).clone());
    let new_key = (999, key.1);
    gpu.state.reuse_pipeline(new_key, &fresh, false);
    assert!(gpu.state.pipelines.contains_key(&new_key));
    // A private strong proof prevents in-place mutation of a live program.
    assert!(Arc::get_mut(&mut fresh).is_none());
    for kind in 0..4 {
        let mut changed = (*shader).clone();
        let mut target = key.1;
        match kind {
            0 => changed.wgsl.push_str("\n// distinct actual code identity"),
            1 => changed.sprite = !changed.sprite,
            2 => changed.additive = !changed.additive,
            _ => target = target.wrapping_add(1),
        }
        gpu.state
            .reuse_pipeline((999, target), &Arc::new(changed), false);
        assert!(
            !gpu.state.pipelines.contains_key(&(999, target)),
            "invalid proof kind={kind}"
        );
    }
    gpu.state.reuse_pipeline(new_key, &shader, true);
    assert!(
        !gpu.state.pipelines.contains_key(&new_key),
        "actual fault injection must bypass proof"
    );
    gpu.state.pipelines.clear();
    // Explicit metadata-capacity fixture, sharing an actual successful GPU
    // handle. It tests retirement bounds, not compilation of oversized source.
    let mut padded = (*shader).clone();
    padded.wgsl = " ".repeat(MAX_PREVIOUS_WGSL_BYTES / 3 + 1);
    let padded = Arc::new(padded);
    for id in 0..4 {
        gpu.state
            .insert_pipeline((id, key.1), &padded, pipeline.clone());
    }
    gpu.state.retire_pipelines();
    let cache = gpu.state.pipeline_cache_info();
    assert_eq!(cache.previous_entries, 2);
    assert_eq!(cache.previous_wgsl_bytes, 2 * padded.wgsl.len());
    assert!(cache.previous_wgsl_bytes <= cache.previous_wgsl_byte_limit);
    gpu.state.insert_pipeline((0, key.1), &shader, pipeline);
    gpu.state.retire_pipelines();
    assert_eq!(
        gpu.state.pipeline_cache_info().previous_entries,
        1,
        "retirement replaces generations instead of retaining edit history"
    );
    renderer.queue.submit([]);
    uploads.submitted();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
}
