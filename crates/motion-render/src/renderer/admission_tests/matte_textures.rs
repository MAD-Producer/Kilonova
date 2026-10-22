use super::*;

fn initialized() -> Renderer {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.ensure_compositing_gpu(0., "initialize").unwrap();
    drain(&renderer);
    renderer
}

fn drain(renderer: &Renderer) {
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.submit_tracked(encoder.finish(), |_| {});
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

fn prepare(renderer: &mut Renderer, size: [u32; 2], needed: &[(u64, bool)], blend: bool) -> Result<bool> {
    renderer.compositing_gpu.as_mut().unwrap().prepare_targets(
        &renderer.device, &renderer.image_layout, &renderer.sampler,
        size, needed, 0, 1 << 20, blend)
}

fn paint(encoder: &mut wgpu::CommandEncoder, texture: &crate::resource_ledger::OwnedTexture, color: wgpu::Color) {
    let view = texture.create_view(&Default::default());
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("admitted matte fixture clear"), multiview_mask: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view, depth_slice: None, resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(color), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
    });
}

fn read(renderer: &Renderer, texture: &crate::resource_ledger::OwnedTexture) -> Vec<u8> {
    let row = texture.width() * texture.format().block_copy_size(None).unwrap();
    let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    // Test-only readback is external fixture capacity, not a product allocation.
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("matte fixture readback"), size: u64::from(padded) * u64::from(texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
    });
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
            offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(texture.height()),
        } }, texture.size());
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

fn accumulators(renderer: &Renderer) -> Vec<crate::effect_gpu::FxTexture> {
    (0..2).map(|_| crate::effect_gpu::admitted_texture(&renderer.device, &renderer.image_layout,
        &renderer.sampler, 4, 4, renderer.target_format, renderer.allocation_budget(), crate::TextureCategory::MatteBlendScratch).unwrap()).collect()
}

fn sample(project: &Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene.sample(project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    scene
}

#[test]
fn matte_and_blend_working_textures_share_admission_and_release_actual_capacity() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.ensure_compositing_gpu(0., "initialize").unwrap();
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.submit_tracked(encoder.finish(), |_| {});
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    let gpu = renderer.compositing_gpu.as_mut().unwrap();
    assert!(gpu.prepare_targets(&renderer.device, &renderer.image_layout, &renderer.sampler,
        [4, 4], &[(11, false), (12, true)], 0, 1 << 20, true).unwrap());
    let matte_bytes: u64 = gpu.mattes.values().map(|t|
        u64::from(t.texture.width()) * u64::from(t.texture.height())).sum();
    let source = &gpu.source.as_ref().unwrap().texture;
    let actual_bytes = matte_bytes + u64::from(source.width()) * u64::from(source.height()) * 4;
    assert_eq!(actual_bytes, 96);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + actual_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations + 3);
    gpu.release();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
    renderer.check_health().unwrap();
}

#[test]
fn refused_target_cut_points_keep_old_bindings_pixels_and_dimensions_until_complete_replacement() {
    let mut renderer = initialized();
    let budget = renderer.allocation_budget().clone();
    let acc = accumulators(&renderer);
    assert!(prepare(&mut renderer, [4, 4], &[(11, false), (12, true)], true).unwrap());
    renderer.compositing_gpu.as_mut().unwrap().prepare_blend_inputs(&renderer.device,
        &renderer.queue, &renderer.sampler, &acc, &[], true).unwrap();
    let gpu = renderer.compositing_gpu.as_ref().unwrap();
    let old = [gpu.mattes[&(11, false)].texture.clone(), gpu.mattes[&(12, true)].texture.clone(),
        gpu.source.as_ref().unwrap().texture.clone()];
    // Owned aliases pair with the native group aliases through the identity checks.
    let groups: Vec<_> = gpu.blend_inputs.iter().map(|input| input.group.clone()).collect();
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    for (index, texture) in old.iter().enumerate() {
        paint(&mut encoder, texture, wgpu::Color { r: (index + 1) as f64 * 0.25, g: 0., b: 0., a: 1. });
    }
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let old_pixels: Vec<_> = old.iter().map(|t| read(&renderer, t)).collect();
    assert!(old_pixels[0].iter().all(|v| v.abs_diff(64) <= 1));
    assert!(old_pixels[1].iter().all(|v| v.abs_diff(128) <= 1));
    let before = budget.info().unwrap();
    for (partial, next, marker) in [(0, 64, "matte layer=11"), (64, 64, "matte layer=12"),
        (128, 256, "blend source size=8x8")] {
        let limit = before.admitted_bytes + partial;
        budget.set_limit(limit).unwrap();
        let error = prepare(&mut renderer, [8, 8], &[(11, false), (12, true)], true).unwrap_err();
        assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations, required_bytes: limit + next, budget_bytes: limit,
        }));
        assert!(error.to_string().contains(marker), "{error}");
        let gpu = renderer.compositing_gpu.as_ref().unwrap();
        assert_eq!(&*gpu.mattes[&(11, false)].texture, &*old[0]);
        assert_eq!(&*gpu.mattes[&(12, true)].texture, &*old[1]);
        assert_eq!(&*gpu.source.as_ref().unwrap().texture, &*old[2]);
        assert_eq!(gpu.blend_inputs.iter().map(|input| &input.group).collect::<Vec<_>>(), groups.iter().collect::<Vec<_>>());
        let after = budget.info().unwrap();
        assert_eq!(after.admitted_bytes, before.admitted_bytes);
        assert_eq!(after.live_allocations, before.live_allocations);
        assert_eq!(after.encoder_scopes, before.encoder_scopes);
        for (texture, pixels) in old.iter().zip(&old_pixels) { assert_eq!(&read(&renderer, texture), pixels); }
    }
    budget.set_limit(8 << 20).unwrap();
    for (category_limit, scope, required) in [(1, ResourceBudgetScope::SourceTextures, 129),
        (200, ResourceBudgetScope::LayerCompositing, 768)] {
        let error = renderer.compositing_gpu.as_mut().unwrap().prepare_targets(
            &renderer.device, &renderer.image_layout, &renderer.sampler,
            [8, 8], &[(11, false), (12, true)], 0, category_limit, true).unwrap_err();
        assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
            scope, required_bytes: required, budget_bytes: category_limit,
        }));
        let gpu = renderer.compositing_gpu.as_ref().unwrap();
        assert_eq!(&*gpu.source.as_ref().unwrap().texture, &*old[2]);
        assert_eq!(gpu.blend_inputs.iter().map(|input| &input.group).collect::<Vec<_>>(), groups.iter().collect::<Vec<_>>());
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    }
    budget.set_limit(1).unwrap();
    assert!(!prepare(&mut renderer, [4, 4], &[(11, false), (12, true)], true).unwrap());
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    budget.set_limit(8 << 20).unwrap();
    assert!(prepare(&mut renderer, [8, 8], &[(11, false), (12, true)], true).unwrap());
    let gpu = renderer.compositing_gpu.as_ref().unwrap();
    assert_ne!(&*gpu.mattes[&(11, false)].texture, &*old[0]);
    assert_ne!(&*gpu.source.as_ref().unwrap().texture, &*old[2]);
    assert!(gpu.blend_inputs.is_empty());
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    for texture in gpu.mattes.values() { paint(&mut encoder, &texture.texture, wgpu::Color { r: 0.5, g: 0., b: 0., a: 1. }); }
    paint(&mut encoder, &gpu.source.as_ref().unwrap().texture, wgpu::Color::BLUE);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    assert!(read(&renderer, &gpu.mattes[&(11, false)].texture).iter().all(|v| v.abs_diff(128) <= 1));
    assert!(read(&renderer, &gpu.source.as_ref().unwrap().texture).chunks_exact(4).all(|p| p == [0, 0, 255, 255]));
    drop(groups);
    drop(old);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes - 96 + 384);
    renderer.check_health().unwrap();
}

#[test]
fn cached_transfer_bindings_keep_source_and_accumulators_admitted_and_render_after_owner_drop() {
    let mut renderer = initialized();
    let budget = renderer.allocation_budget().clone();
    let target = renderer.capture_target(4, 4).unwrap();
    let acc = accumulators(&renderer);
    prepare(&mut renderer, [4, 4], &[], true).unwrap();
    let mut project = Project::new(4, 4, 30, 1).unwrap();
    let mut layer = Layer::solid(1, "transfer", [4.; 2], [2., 2., 0.], [1.; 4]);
    layer.blend.mode = motion_core::compositing::BlendMode::Screen;
    project.layers.push(layer);
    let scene = sample(&project);
    let gpu = renderer.compositing_gpu.as_mut().unwrap();
    gpu.prepare_blend_inputs(&renderer.device, &renderer.queue, &renderer.sampler, &acc, &scene.layers, true).unwrap();
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    for texture in &acc { paint(&mut encoder, &texture.texture, wgpu::Color::RED); }
    paint(&mut encoder, &renderer.compositing_gpu.as_ref().unwrap().source.as_ref().unwrap().texture, wgpu::Color::BLUE);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    drain(&renderer);
    let before = budget.info().unwrap();
    drop(acc);
    drop(renderer.compositing_gpu.as_mut().unwrap().source.take());
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    let gpu = renderer.compositing_gpu.as_ref().unwrap();
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("retained transfer fixture"), multiview_mask: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view, depth_slice: None, resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
        });
        pass.set_pipeline(&gpu.blend);
        pass.set_bind_group(0, &gpu.blend_group, &[gpu.blend_offset(0)]);
        pass.set_bind_group(1, &gpu.blend_inputs[0].group, &[]);
        pass.draw(0..3, 0..1);
    }
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    assert!(renderer.read_target(&target).unwrap().chunks_exact(4).all(|p| p == [255, 0, 255, 255]));
    renderer.compositing_gpu.as_mut().unwrap().release();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes - 3 * 64);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations - 3);
    renderer.check_health().unwrap();
}

#[test]
fn matte_command_scopes_keep_working_textures_until_abandonment_or_actual_completion() {
    enum Work { Encoding(crate::AllocationEncoder), Finished(crate::AllocatedCommandBuffer) }
    let mut renderer = initialized();
    let budget = renderer.allocation_budget().clone();
    let base = budget.info().unwrap().admitted_bytes;
    for mode in 0..3 {
        let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
        prepare(&mut renderer, [4, 4], &[(11, false), (12, true)], true).unwrap();
        let gpu = renderer.compositing_gpu.as_ref().unwrap();
        for texture in gpu.mattes.values() { paint(&mut encoder, &texture.texture, wgpu::Color::WHITE); }
        paint(&mut encoder, &gpu.source.as_ref().unwrap().texture, wgpu::Color::BLUE);
        let work = if mode == 0 { Work::Encoding(encoder) } else { Work::Finished(encoder.finish()) };
        renderer.compositing_gpu.as_mut().unwrap().release();
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, base + 96);
        let later = renderer.allocation_command_encoder(&Default::default()).unwrap();
        match work {
            Work::Encoding(encoder) => drop(encoder),
            Work::Finished(buffer) if mode == 2 => { renderer.submit_allocated(buffer, |_| {}).unwrap(); drain(&renderer); },
            Work::Finished(buffer) => drop(buffer),
        }
        assert_eq!(budget.info().unwrap().admitted_bytes, base);
        drop(later);
    }
    renderer.check_health().unwrap();
}

#[test]
fn ordinary_and_local_target_refusals_preserve_last_pixels_and_recover_the_same_output() {
    fn draw(renderer: &mut Renderer, scene: &Scene, target: &CaptureTarget, rasters: &[LayerRaster], local: bool) -> Result<Vec<u8>> {
        if !local { return renderer.capture(scene, target).map(|result| result.0); }
        let refs: Vec<_> = rasters.iter().collect();
        renderer.draw_layer_rasters(scene, &refs, &target.view, 64, 64)?;
        renderer.read_target(target)
    }
    for local in [false, true] {
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let mut project = Project::new(64, 64, 30, 60).unwrap();
        project.background = [0.; 4];
        project.camera.created = false;
        project.layers = vec![
            Layer::solid(1, "back", [64.; 2], [32., 32., 0.], [0.2, 0.2, 0.2, 1.]),
            Layer::solid(2, "front", [64.; 2], [32., 32., 0.], [0.7, 0.7, 0.7, 0.5]),
        ];
        if local {
            project.layers[1].blend = motion_core::compositing::LayerBlend {
                mode: motion_core::compositing::BlendMode::Multiply,
                space: motion_core::compositing::BlendSpace::Srgb,
            };
        } else {
            project.layers[0] = Layer::solid(1, "coverage", [24.; 2], [32., 32., 0.], [0.5; 4]);
            project.layers[0].visible = false;
            project.layers[1] = Layer::solid(2, "receiver", [32.; 2], [32., 32., 0.], [0.9, 0.2, 0.1, 0.7]);
            project.layers[1].track_matte = Some(motion_core::compositing::TrackMatte {
                source: 1, mode: motion_core::compositing::MatteMode::Alpha, hide_source: true,
            });
        }
        project.validate().unwrap();
        let scene = sample(&project);
        let rasters: Vec<_> = if local { scene.layers.iter().map(|layer|
            renderer.materialize_layer(&scene, layer.id, 64, 64, false).unwrap().unwrap().0).collect() } else { vec![] };
        let target = renderer.capture_target(64, 64).unwrap();
        let expected = draw(&mut renderer, &scene, &target, &rasters, local).unwrap();
        let center = (32 * 64 + 32) * 4;
        if local {
            assert_eq!(expected[center + 3], 255);
            assert!(expected[center].abs_diff(43) <= 3);
        } else {
            assert!(expected[center + 3].abs_diff(89) <= 3);
            assert_eq!(expected[3], 0);
        }
        renderer.compositing_gpu.as_mut().unwrap().release();
        drain(&renderer);
        let budget = renderer.allocation_budget().clone();
        let before = budget.info().unwrap();
        budget.set_limit(before.admitted_bytes).unwrap();
        let error = draw(&mut renderer, &scene, &target, &rasters, local).unwrap_err();
        assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + if local { 64 * 64 * 4 } else { 64 * 64 },
            budget_bytes: before.admitted_bytes,
        }));
        let phase = if local { "phase=local_targets" } else { "phase=targets" };
        assert!(error.to_string().contains(phase), "{error}");
        assert_eq!(renderer.compositing_pipeline_set_builds, 1);
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(renderer.read_target(&target).unwrap(), expected);
        budget.set_limit(8 << 20).unwrap();
        assert_eq!(draw(&mut renderer, &scene, &target, &rasters, local).unwrap(), expected);
        assert_eq!(renderer.compositing_pipeline_set_builds, 1);
        renderer.check_health().unwrap();
    }
}
