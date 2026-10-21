use super::*;

fn flush(renderer: &Renderer) {
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
}

fn video_scene() -> Scene {
    let mut project = Project::new(2, 2, 30, 60).unwrap();
    project.background = [0.; 4];
    project.video_assets.push(motion_core::VideoAsset {
        id: 7,
        path: "assets/authored.mp4".into(),
        bytes: 1,
        mime: "video/avc".into(),
        track: 0,
        width: 2,
        height: 2,
        rotation: 0,
        display_width: 2,
        display_height: 2,
        video_start_us: 0,
        video_end_us: 2_000_000,
        duration_us: 2_000_000,
        frame_count: 2,
        variable_frame_rate: true,
        nominal_frame_rate: 24.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    let mut layer = Layer::solid(1, "Authored video", [2.; 2], [1., 1., 0.], [1.; 4]);
    layer.content = Content::Video {
        video: motion_core::VideoClip::new(7),
    };
    layer.sampling.frame_blend = motion_core::render_sampling::FrameBlend::Linear;
    project.layers.push(layer);
    project.validate().unwrap();
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

fn pixels(renderer: &mut Renderer) -> Vec<u8> {
    let target = renderer.capture_target(2, 2).unwrap();
    renderer.capture(&video_scene(), &target).unwrap().0
}

fn recover_and_blend(renderer: &mut Renderer) {
    renderer
        .upload_video_frame(1, 7, 100, 2, 2, &[0, 0, 0, 255].repeat(4))
        .unwrap();
    renderer.cache_video_blend_source(1, 7, 100, 0).unwrap();
    renderer
        .upload_video_frame(1, 7, 200, 2, 2, &[255; 16])
        .unwrap();
    renderer.cache_video_blend_source(1, 7, 200, 1).unwrap();
    renderer.blend_video_sources(1, 7, 100, 200, 0.5).unwrap();
    let actual = pixels(renderer);
    assert!(
        actual
            .chunks_exact(4)
            .all(|p| (187..=189).contains(&p[0]) && p[0] == p[1] && p[1] == p[2] && p[3] == 255),
        "{actual:?}"
    );
    renderer.check_health().unwrap();
}

#[test]
fn late_blend_uniform_refusal_preserves_the_decoded_frame_and_recovers_real_pixels() {
    // Startup image + decoded RGBA + newly allocated cache image + uniform.
    let budget = crate::AllocationBudget::new(4 + 16 + 16 + 16 - 1).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    renderer
        .upload_video_frame(1, 7, 100, 2, 2, &[0, 0, 0, 255].repeat(4))
        .unwrap();
    let error = renderer.cache_video_blend_source(1, 7, 100, 0).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: 52,
            budget_bytes: 51,
        })
    );
    for context in [
        "object=1 source=7 pts_us=100 slot=0 size=2x2 phase=blend_cache",
        "buffer size=16 phase=allocate label=video frame blend weight",
    ] {
        assert!(error.to_string().contains(context), "{error}");
    }
    assert_eq!(budget.info().unwrap().admitted_bytes, 36);
    assert!(renderer.video_blend.is_none());
    assert_eq!(renderer.video_blend_submissions, 0);
    assert!(!renderer.video_blend_source_ready(1, 7, 100, 0));
    assert_eq!(
        renderer.images[&TextureKey::Video(1)].video_stamp,
        Some((7, 100))
    );
    budget.set_limit(1 << 20).unwrap();
    assert!(pixels(&mut renderer)
        .chunks_exact(4)
        .all(|p| p == [0, 0, 0, 255]));
    renderer.cache_video_blend_source(1, 7, 100, 0).unwrap();
    flush(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, 52);
    assert!(renderer.video_blend_source_ready(1, 7, 100, 0));
    recover_and_blend(&mut renderer);
}

#[test]
fn refused_blend_encoder_does_not_queue_a_write_or_acknowledge_the_cache() {
    let budget = crate::AllocationBudget::new(1 << 20).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    renderer
        .upload_video_frame(1, 7, 100, 2, 2, &[255, 0, 0, 255].repeat(4))
        .unwrap();
    renderer.cache_video_blend_source(1, 7, 100, 0).unwrap();
    flush(&renderer);
    let scopes: Vec<_> = (0..64)
        .map(|_| {
            renderer
                .allocation_command_encoder(&Default::default())
                .unwrap()
        })
        .collect();
    let error = renderer.cache_video_blend_source(1, 7, 100, 1).unwrap_err();
    assert!(
        error.resource_rejection().is_none(),
        "scope exhaustion is not byte-budget exhaustion"
    );
    assert!(
        error
            .to_string()
            .contains("too many pending render allocation encoder scopes"),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("object=1 source=7 pts_us=100 slot=1 size=2x2 phase=blend_cache"),
        "{error}"
    );
    assert_eq!(renderer.video_blend_submissions, 1);
    assert!(renderer.video_blend_source_ready(1, 7, 100, 0));
    assert!(!renderer.video_blend_source_ready(1, 7, 100, 1));
    renderer.reclaim_preview_working_set().unwrap();
    drop(scopes);
    // No submit/poll here: a queued failed uniform write would retain 16 bytes.
    assert_eq!(budget.info().unwrap().admitted_bytes, 4);
    recover_and_blend(&mut renderer);
}

#[test]
fn abandoned_encoder_scopes_retain_new_blend_uniform_capacity() {
    for finish in [false, true] {
        let budget = crate::AllocationBudget::new(1 << 20).unwrap();
        let mut renderer =
            pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
        let encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        renderer
            .upload_video_frame(1, 7, 100, 2, 2, &[255; 16])
            .unwrap();
        renderer.cache_video_blend_source(1, 7, 100, 0).unwrap();
        flush(&renderer);
        assert_eq!(budget.info().unwrap().admitted_bytes, 52);
        assert_eq!(budget.info().unwrap().live_allocations, 4);
        renderer.reclaim_preview_working_set().unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, 52);
        if finish {
            drop(encoder.finish());
        } else {
            drop(encoder);
        }
        assert_eq!(budget.info().unwrap().admitted_bytes, 4);
        let device = renderer.device.clone();
        let queue = renderer.queue.clone();
        drop(renderer);
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, 0);
        drop(queue);
    }
}
