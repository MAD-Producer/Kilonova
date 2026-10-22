use super::*;

#[test]
fn transfer_accumulators_share_admission_and_release_actual_capacity() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    assert!(renderer.layer_gpu.prepare_accumulators(&renderer.device, &renderer.image_layout,
        &renderer.sampler, renderer.target_format, Some([4, 4])).unwrap());
    let actual_bytes: u64 = renderer.layer_gpu.accumulators.iter().map(|target|
        u64::from(target.texture.width()) * u64::from(target.texture.height()) * 4).sum();
    assert_eq!(actual_bytes, 128);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + actual_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations + 2);
    assert!(renderer.layer_gpu.prepare_accumulators(&renderer.device, &renderer.image_layout,
        &renderer.sampler, renderer.target_format, None).unwrap());
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
    renderer.check_health().unwrap();
}

// Prepared additional native controls; append after the capacity control reproduces.
fn prepare(renderer: &mut Renderer, size: Option<[u32; 2]>) -> Result<bool> {
    renderer.layer_gpu.prepare_accumulators(&renderer.device, &renderer.image_layout,
        &renderer.sampler, renderer.target_format, size)
}

fn drain(renderer: &Renderer) {
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.submit_tracked(encoder.finish(), |_| {});
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

fn paint(encoder: &mut wgpu::CommandEncoder, texture: &crate::resource_ledger::OwnedTexture, color: wgpu::Color) {
    let view = texture.create_view(&Default::default());
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view, depth_slice: None, resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(color), store: wgpu::StoreOp::Store },
        })], ..Default::default()
    });
}

fn read(renderer: &Renderer, texture: &crate::resource_ledger::OwnedTexture, target: &CaptureTarget) -> Vec<u8> {
    let row = target.width * texture.format().block_copy_size(None).unwrap();
    let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    // Read back through an external test buffer. Product capture textures are
    // render/copy sources and intentionally do not carry COPY_DST usage.
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("accumulator fixture readback"), size: u64::from(padded) * u64::from(target.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
    });
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
            offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(target.height),
        } },
        wgpu::Extent3d { width: target.width, height: target.height, depth_or_array_layers: 1 });
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| send.send(result).unwrap());
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv_timeout(std::time::Duration::from_secs(5)).unwrap().unwrap();
    let mapped = buffer.get_mapped_range(..).unwrap();
    let pixels = mapped.chunks_exact(padded as usize).flat_map(|r| r[..row as usize].iter().copied()).collect();
    drop(mapped);
    buffer.unmap();
    pixels
}

#[test]
fn accumulator_cut_points_preserve_both_old_targets_pixels_and_dimensions_before_atomic_replacement() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(4, 4).unwrap();
    drain(&renderer);
    assert!(prepare(&mut renderer, Some([4, 4])).unwrap());
    let old: Vec<_> = renderer.layer_gpu.accumulators.iter().map(|t| t.texture.clone()).collect();
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    paint(&mut encoder, &old[0], wgpu::Color::RED);
    paint(&mut encoder, &old[1], wgpu::Color::BLUE);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let pixels: Vec<_> = old.iter().map(|t| read(&renderer, t, &target)).collect();
    assert!(pixels[0].chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
    assert!(pixels[1].chunks_exact(4).all(|p| p == [0, 0, 255, 255]));
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    for slot in 0..2 {
        let limit = before.admitted_bytes + slot * 256;
        budget.set_limit(limit).unwrap();
        let error = prepare(&mut renderer, Some([8, 8])).unwrap_err();
        assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: limit + 256, budget_bytes: limit,
        }));
        assert!(error.to_string().contains(&format!("layer accumulator slot={slot} size=8x8")), "{error}");
        for (index, texture) in old.iter().enumerate() {
            let current = &renderer.layer_gpu.accumulators[index].texture;
            assert_eq!(&**current, &**texture);
            assert_eq!(current.width(), 4);
            assert_eq!(current.height(), 4);
            assert_eq!(read(&renderer, current, &target), pixels[index]);
        }
        let after = budget.info().unwrap();
        assert_eq!(after.admitted_bytes, before.admitted_bytes);
        assert_eq!(after.live_allocations, before.live_allocations);
        assert_eq!(after.encoder_scopes, before.encoder_scopes);
    }
    budget.set_limit(1).unwrap();
    assert!(!prepare(&mut renderer, Some([4, 4])).unwrap());
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    budget.set_limit(8 << 20).unwrap();
    assert!(prepare(&mut renderer, Some([8, 8])).unwrap());
    for (index, texture) in renderer.layer_gpu.accumulators.iter().enumerate() {
        assert_ne!(&*texture.texture, &*old[index]);
        assert_eq!(texture.texture.width(), 8);
    }
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    for texture in &renderer.layer_gpu.accumulators { paint(&mut encoder, &texture.texture, wgpu::Color::GREEN); }
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    for texture in &renderer.layer_gpu.accumulators {
        assert!(read(&renderer, &texture.texture, &target).chunks_exact(4).all(|p| p == [0, 255, 0, 255]));
    }
    drop(old);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes - 128 + 512);
    assert!(prepare(&mut renderer, None).unwrap());
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes - 128);
    renderer.check_health().unwrap();
}

#[test]
fn accumulator_commands_keep_both_targets_until_their_own_abandonment_or_completion() {
    enum Work { Encoding(crate::AllocationEncoder), Finished(crate::AllocatedCommandBuffer) }
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    drain(&renderer);
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    for mode in 0..3 {
        let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
        prepare(&mut renderer, Some([4, 4])).unwrap();
        for texture in &renderer.layer_gpu.accumulators { paint(&mut encoder, &texture.texture, wgpu::Color::GREEN); }
        let work = if mode == 0 { Work::Encoding(encoder) } else { Work::Finished(encoder.finish()) };
        prepare(&mut renderer, None).unwrap();
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + 128);
        assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations + 2);
        let later = renderer.allocation_command_encoder(&Default::default()).unwrap();
        match work {
            Work::Encoding(encoder) => drop(encoder),
            Work::Finished(buffer) if mode == 2 => { renderer.submit_allocated(buffer, |_| {}).unwrap(); drain(&renderer); },
            Work::Finished(buffer) => drop(buffer),
        }
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
        drop(later);
    }
    renderer.check_health().unwrap();
}

#[test]
fn cached_transfer_bindings_keep_actual_layer_accumulators_renderable_after_cache_release() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(4, 4).unwrap();
    renderer.ensure_compositing_gpu(0., "initialize").unwrap();
    drain(&renderer);
    prepare(&mut renderer, Some([4, 4])).unwrap();
    let mut project = Project::new(4, 4, 30, 1).unwrap();
    let mut layer = Layer::solid(1, "transfer", [4.; 2], [2., 2., 0.], [1.; 4]);
    layer.blend.mode = motion_core::compositing::BlendMode::Screen;
    project.layers.push(layer);
    let mut scene = Scene::new(&project).unwrap();
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let gpu = renderer.compositing_gpu.as_mut().unwrap();
    gpu.prepare_targets(&renderer.device, &renderer.image_layout, &renderer.sampler,
        [4, 4], &[], 0, 1 << 20, true).unwrap();
    gpu.prepare_blend_inputs(&renderer.device, &renderer.queue, &renderer.sampler,
        &renderer.layer_gpu.accumulators, &scene.layers, true).unwrap();
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    for texture in &renderer.layer_gpu.accumulators { paint(&mut encoder, &texture.texture, wgpu::Color::RED); }
    paint(&mut encoder, &renderer.compositing_gpu.as_ref().unwrap().source.as_ref().unwrap().texture, wgpu::Color::BLUE);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    drain(&renderer);
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    prepare(&mut renderer, None).unwrap();
    drop(renderer.compositing_gpu.as_mut().unwrap().source.take());
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    {
        let gpu = renderer.compositing_gpu.as_ref().unwrap();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view, depth_slice: None, resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })], ..Default::default()
        });
        pass.set_pipeline(&gpu.blend);
        pass.set_bind_group(0, &gpu.blend_group, &[gpu.blend_offset(0)]);
        pass.set_bind_group(1, &gpu.blend_inputs[0].group, &[]);
        pass.draw(0..3, 0..1);
    }
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    assert!(renderer.read_target(&target).unwrap().chunks_exact(4).all(|p| p == [255, 0, 255, 255]));
    renderer.compositing_gpu.as_mut().unwrap().release();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes - 192);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations - 3);
    renderer.check_health().unwrap();
}

#[test]
fn ordinary_and_local_accumulator_refusals_preserve_output_then_recover_exact_pixels() {
    for local in [false, true] {
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let mut project = Project::new(64, 64, 30, 60).unwrap();
        project.background = [0.; 4];
        project.camera.created = false;
        project.layers = vec![
            Layer::solid(1, "back", [64.; 2], [32., 32., 0.], [0.2, 0.2, 0.2, 1.]),
            Layer::solid(2, "front", [64.; 2], [32., 32., 0.], [0.7, 0.7, 0.7, 0.5]),
        ];
        project.layers[1].blend = motion_core::compositing::LayerBlend {
            mode: motion_core::compositing::BlendMode::Multiply,
            space: motion_core::compositing::BlendSpace::Srgb,
        };
        project.validate().unwrap();
        let mut scene = Scene::new(&project).unwrap();
        scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
        let rasters: Vec<_> = if local { scene.layers.iter().map(|layer|
            renderer.materialize_layer(&scene, layer.id, 64, 64, false).unwrap().unwrap().0).collect() } else { vec![] };
        let target = renderer.capture_target(64, 64).unwrap();
        let draw = |renderer: &mut Renderer| -> Result<Vec<u8>> {
            if !local { return renderer.capture(&scene, &target).map(|r| r.0); }
            let refs: Vec<_> = rasters.iter().collect();
            renderer.draw_layer_rasters(&scene, &refs, &target.view, 64, 64)?;
            renderer.read_target(&target)
        };
        let expected = draw(&mut renderer).unwrap();
        let center = (32 * 64 + 32) * 4;
        assert_eq!(expected[center + 3], 255);
        assert!(expected[center].abs_diff(43) <= 3);
        prepare(&mut renderer, None).unwrap();
        renderer.compositing_gpu.as_mut().unwrap().blend_inputs.clear();
        drain(&renderer);
        let budget = renderer.allocation_budget().clone();
        let before = budget.info().unwrap();
        budget.set_limit(before.admitted_bytes).unwrap();
        let error = draw(&mut renderer).unwrap_err();
        assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + 64 * 64 * 4, budget_bytes: before.admitted_bytes,
        }));
        let phase = if local { "phase=local_accumulators" } else { "phase=accumulators" };
        assert!(error.to_string().contains(phase), "{error}");
        assert!(error.to_string().contains("slot=0 size=64x64"), "{error}");
        assert!(renderer.layer_gpu.accumulators.is_empty());
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(renderer.read_target(&target).unwrap(), expected);
        budget.set_limit(8 << 20).unwrap();
        assert_eq!(draw(&mut renderer).unwrap(), expected);
        assert_eq!(renderer.compositing_pipeline_set_builds, 1);
        renderer.check_health().unwrap();
    }
}
