use super::*;

#[test]
fn feathered_mask_mesh_and_filter_uniforms_share_renderer_admission() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let p = project();
    let mut scene = Scene::new(&p);
    scene.sample(&p, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let masks = MaskCache::default().build(&scene, 1., false).unwrap();
    assert_eq!(masks.len(), 1);
    let mesh_bytes = (masks[0].vertices.len() as u64 * 24).max(24).next_multiple_of(4);
    // Two feather axes and one combine pass have three distinct 624-byte
    // uniforms. Texture admission is a separate pending part of #61.
    let expected = mesh_bytes + 3 * 624;
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let mut images = HashMap::new();
    let mut bytes = 0;
    assert!(gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
        &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap());
    assert_eq!(gpu.uniforms.len(), 3);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + expected);
    renderer.submit_tracked(encoder.finish(), |_| {});
    let pixels = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
    assert_eq!(pixels[32 * 64 + 32], 255);
    assert_eq!(pixels[32 * 64 + 2], 0);
    drop(gpu);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mut retired = before;
    // Peak capacity records the completed work; it never falls with live bytes.
    retired.peak_admitted_bytes += expected;
    assert_eq!(budget.info().unwrap(), retired);
}

fn sampled(p: &Project) -> (Scene, Vec<MaskRaster>) {
    let mut scene = Scene::new(p);
    scene.sample(p, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let masks = MaskCache::default().build(&scene, 1., false).unwrap();
    (scene, masks)
}

fn assert_refusal(error: &RenderError, required: u64, limit: u64, phase: &str) {
    assert_eq!(error.resource_rejection(), Some(&crate::ResourceRejection::Budget {
        scope: crate::ResourceBudgetScope::RenderAllocations,
        required_bytes: required, budget_bytes: limit,
    }));
    for context in ["mask layer=1 mask=1 frame=0", phase, "phase=allocate"] {
        assert!(error.to_string().contains(context), "missing {context}: {error}");
    }
}

#[test]
fn every_mask_uniform_refusal_preserves_partial_capacity_and_recovers_pixels() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let base = crate::FIXED_RENDERER_ALLOCATION_BYTES;
    let layout = layout(&renderer.device);
    let (scene, masks) = sampled(&project());
    let mesh = masks[0].vertices.len() as u64 * 24;
    for existing in 0..3 {
        let mut gpu = admitted_gpu(&renderer, &layout);
        let mut images = HashMap::new();
        let mut bytes = 0;
        let limit = base + mesh + existing as u64 * 624 + 623;
        budget.set_limit(limit).unwrap();
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        let error = gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
            &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap_err();
        assert_refusal(&error, limit + 1, limit,
            if existing < 2 { "phase=feather" } else { "phase=combine" });
        assert!(error.to_string().contains("buffer size=624 phase=allocate label=mask pass uniform"));
        assert_eq!(gpu.uniforms.len(), existing);
        assert_eq!(budget.info().unwrap().admitted_bytes, base + mesh + existing as u64 * 624);
        assert_eq!(gpu.uploads, 0);
        assert!(gpu.fingerprints.is_empty());
        let ids: Vec<_> = gpu.uniforms.iter().map(|slot| slot.buffer.allocation().id()).collect();
        drop(encoder);
        budget.set_limit(8 << 20).unwrap();
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        assert!(gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
            &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap());
        assert_eq!(gpu.uniforms.len(), 3);
        assert_eq!(ids, gpu.uniforms[..existing].iter()
            .map(|slot| slot.buffer.allocation().id()).collect::<Vec<_>>());
        renderer.submit_tracked(encoder.finish(), |_| {});
        gpu.allocation_uploads.submitted();
        let pixels = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
        assert_eq!(pixels[32 * 64 + 32], 255);
        assert_eq!(pixels[32 * 64 + 2], 0);
        drop(gpu);
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, base);
    }
    renderer.check_health().unwrap();
}

#[test]
fn refused_mask_mesh_replacement_keeps_identity_and_recovers_changed_pixels() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let mut p = project();
    let (scene, masks) = sampled(&p);
    let mut images = HashMap::new();
    let mut bytes = 0;
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
        &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap();
    renderer.submit_tracked(encoder.finish(), |_| {});
    gpu.allocation_uploads.submitted();
    let old_pixels = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
    let old_id = gpu.meshes[&(1, 1)].1.allocation().id();
    let old_fingerprint = gpu.meshes[&(1, 1)].0;
    let before = budget.info().unwrap();
    p.layers[0].masks[0].path.nodes[0].geometry.value[0] = 24.;
    let (changed, changed_masks) = sampled(&p);
    let required = before.admitted_bytes + changed_masks[0].vertices.len() as u64 * 24;
    budget.set_limit(before.admitted_bytes).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let error = gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
        &changed, &changed_masks, &mut images, &mut bytes, 0, 128 << 20).unwrap_err();
    assert_refusal(&error, required, before.admitted_bytes, "phase=vertices");
    assert_eq!(gpu.meshes[&(1, 1)].1.allocation().id(), old_id);
    assert_eq!(gpu.meshes[&(1, 1)].0, old_fingerprint);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    drop(encoder);
    assert_eq!(read(&renderer, &images[&TextureKey::Mask(1)]._texture), old_pixels);
    // A normal unchanged cache hit stays usable even below the new limit.
    budget.set_limit(1).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    assert!(!gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
        &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap());
    drop(encoder);
    budget.set_limit(8 << 20).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    assert!(gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
        &changed, &changed_masks, &mut images, &mut bytes, 0, 128 << 20).unwrap());
    assert_ne!(gpu.meshes[&(1, 1)].1.allocation().id(), old_id);
    renderer.submit_tracked(encoder.finish(), |_| {});
    gpu.allocation_uploads.submitted();
    let pixels = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
    assert_ne!(pixels, old_pixels);
    assert_eq!(pixels[32 * 64 + 32], 255);
    renderer.check_health().unwrap();
}

#[test]
fn queued_mask_uniform_outlives_drop_without_renewal_by_later_encoders() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.queue.submit([]);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let budget = renderer.allocation_budget();
    let base = crate::FIXED_RENDERER_ALLOCATION_BYTES;
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let texture = filled(&renderer, 0);
    let view = texture.create_view(&Default::default());
    let target = filled(&renderer, 0);
    let target_view = target.create_view(&Default::default());
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    gpu.filter(&renderer.device, &renderer.queue, &mut encoder, &mut 0,
        1, mask_plan::uniform(64, 64), &view, &view, &target_view).unwrap();
    drop(encoder);
    drop(gpu);
    assert_eq!(budget.info().unwrap().admitted_bytes, base + 624);
    let later = renderer.allocation_command_encoder(&Default::default()).unwrap();
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, base);
    drop(later);
    renderer.check_health().unwrap();
}

#[test]
fn abandoned_mask_commands_keep_all_buffers_without_renewing_completed_scopes() {
    enum Work {
        Encoding { _encoder: crate::AllocationEncoder },
        Finished { _commands: crate::AllocatedCommandBuffer },
    }
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.queue.submit([]);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let budget = renderer.allocation_budget();
    let base = crate::FIXED_RENDERER_ALLOCATION_BYTES;
    let layout = layout(&renderer.device);
    let (scene, masks) = sampled(&project());
    let buffers = masks[0].vertices.len() as u64 * 24 + 3 * 624;
    for finished in [false, true] {
        let mut gpu = admitted_gpu(&renderer, &layout);
        let mut images = HashMap::new();
        let mut bytes = 0;
        let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
        gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
            &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap();
        let work = if finished { Work::Finished { _commands: encoder.finish() } }
            else { Work::Encoding { _encoder: encoder } };
        drop(gpu);
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, base + buffers);
        let later = renderer.allocation_command_encoder(&Default::default()).unwrap();
        drop(work);
        assert_eq!(budget.info().unwrap().admitted_bytes, base);
        drop(later);
    }
    renderer.check_health().unwrap();
}
