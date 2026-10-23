use super::*;

fn sampled(project: &Project) -> (Scene, Vec<MaskRaster>) {
    let mut scene = Scene::new(project).unwrap();
    scene.sample(project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let masks = MaskCache::default().build(&scene, 1., false).unwrap();
    (scene, masks)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_mask_output_and_scratch_join_shared_admission_and_release_exactly() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let (scene, masks) = sampled(&project());
    let mut images = HashMap::new();
    let mut bytes = 0;
    let before = budget.info().unwrap();
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    assert!(gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
        &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap());
    let native_bytes = gpu.scratch.iter().map(|target| u64::from(target.texture.width())
        * u64::from(target.texture.height()) * u64::from(target.texture.format().block_copy_size(None).unwrap()))
        .chain(images.values().map(|image| u64::from(image._texture.width())
            * u64::from(image._texture.height()) * u64::from(image._texture.format().block_copy_size(None).unwrap())))
        .sum::<u64>();
    let buffer_bytes = gpu.meshes.values().map(|(_, buffer)| buffer.size())
        .chain(gpu.uniforms.iter().map(|slot| slot.buffer.size())).sum::<u64>();
    assert_eq!(native_bytes, 3 * 64 * 64);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + buffer_bytes + native_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations + 7);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    gpu.allocation_uploads.submitted();
    let pixels = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
    assert_eq!(pixels[32 * 64 + 32], 255);
    assert_eq!(pixels[32 * 64 + 2], 0);
    drop(gpu);
    drop(images);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn every_native_mask_texture_cut_point_preserves_old_targets_bindings_bytes_and_pixels() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let mut project = project();
    let (scene, masks) = sampled(&project);
    let mut images = HashMap::new();
    let mut bytes = 0;
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
        &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    gpu.allocation_uploads.submitted();
    let pixels = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
    assert_eq!(pixels[32 * 64 + 32], 255);
    assert_eq!(pixels[32 * 64 + 2], 0);
    let ids = gpu.scratch.iter().map(|t| t.texture.allocation().unwrap().id()).collect::<Vec<_>>();
    let output_id = images[&TextureKey::Mask(1)]._texture.allocation().unwrap().id();
    let bindings = gpu.uniforms.iter().map(|slot| slot.inputs.as_ref().unwrap().group.clone()).collect::<Vec<_>>();
    let before = budget.info().unwrap();
    let previous_bytes = bytes;
    project.layers[0].size = [128., 64.];
    let (changed, changed_masks) = sampled(&project);
    assert_eq!((changed_masks[0].width, changed_masks[0].height), (128, 64));
    for slot in 0..3 {
        let limit = before.admitted_bytes + slot * 8192;
        budget.set_limit(limit).unwrap();
        let error = gpu.prepare_textures(&renderer.device, &layout, &changed,
            &changed_masks, &mut images, &mut bytes, 0, 128 << 20).unwrap_err();
        assert_eq!(error.resource_rejection(), Some(&crate::ResourceRejection::Budget {
            scope: crate::ResourceBudgetScope::RenderAllocations,
            required_bytes: limit + 8192, budget_bytes: limit,
        }));
        for context in ["mask layer=1 mask=1 frame=0", "size=128x64 format=R8Unorm", "phase=allocate"] {
            assert!(error.to_string().contains(context), "missing {context}: {error}");
        }
        let phase = if slot == 2 { "phase=output_allocate".to_owned() }
            else { format!("phase=scratch_allocate slot={slot}") };
        assert!(error.to_string().contains(&phase), "{error}");
        assert_eq!(gpu.scratch_size, (64, 64));
        assert_eq!(gpu.scratch.iter().map(|t| t.texture.allocation().unwrap().id()).collect::<Vec<_>>(), ids);
        assert_eq!(images[&TextureKey::Mask(1)]._texture.allocation().unwrap().id(), output_id);
        assert_eq!(gpu.uniforms.iter().map(|slot| slot.inputs.as_ref().unwrap().group.clone()).collect::<Vec<_>>(), bindings);
        assert_eq!(bytes, previous_bytes);
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
        assert_eq!(read(&renderer, &images[&TextureKey::Mask(1)]._texture), pixels);
    }
    budget.set_limit(1).unwrap();
    assert!(!gpu.prepare_textures(&renderer.device, &layout, &scene, &masks,
        &mut images, &mut bytes, 0, 128 << 20).unwrap());
    budget.set_limit(8 << 20).unwrap();
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
        &changed, &changed_masks, &mut images, &mut bytes, 0, 128 << 20).unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    gpu.allocation_uploads.submitted();
    let recovered = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
    assert_eq!(gpu.scratch_size, (128, 64));
    assert_eq!(bytes, 128 * 64);
    assert_eq!(recovered[32 * 128 + 32], 255);
    assert_eq!(recovered[32 * 128 + 2], 0);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn mask_cached_input_binding_keeps_native_source_admission_and_renders_after_cache_clear() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let before = budget.info().unwrap();
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let (scene, masks) = sampled(&project());
    let mut images = HashMap::new();
    let mut bytes = 0;
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
        &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    gpu.allocation_uploads.submitted();
    read(&renderer, &images[&TextureKey::Mask(1)]._texture);
    let retained = gpu.uniforms[0].inputs.take().unwrap();
    gpu.input_binding_observations.clear();
    gpu.clear();
    drop(images);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + 3 * 624 + 4096);
    let output = filled(&renderer, 0);
    let view = output.create_view(&Default::default());
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("retained admitted mask input"), multiview_mask: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view, depth_slice: None, resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
        });
        pass.set_pipeline(&gpu.filters[0]);
        pass.set_bind_group(0, &gpu.uniforms[0].group, &[]);
        pass.set_bind_group(1, &retained.group, &[]);
        pass.draw(0..3, 0..1);
    }
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let pixels = read(&renderer, &output);
    assert_eq!(pixels[32 * 64 + 32], 255);
    drop(retained);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + 3 * 624);
    drop(gpu);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn mask_texture_and_buffer_commands_retain_capacity_until_their_own_retirement() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let layout = layout(&renderer.device);
    let (scene, masks) = sampled(&project());
    enum Work { Encoding(crate::AllocationEncoder), Finished(crate::AllocatedCommandBuffer) }
    for mode in 0..3 {
        let before = budget.info().unwrap();
        let mut gpu = admitted_gpu(&renderer, &layout);
        let mut images = HashMap::new();
        let mut bytes = 0;
        let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
        gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
            &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap();
        let allocated = 3 * 64 * 64 + masks[0].vertices.len() as u64 * 24 + 3 * 624;
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + allocated);
        let uploads = gpu.allocation_uploads.clone();
        let work = if mode == 0 { Work::Encoding(encoder) } else { Work::Finished(encoder.finish()) };
        drop(gpu);
        drop(images);
        let flush = renderer.device.create_command_encoder(&Default::default());
        renderer.submit_tracked(flush.finish(), |_| {});
        uploads.submitted();
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + allocated);
        let later = renderer.allocation_command_encoder(&Default::default()).unwrap();
        match work {
            Work::Encoding(encoder) => drop(encoder),
            Work::Finished(commands) if mode == 2 => {
                renderer.submit_allocated(commands, |_| {}).unwrap();
                renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            }
            Work::Finished(commands) => drop(commands),
        }
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
        drop(later);
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn two_renderers_share_mask_texture_capacity_and_partial_refusal_rolls_back() {
    let budget = crate::AllocationBudget::new(16 << 20).unwrap();
    let first = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let second = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let first_layout = layout(&first.device);
    let second_layout = layout(&second.device);
    let mut first_gpu = admitted_gpu(&first, &first_layout);
    let mut second_gpu = admitted_gpu(&second, &second_layout);
    let (scene, masks) = sampled(&project());
    let mut first_images = HashMap::new();
    let mut second_images = HashMap::new();
    let mut first_bytes = 0;
    let mut second_bytes = 0;
    let before = budget.info().unwrap();
    budget.set_limit(before.admitted_bytes + 3 * 4096).unwrap();
    first_gpu.prepare_textures(&first.device, &first_layout, &scene, &masks,
        &mut first_images, &mut first_bytes, 0, 128 << 20).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + 3 * 4096);
    let error = second_gpu.prepare_textures(&second.device, &second_layout, &scene, &masks,
        &mut second_images, &mut second_bytes, 0, 128 << 20).unwrap_err();
    assert!(error.to_string().contains("phase=scratch_allocate slot=0"), "{error}");
    drop(first_gpu);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + 4096);
    let error = second_gpu.prepare_textures(&second.device, &second_layout, &scene, &masks,
        &mut second_images, &mut second_bytes, 0, 128 << 20).unwrap_err();
    assert!(error.to_string().contains("phase=output_allocate"), "{error}");
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + 4096);
    assert!(second_gpu.scratch.is_empty());
    assert!(second_images.is_empty());
    assert_eq!(second_bytes, 0);
    drop(first_images);
    second_gpu.prepare_textures(&second.device, &second_layout, &scene, &masks,
        &mut second_images, &mut second_bytes, 0, 128 << 20).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + 3 * 4096);
    drop(second_gpu);
    drop(second_images);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
    first.check_health().unwrap();
    second.check_health().unwrap();
}
