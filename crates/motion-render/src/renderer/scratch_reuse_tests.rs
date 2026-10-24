use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn restored_effect_snapshots_clear_poisoned_padding_and_preserve_valid_pixels() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "tint")
        .unwrap();
    let mut project = Project::new(8, 8, 30, 1).unwrap();
    let mut layer =
        motion_core::Layer::solid(1, "snapshot restore", [8.; 2], [4., 4., 0.], [1.; 4]);
    layer.effects.push(motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [8.; 2],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.preflight_effects(&scene, 8, 8).unwrap();
    let frame = &renderer.effect_gpu.builder.frame;
    let output = renderer.effect_gpu.state.output(frame, 0).unwrap();
    assert_eq!((output.texture.width(), output.texture.height()), (8, 8));
    assert_eq!(output.texture.format(), wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut readback = renderer.capture_target(8, 8).unwrap();
    readback.texture = output.texture.clone();
    readback.view = output.texture.admitted_view().unwrap();
    // Both padded restoration and full-size copies use the production helper.
    for size in [(2, 2), (8, 8), (2, 2)] {
        let source = renderer.capture_target(size.0, size.1).unwrap();
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        for (view, color) in [
            (&*source.view, wgpu::Color::RED),
            (
                &output.view,
                wgpu::Color {
                    r: 1.,
                    g: 0.,
                    b: 1.,
                    a: 1.,
                },
            ),
        ] {
            drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("poison scratch before snapshot restoration"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }));
        }
        restore_effect_snapshot(&mut encoder, &source.texture, size, output);
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        let pixels = renderer.read_target(&readback).unwrap();
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < size.0 as usize && index / 8 < size.1 as usize {
                [255, 0, 0, 255]
            } else {
                [0; 4]
            };
            assert_eq!(pixel, expected, "size={size:?}, pixel={index}");
        }
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn more_than_eight_physical_scratch_allocations_render_reorder_and_recover_exact_pixels() {
    let package = motion_effects::builtin::package().unwrap();
    let mut project = Project::new(512, 512, 30, 60).unwrap();
    project.background = [0.; 4];
    for (index, size) in [[512., 1.], [1., 512.], [32., 32.]].into_iter().enumerate() {
        let mut layer = motion_core::Layer::solid(
            index as u64 + 1,
            "dynamic scratch pool",
            size,
            [256., 256., 0.],
            [0.2 + index as f32 * 0.2, 0.8, 0.4, 0.7],
        );
        for (effect, name) in ["gaussian_blur", "tint"].iter().enumerate() {
            let definition = package
                .manifest
                .effects
                .iter()
                .find(|d| &d.id == name)
                .unwrap();
            layer.effects.push(motion_core::EffectInstance::new(
                (index * 2 + effect + 1) as u64,
                &package.manifest.id,
                &package.manifest.version,
                &package.hash,
                definition,
                size,
            ));
        }
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    let sample = |project: &Project| {
        let mut scene = Scene::new(project).unwrap();
        scene
            .sample(project, 17., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        scene
    };
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(512, 512).unwrap();
    let original_scene = sample(&project);
    let original = renderer.capture(&original_scene, &target).unwrap().0;
    assert!(original.chunks_exact(4).any(|p| p[0] > 0 && p[3] > 0));
    let frame = &renderer.effect_gpu.builder.frame;
    assert!(frame.scratch_allocations.len() > 8);
    assert_eq!(
        renderer.effect_gpu.state.pool.len(),
        frame.scratch_allocations.len()
    );
    assert!(frame.scratch_bindings.iter().any(|p| p.output >= 8));
    assert_eq!(
        renderer.texture_memory_breakdown().effect_scratch,
        frame.scratch_bytes()
    );
    for (descriptor, native) in frame
        .scratch_allocations
        .iter()
        .zip(&renderer.effect_gpu.state.pool)
    {
        let native = native.as_ref().unwrap();
        assert_eq!(
            (
                native.texture.width(),
                native.texture.height(),
                native.texture.format()
            ),
            (
                descriptor.width,
                descriptor.height,
                descriptor.class.format.native()
            )
        );
    }
    project.layers.rotate_left(1);
    let reordered = sample(&project);
    let warm = renderer.capture(&reordered, &target).unwrap().0;
    let mut fresh = pollster::block_on(Renderer::headless()).unwrap();
    let fresh_target = fresh.capture_target(512, 512).unwrap();
    assert_eq!(warm, fresh.capture(&reordered, &fresh_target).unwrap().0);
    assert_eq!(
        original,
        renderer.capture(&original_scene, &target).unwrap().0
    );
    renderer.check_health().unwrap();
    fresh.check_health().unwrap();
}
