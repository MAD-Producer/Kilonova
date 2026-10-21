use motion_core::{Asset, Content, Layer, Project};
use motion_render::{
    AllocationBudget, ChromaLayout, Renderer, ResourceBudgetScope, ResourceRejection, Scene,
    Yuv420Frame,
};

fn renderer(budget: &AllocationBudget) -> Renderer {
    pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap()
}
fn submit_and_wait(renderer: &Renderer) {
    let encoder = renderer.device.create_command_encoder(&Default::default());
    // This public submission path must retire uploads even with diagnostics off
    // and without an admitted encoder retaining all current destinations.
    renderer.submit_tracked(encoder.finish(), |_| {
        panic!("diagnostics must remain disabled")
    });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
}
fn image_scene() -> Scene {
    let mut project = Project::new(4, 4, 30, 1).unwrap();
    project.background = [0.; 4];
    project.assets.push(Asset {
        id: 7,
        path: "assets/test.png".into(),
        width: 2,
        height: 2,
    });
    let mut layer = Layer::solid(1, "Image", [4.; 2], [2., 2., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 7 };
    project.layers.push(layer);
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
fn evicted_unsubmitted_destinations_keep_capacity_and_normal_submissions_release_it() {
    let budget = AllocationBudget::new(4 + 16 + 16).unwrap();
    let mut renderer = renderer(&budget);
    assert_eq!(budget.info().unwrap().admitted_bytes, 4);
    for _ in 0..6 {
        renderer
            .upload_image(7, 2, 2, &[0, 255, 0, 255].repeat(4))
            .unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, 20);
        assert_eq!(budget.info().unwrap().live_allocations, 2);
    }
    renderer.clear_assets();
    assert_eq!(renderer.image_dimensions(7), None);
    assert_eq!(budget.info().unwrap().admitted_bytes, 20);
    renderer
        .upload_image(8, 2, 2, &[0, 0, 255, 255].repeat(4))
        .unwrap();
    renderer.clear_assets();
    assert_eq!(budget.info().unwrap().admitted_bytes, 36);
    let before = budget.info().unwrap();
    let error = renderer.upload_image(9, 1, 1, &[255; 4]).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: 40,
            budget_bytes: 36,
        })
    );
    for context in [
        "key=Static(9)",
        "phase=allocate",
        "required_bytes=40",
        "budget_bytes=36",
    ] {
        assert!(error.to_string().contains(context), "{error}");
    }
    assert_eq!(budget.info().unwrap(), before);
    assert_eq!(renderer.image_count(), 1);
    assert_eq!(renderer.image_dimensions(9), None);
    // Polling is not a submission and must not fabricate completion of writes.
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 36);
    submit_and_wait(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, 4);
    renderer.upload_image(9, 1, 1, &[255; 4]).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 8);
    renderer.clear_assets();
    submit_and_wait(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, 4);
}

#[test]
fn refused_image_growth_keeps_original_pixels_and_same_size_updates_need_no_new_destination() {
    let budget = AllocationBudget::new(4 + 16 + 64 + 4 * 256).unwrap();
    let mut renderer = renderer(&budget);
    renderer
        .upload_image(7, 2, 2, &[0, 255, 0, 255].repeat(4))
        .unwrap();
    let target = renderer.capture_target(4, 4).unwrap();
    let scene = image_scene();
    let green = renderer.capture(&scene, &target).unwrap().0;
    assert!(green.chunks_exact(4).all(|pixel| pixel == [0, 255, 0, 255]));
    let before = budget.info().unwrap();
    assert_eq!(before.admitted_bytes, 1108);
    let error = renderer.upload_image(7, 4, 4, &[255; 64]).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: 1172,
            budget_bytes: 1108,
        })
    );
    assert_eq!(renderer.image_dimensions(7), Some((2, 2)));
    assert_eq!(budget.info().unwrap(), before);
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, green);
    renderer
        .upload_image(7, 2, 2, &[0, 0, 255, 255].repeat(4))
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 1108);
    let blue = renderer.capture(&scene, &target).unwrap().0;
    assert!(blue.chunks_exact(4).all(|pixel| pixel == [0, 0, 255, 255]));
    drop(target);
    submit_and_wait(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, 20);
    renderer.upload_image(7, 4, 4, &[255; 64]).unwrap();
    assert_eq!(renderer.image_dimensions(7), Some((4, 4)));
    assert_eq!(budget.info().unwrap().admitted_bytes, 68);
}

#[test]
fn video_output_admission_preserves_previous_rgba_and_conversion_retirement() {
    let budget = AllocationBudget::new(20).unwrap();
    let mut renderer = renderer(&budget);
    renderer
        .upload_video_frame(1, 10, 0, 2, 2, &[255; 16])
        .unwrap();
    let error = renderer
        .upload_video_frame(2, 11, 0, 1, 1, &[255; 4])
        .unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: 24,
            budget_bytes: 20,
        })
    );
    assert!(error
        .to_string()
        .contains("object=2 source=11 pts_us=0 phase=rgba_upload"));
    let frame = Yuv420Frame {
        width: 2,
        height: 2,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: ChromaLayout::Uv,
        y: vec![128; 4],
        uv: vec![128; 2],
    };
    let error = renderer.upload_video_yuv(2, 11, 1, &frame).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: 36,
            budget_bytes: 20,
        })
    );
    assert!(error
        .to_string()
        .contains("object=2 source=11 pts_us=1 phase=yuv_allocate"));
    let uploads = renderer.video_uploads;
    renderer
        .upload_video_frame(1, 10, 0, 2, 2, &[255; 16])
        .unwrap();
    assert_eq!(
        renderer.video_uploads, uploads,
        "refusal changed the prior completed stamp"
    );
    assert_eq!(renderer.image_count(), 2);
    assert_eq!(renderer.texture_memory_breakdown().video_planes, 0);
    assert_eq!(budget.info().unwrap().admitted_bytes, 20);
    // Include the newly admitted Y/UV planes (6) and conversion uniform (48).
    budget.set_limit(36 + 6 + 48).unwrap();
    let unfinished = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.upload_video_yuv(2, 11, 1, &frame).unwrap();
    assert_eq!(renderer.image_count(), 3);
    assert_eq!(renderer.video_conversion_submissions, 1);
    let project = Project::new(4, 4, 30, 1).unwrap();
    let mut empty = Scene::new(&project);
    empty
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.retain_video_instances(&empty);
    assert_eq!(renderer.texture_memory_breakdown().video_rgba, 0);
    assert_eq!(budget.info().unwrap().admitted_bytes, 36 + 6 + 48);
    assert!(renderer.render_target(1, 1).is_err());
    drop(unfinished);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 4);
    renderer.check_health().unwrap();
}

#[test]
fn standalone_video_scope_refusal_preserves_the_previous_image_and_stamp() {
    let budget = AllocationBudget::new(1024).unwrap();
    let mut renderer = renderer(&budget);
    renderer
        .upload_video_frame(1, 10, 0, 2, 2, &[255; 16])
        .unwrap();
    submit_and_wait(&renderer);
    let scopes: Vec<_> = (0..64)
        .map(|_| {
            renderer
                .allocation_command_encoder(&Default::default())
                .unwrap()
        })
        .collect();
    let before = budget.info().unwrap();
    let uploads = renderer.video_uploads;
    let frame = Yuv420Frame {
        width: 2,
        height: 2,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: ChromaLayout::Uv,
        y: vec![128; 4],
        uv: vec![128; 2],
    };
    let error = renderer.upload_video_yuv(1, 10, 1, &frame).unwrap_err();
    let cause = error.to_string();
    assert!(cause.contains("too many pending render allocation encoder scopes"));
    assert!(cause.contains("object=1 source=10 pts_us=1 phase=conversion_encode"));
    assert_eq!(budget.info().unwrap(), before);
    assert_eq!(renderer.video_uploads, uploads);
    assert_eq!(renderer.video_gpu_conversions, 0);
    assert_eq!(renderer.image_count(), 2);
    assert_eq!(renderer.texture_memory_breakdown().video_planes, 0);
    // The old stamp still matches, so this is a real no-op, not a re-upload of
    // the prior pixels into an accidentally replaced destination.
    renderer
        .upload_video_frame(1, 10, 0, 2, 2, &[255; 16])
        .unwrap();
    assert_eq!(renderer.video_uploads, uploads);
    drop(scopes);
    renderer.upload_video_yuv(1, 10, 1, &frame).unwrap();
    assert_eq!(renderer.video_gpu_conversions, 1);
    assert_eq!(renderer.video_conversion_submissions, 1);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 20 + 6 + 48);
    renderer.check_health().unwrap();
}

#[test]
fn renderer_teardown_retires_unsubmitted_destinations_with_external_queue_and_device_handles() {
    let budget = AllocationBudget::new(20).unwrap();
    for _ in 0..3 {
        let mut renderer = renderer(&budget);
        renderer.upload_image(7, 2, 2, &[255; 16]).unwrap();
        renderer.clear_assets();
        assert_eq!(budget.info().unwrap().admitted_bytes, 20);
        let device = renderer.device.clone();
        let queue = renderer.queue.clone();
        drop(renderer);
        // Pending writes were submitted by teardown. The caller's raw queue
        // remains valid, so simply dropping its application owner cannot serve
        // as proof that the enrolled destinations retired.
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, 0);
        assert_eq!(budget.info().unwrap().live_allocations, 0);
        drop(queue);
    }
}

#[test]
fn startup_white_source_is_admitted_and_refusal_leaves_no_reservation() {
    let budget = AllocationBudget::new(3).unwrap();
    let error = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone()))
        .err()
        .unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: 4,
            budget_bytes: 3,
        })
    );
    assert!(error
        .to_string()
        .contains("key=Static(0) size=1x1 phase=allocate"));
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
    assert_eq!(budget.info().unwrap().live_allocations, 0);
}
