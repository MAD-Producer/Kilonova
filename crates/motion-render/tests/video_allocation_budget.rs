const SYSTEM_BYTES: u64 = motion_render::FIXED_RENDERER_ALLOCATION_BYTES;

use motion_core::{Layer, Project};
use motion_render::{
    AllocationBudget, ChromaLayout, Renderer, ResourceBudgetScope, ResourceRejection, Scene,
    Yuv420Frame,
};

fn frame() -> Yuv420Frame {
    Yuv420Frame {
        width: 2,
        height: 2,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: ChromaLayout::Uv,
        y: vec![128; 4],
        uv: vec![128; 2],
    }
}
fn renderer(budget: &AllocationBudget) -> Renderer {
    pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap()
}
fn scene() -> Scene {
    let mut project = Project::new(2, 2, 30, 1).unwrap();
    project.background = [0.; 4];
    project
        .layers
        .push(Layer::solid(1, "Video", [2.; 2], [1., 1., 0.], [1.; 4]));
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene.layers[0].video = Some(motion_core::VideoSample {
        asset: 10,
        source_time_us: 0,
    });
    scene
}
fn flush(renderer: &Renderer) {
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.submit_tracked(encoder.finish(), |_| panic!("diagnostics must remain off"));
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
}

#[test]
fn late_video_uniform_refusal_rolls_back_planes_and_output_without_changing_old_pixels() {
    let budget = AllocationBudget::new(SYSTEM_BYTES + 4092).unwrap();
    let mut renderer = renderer(&budget);
    renderer
        .upload_video_frame(1, 10, 0, 2, 2, &[0, 255, 0, 255].repeat(4))
        .unwrap();
    let target = renderer.capture_target(2, 2).unwrap();
    let old = renderer.capture(&scene(), &target).unwrap().0;
    assert_eq!(old, [0, 255, 0, 255].repeat(4));
    let before = budget.info().unwrap();
    assert_eq!(before.admitted_bytes, SYSTEM_BYTES + 16 + 16 + 512);
    let textures = renderer.texture_memory_breakdown();
    let uploads = renderer.video_uploads;
    // New RGBA16 + Y4 + UV2 + uniform48 overlap the old image until completion.
    budget.set_limit(before.admitted_bytes + 70 - 1).unwrap();
    let error = renderer.upload_video_yuv(1, 10, 1, &frame()).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + 70,
            budget_bytes: before.admitted_bytes + 69,
        })
    );
    for text in [
        "object=1 source=10 pts_us=1 phase=yuv_prepare",
        "buffer size=48 phase=allocate",
    ] {
        assert!(error.to_string().contains(text), "{error}");
    }
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(
        budget.info().unwrap().live_allocations,
        before.live_allocations
    );
    assert_eq!(renderer.texture_memory_breakdown(), textures);
    assert_eq!(renderer.video_uploads, uploads);
    renderer
        .upload_video_frame(1, 10, 0, 2, 2, &[255; 16])
        .unwrap();
    assert_eq!(
        renderer.video_uploads, uploads,
        "old stamp was replaced on refusal"
    );
    assert_eq!(renderer.capture(&scene(), &target).unwrap().0, old);
    budget.set_limit(before.admitted_bytes + 70).unwrap();
    renderer.upload_video_yuv(1, 10, 1, &frame()).unwrap();
    let actual = renderer.capture(&scene(), &target).unwrap().0;
    assert!(actual
        .iter()
        .zip(frame().to_rgba().unwrap())
        .all(|(a, b)| a.abs_diff(b) <= 1));
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        SYSTEM_BYTES + 16 + 6 + 48 + 16 + 512
    );
    drop(target);
    renderer.clear_assets();
    flush(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
}

#[test]
fn raw_encoded_video_tokens_keep_destinations_through_cache_clear_and_real_submission() {
    let budget = AllocationBudget::new(SYSTEM_BYTES + 16 + 6 + 48).unwrap();
    let mut renderer = renderer(&budget);
    for submitted in [false, true] {
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        let upload = renderer
            .encode_video_yuv(1, 10, 0, &frame(), &mut encoder)
            .unwrap()
            .unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 70);
        assert_eq!(budget.info().unwrap().live_allocations, 10);
        renderer.clear_assets();
        // No admitted encoder can hide missing retention by the upload token.
        assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 70);
        if submitted {
            renderer.queue.submit(Some(encoder.finish()));
            upload.mark_submitted();
        } else {
            drop(encoder);
            drop(upload);
            // Only the queued plane/uniform writes plus the white source remain.
            assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 6 + 48);
        }
        flush(&renderer);
        assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
        assert_eq!(budget.info().unwrap().live_allocations, 6);
    }
}
