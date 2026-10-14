use motion_render::{Renderer, Scene, TextureResourceLedger, TextureState};

#[test]
fn disabled_tracking_has_no_snapshot_work_and_callbacks_complete_real_submissions() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.render_target(16, 16).unwrap();
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.submit_tracked(encoder.finish(), |_| {
        panic!("disabled tracking enumerated targets")
    });
    assert_eq!(
        renderer
            .submission_memory_info(&Default::default())
            .submitted,
        0
    );
    renderer.set_submission_tracking(true);
    let mut current = renderer.texture_resource_ledger();
    target.collect_texture_resources(&mut current, "test/target", TextureState::Retained);
    for _ in 0..2 {
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.submit_tracked(encoder.finish(), |ledger| {
            target.collect_texture_resources(ledger, "test/target", TextureState::Retained);
        });
    }
    renderer.device.poll(wgpu::Maintain::Wait);
    let info = renderer.submission_memory_info(&current);
    assert_eq!(info.submitted, 2);
    assert_eq!(info.completed, 2);
    assert_eq!(info.pending_submissions, 0);
    assert_eq!(info.peak_known_pending_bytes, current.total_known_bytes());
    assert_eq!(info.known_retired_bytes, 0);
    renderer.check_health().unwrap();
}

#[test]
fn rejected_encoding_does_not_create_a_submission_and_capture_keeps_existing_pixels() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_submission_tracking(true);
    let mut project = motion_core::Project::new(16, 16, 30, 60).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "sample",
        [16.; 2],
        [8., 8., 0.],
        [0.3, 0.6, 0.9, 0.5],
    ));
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let target = renderer.capture_target(16, 16).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    assert!(renderer
        .encode(&scene, &target.view, 0, 0, &mut encoder, None)
        .is_err());
    drop(encoder);
    assert_eq!(
        renderer
            .submission_memory_info(&TextureResourceLedger::default())
            .submitted,
        0
    );
    let tracked = renderer.capture(&scene, &target).unwrap();
    assert!(tracked.0.chunks_exact(4).any(|pixel| pixel[0] > 0));
    let info = renderer.submission_memory_info(&target.texture_resource_ledger());
    assert_eq!(info.submitted, 2);
    assert_eq!(info.completed, 2);
    assert_eq!(info.pending_submissions, 0);
    renderer.set_submission_tracking(false);
    assert_eq!(tracked.0, renderer.capture(&scene, &target).unwrap().0);
    assert_eq!(
        renderer
            .submission_memory_info(&target.texture_resource_ledger())
            .submitted,
        2
    );
}
