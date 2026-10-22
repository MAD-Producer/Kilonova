use super::*;

fn capacity(renderer: &Renderer) -> (u64, u64) {
    let alignment = renderer.device.limits().min_uniform_buffer_offset_alignment as usize;
    ((std::mem::size_of::<DrawUniform>().div_ceil(alignment) * alignment * MAX_LAYERS) as u64,
        (motion_effects::shader::UNIFORM_BYTES.div_ceil(alignment) * alignment * MAX_LAYERS) as u64)
}

fn project(matte: bool) -> Project {
    use motion_core::compositing::*;
    let mut p = Project::new(64, 64, 30, 60).unwrap();
    p.background = [0.; 4];
    p.camera.created = false;
    p.layers = vec![
        Layer::solid(1, "coverage", [24.; 2], [32., 32., 0.], [0.5, 0.5, 0.5, 0.5]),
        Layer::solid(2, "receiver", [32.; 2], [32., 32., 0.], [0.9, 0.2, 0.1, 0.7]),
    ];
    if matte {
        p.layers[0].visible = false;
        p.layers[1].track_matte = Some(TrackMatte { source: 1, mode: MatteMode::Alpha, hide_source: true });
    } else {
        p.layers[1].blend = LayerBlend { mode: BlendMode::Multiply, space: BlendSpace::Srgb };
    }
    p.validate().unwrap();
    p
}

fn sample(p: &Project) -> Scene {
    let mut scene = Scene::new(p).unwrap();
    scene.sample(p, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    scene
}

fn drain(renderer: &Renderer) {
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.submit_tracked(encoder.finish(), |_| {});
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

#[test]
fn matte_and_transfer_system_resources_share_renderer_admission() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    let gpu = crate::compositing_gpu::CompositingGpu::new(&renderer.device,
        &renderer.queue, &renderer.uniform_layout, &renderer.image_layout,
        &renderer.sampler, renderer.target_format, &budget, renderer.allocation_uploads.clone()).unwrap();
    let actual_bytes = gpu.extract_buffer.size() + gpu.blend_buffer.size() + 1;
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + actual_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations + 3);
    drop(gpu);
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.submit_tracked(encoder.finish(), |_| {});
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
    renderer.check_health().unwrap();
}

#[test]
fn every_constructor_cut_point_releases_partial_capacity_before_recovery() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    drain(&renderer);
    let budget = renderer.allocation_budget().clone();
    let base = budget.info().unwrap();
    let (extract, blend) = capacity(&renderer);
    for (partial, next, marker) in [
        (0, extract, "matte source uniforms"),
        (extract, blend, "layer transfer values"),
        (extract + blend, 1, "texture"),
    ] {
        let limit = base.admitted_bytes + partial;
        budget.set_limit(limit).unwrap();
        let error = renderer.ensure_compositing_gpu(7., "initialize").unwrap_err();
        assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: limit + next, budget_bytes: limit,
        }));
        assert!(error.to_string().contains("compositing frame=7 phase=initialize"), "{error}");
        assert!(error.to_string().contains(marker), "{error}");
        assert!(renderer.compositing_gpu.is_none());
        assert_eq!(renderer.compositing_pipeline_set_builds, 0);
        let after = budget.info().unwrap();
        assert_eq!(after.admitted_bytes, base.admitted_bytes);
        assert_eq!(after.live_allocations, base.live_allocations);
        assert_eq!(after.encoder_scopes, base.encoder_scopes);
        // Refusal has no queue writes that need a future submit to retire.
    }
    budget.set_limit(8 << 20).unwrap();
    renderer.ensure_compositing_gpu(7., "initialize").unwrap();
    assert_eq!(renderer.compositing_pipeline_set_builds, 1);
    assert_eq!(budget.info().unwrap().admitted_bytes, base.admitted_bytes + extract + blend + 1);
    drop(renderer.compositing_gpu.take());
    assert_eq!(budget.info().unwrap().admitted_bytes, base.admitted_bytes + 1);
    drain(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, base.admitted_bytes);
    renderer.check_health().unwrap();
}

#[test]
fn normal_matte_refusal_preserves_cached_pixels_and_recovers_without_partial_publication() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let scene = sample(&project(true));
    let target = renderer.capture_target(64, 64).unwrap();
    let expected = renderer.capture(&scene, &target).unwrap().0;
    assert!(expected[(32 * 64 + 32) * 4 + 3].abs_diff(89) <= 3);
    assert_eq!(expected[3], 0);
    drop(renderer.compositing_gpu.take());
    drain(&renderer);
    let budget = renderer.allocation_budget().clone();
    let base = budget.info().unwrap();
    budget.set_limit(base.admitted_bytes).unwrap();
    let error = renderer.capture(&scene, &target).unwrap_err();
    assert!(error.resource_rejection().is_some());
    assert!(error.to_string().contains("compositing frame=0 phase=initialize"), "{error}");
    assert!(renderer.compositing_gpu.is_none());
    assert_eq!(renderer.compositing_pipeline_set_builds, 1);
    assert_eq!(budget.info().unwrap().admitted_bytes, base.admitted_bytes);
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
    budget.set_limit(8 << 20).unwrap();
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, expected);
    assert_eq!(renderer.compositing_pipeline_set_builds, 2);
    renderer.check_health().unwrap();
}

#[test]
fn exposure_local_refusal_keeps_retained_inputs_and_recovers_the_same_pixels() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let scene = sample(&project(false));
    let rasters: Vec<_> = scene.layers.iter().map(|layer|
        renderer.materialize_layer(&scene, layer.id, 64, 64, false).unwrap().unwrap().0).collect();
    let refs: Vec<_> = rasters.iter().collect();
    let target = renderer.capture_target(64, 64).unwrap();
    renderer.draw_layer_rasters(&scene, &refs, &target.view, 64, 64).unwrap();
    let expected = renderer.read_target(&target).unwrap();
    assert!(expected[(32 * 64 + 32) * 4 + 3] > 150);
    assert_eq!(expected[3], 0);
    drop(renderer.compositing_gpu.take());
    drain(&renderer);
    let budget = renderer.allocation_budget().clone();
    let base = budget.info().unwrap();
    budget.set_limit(base.admitted_bytes).unwrap();
    let error = renderer.draw_layer_rasters(&scene, &refs, &target.view, 64, 64).unwrap_err();
    assert!(error.resource_rejection().is_some());
    assert!(error.to_string().contains("compositing frame=0 phase=local_initialize"), "{error}");
    assert!(renderer.compositing_gpu.is_none());
    assert_eq!(renderer.compositing_pipeline_set_builds, 1);
    assert_eq!(budget.info().unwrap().admitted_bytes, base.admitted_bytes);
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
    budget.set_limit(8 << 20).unwrap();
    renderer.draw_layer_rasters(&scene, &refs, &target.view, 64, 64).unwrap();
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
    assert_eq!(renderer.compositing_pipeline_set_builds, 2);
    renderer.check_health().unwrap();
}

#[test]
fn queued_zero_extract_and_blend_uploads_retire_after_native_completion_without_renewal() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    drain(&renderer);
    let mut p = project(true);
    p.layers[1].blend = motion_core::compositing::LayerBlend {
        mode: motion_core::compositing::BlendMode::Multiply,
        space: motion_core::compositing::BlendSpace::Srgb,
    };
    let scene = sample(&p);
    let target = renderer.capture_target(64, 64).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.encode(&scene, &target.view, 64, 64, &mut encoder, None).unwrap();
    drop(encoder);
    // Keep working textures outside this system-upload retirement observation.
    // Their independent command and binding lifetimes are covered in matte_textures.
    let _working_textures: Vec<_> = {
        let gpu = renderer.compositing_gpu.as_ref().unwrap();
        gpu.mattes.values().map(|t| t.texture.clone())
            .chain(gpu.source.iter().map(|t| t.texture.clone())).collect()
    };
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    let (extract, blend) = capacity(&renderer);
    drop(renderer.compositing_gpu.take());
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    let later = renderer.allocation_command_encoder(&Default::default()).unwrap();
    drain(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes - extract - blend - 1);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations - 3);
    drop(later);
    renderer.check_health().unwrap();
}

#[test]
fn abandoned_compositing_commands_retain_capacity_until_their_own_drop() {
    enum Work { Encoding(crate::AllocationEncoder), Finished(crate::AllocatedCommandBuffer) }
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    drain(&renderer);
    let budget = renderer.allocation_budget().clone();
    let base = budget.info().unwrap().admitted_bytes;
    let (extract, blend) = capacity(&renderer);
    for finished in [false, true] {
        let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
        renderer.ensure_compositing_gpu(0., "initialize").unwrap();
        let gpu = renderer.compositing_gpu.as_ref().unwrap();
        encoder.clear_buffer(&gpu.extract_buffer, 0, None);
        encoder.clear_buffer(&gpu.blend_buffer, 0, None);
        let work = if finished { Work::Finished(encoder.finish()) } else { Work::Encoding(encoder) };
        drop(renderer.compositing_gpu.take());
        drain(&renderer);
        assert_eq!(budget.info().unwrap().admitted_bytes, base + extract + blend + 1);
        let later = renderer.allocation_command_encoder(&Default::default()).unwrap();
        match work { Work::Encoding(encoder) => drop(encoder), Work::Finished(buffer) => drop(buffer) }
        assert_eq!(budget.info().unwrap().admitted_bytes, base);
        drop(later);
    }
    renderer.check_health().unwrap();
}
