use motion_core::{
    compositing::{MatteMode, TrackMatte},
    Content, Layer, LayerTimeline, Project, VideoAsset, VideoClip,
};
use motion_render::{
    preview_input::PreviewInputTexture, ChromaLayout, Renderer, Scene, Yuv420Frame,
};

fn project() -> Project {
    let mut p = Project::new(32, 32, 30, 120).unwrap();
    p.video_assets.push(VideoAsset {
        id: 1,
        path: "assets/video.mp4".into(),
        bytes: 1,
        mime: "video/avc".into(),
        track: 0,
        width: 16,
        height: 16,
        rotation: 0,
        display_width: 16,
        display_height: 16,
        video_start_us: 0,
        video_end_us: 2_000_000,
        duration_us: 2_000_000,
        frame_count: 60,
        variable_frame_rate: false,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    p
}
fn video(id: u64) -> Layer {
    let mut l = Layer::solid(id, "video", [32.; 2], [16., 16., 0.], [1.; 4]);
    l.content = Content::Video {
        video: VideoClip::new(1),
    };
    l.timeline = Some(LayerTimeline::full(60));
    l
}
fn pixels() -> Yuv420Frame {
    Yuv420Frame {
        width: 16,
        height: 16,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: ChromaLayout::Uv,
        y: vec![128; 256],
        uv: vec![128; 128],
    }
}

#[test]
fn sixty_clip_traversals_and_reverse_seeks_keep_one_live_video_and_release_the_gap() {
    let mut p = project();
    for frame in 0..60 {
        let mut l = video(frame as u64 + 1);
        l.timeline = Some(LayerTimeline {
            in_frame: frame,
            out_frame: frame + 1,
            offset_frame: frame,
        });
        p.layers.push(l);
    }
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let baseline = renderer.texture_bytes();
    let mut scene = Scene::new(&p);
    let f = pixels();
    let target = renderer.capture_target(32, 32).unwrap();
    let mut peak = 0;
    for frame in (0..60).chain((0..60).rev()) {
        scene
            .sample(
                &p,
                f64::from(frame) + 0.5,
                None,
                &motion_core::ExpressionEvaluator,
            )
            .unwrap();
        renderer.retain_video_instances(&scene);
        let layers = scene.video_layers();
        assert_eq!(layers.len(), 1);
        renderer.upload_video_yuv(layers[0].id, 1, 0, &f).unwrap();
        assert_eq!(renderer.image_count(), 2);
        assert_eq!(renderer.texture_bytes(), baseline + 16 * 16 * 4 + 256 + 128);
        peak = peak.max(renderer.texture_bytes());
        let uploads = renderer.video_uploads;
        renderer.upload_video_yuv(layers[0].id, 1, 0, &f).unwrap();
        assert_eq!(uploads, renderer.video_uploads);
        if frame % 10 == 0 {
            let out = renderer.capture(&scene, &target).unwrap().0;
            // Video content uses the asset's intrinsic 16×16 size. Inspect its
            // center rather than asserting it covers the whole 32×32 canvas.
            let center = &out[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4];
            let expected = f.to_rgba().unwrap();
            assert!(
                center
                    .iter()
                    .zip(&expected[..4])
                    .all(|(a, b)| a.abs_diff(*b) <= 1),
                "{center:?}"
            );
        }
    }
    scene
        .sample(&p, 90., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.retain_video_instances(&scene);
    assert_eq!(renderer.texture_bytes(), baseline);
    assert_eq!(renderer.image_count(), 1);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
    renderer.check_health().unwrap();
    eprintln!(
        "video logical texture baseline={baseline}, peak={peak}, gap={}",
        renderer.texture_bytes()
    );
}

#[test]
fn hidden_matte_source_is_live_but_unreferenced_hidden_video_is_reclaimed() {
    let mut p = project();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    let mut source = video(1);
    source.visible = false;
    let mut target = Layer::solid(2, "target", [32.; 2], [16., 16., 0.], [1.; 4]);
    target.track_matte = Some(TrackMatte {
        source: 1,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    let mut unused = video(3);
    unused.visible = false;
    p.layers = vec![source, target, unused];
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let baseline = renderer.texture_bytes();
    let f = pixels();
    for object in [1, 3] {
        renderer.upload_video_yuv(object, 1, 0, &f).unwrap();
    }
    let mut scene = Scene::new(&p);
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert_eq!(
        scene
            .video_layers()
            .iter()
            .map(|l| l.id)
            .collect::<Vec<_>>(),
        vec![1]
    );
    renderer.retain_video_instances(&scene);
    assert_eq!(renderer.texture_bytes(), baseline + 1408);
    assert_eq!(renderer.image_count(), 2);
    p.layers[1].track_matte = None;
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.retain_video_instances(&scene);
    assert_eq!(renderer.texture_bytes(), baseline);
    assert_eq!(renderer.image_count(), 1);
}

#[test]
fn standalone_input_switches_do_not_accumulate_planes_or_rgba_sources() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut input = PreviewInputTexture::new(renderer.device.clone(), renderer.queue.clone(), 4096);
    let f = pixels();
    let rgba = [128, 128, 128, 255].repeat(256);
    for _ in 0..60 {
        input.upload_yuv(&f, 16).unwrap();
        assert_eq!(input.memory_bytes(), 1024 + 384);
        input.upload_rgba(16, 16, &rgba, false, 16).unwrap();
        assert_eq!(input.memory_bytes(), 1024 + 1024);
    }
    input.transparent().unwrap();
    assert_eq!(input.memory_bytes(), 4);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
    renderer.check_health().unwrap();
}

#[test]
fn rgba_handoff_releases_only_its_planes_and_preserves_both_instances_pixels() {
    let mut p = project();
    p.layers = vec![video(1), video(2)];
    for layer in &mut p.layers {
        layer.size = [16.; 2];
    }
    p.layers[0].transform.position.value = [8., 16., 0.];
    p.layers[1].transform.position.value = [24., 16., 0.];
    let mut scene = Scene::new(&p);
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.retain_video_instances(&scene);
    let f = pixels();
    for object in [1, 2] {
        renderer.upload_video_yuv(object, 1, 0, &f).unwrap();
    }
    let blue = [0, 0, 255, 255].repeat(256);
    let target = renderer.capture_target(32, 32).unwrap();
    for pts in 1..=8 {
        renderer.upload_video_yuv(1, 1, pts * 2, &f).unwrap();
        assert_eq!(renderer.texture_memory_breakdown().video_planes, 768);
        renderer
            .upload_video_frame(1, 1, pts * 2 + 1, 16, 16, &blue)
            .unwrap();
        let memory = renderer.texture_memory_breakdown();
        assert_eq!(
            memory.video_planes, 384,
            "RGBA instance retained obsolete YUV planes"
        );
        assert_eq!(memory.video_rgba, 2048);
        let ledger = renderer.texture_resource_ledger();
        let planes: Vec<_> = ledger
            .resources()
            .filter(|r| {
                r.references.iter().any(|reference| {
                    reference.category == motion_render::TextureCategory::VideoPlanes
                })
            })
            .collect();
        assert_eq!(planes.len(), 2);
        assert!(planes.iter().all(|r| r
            .references
            .iter()
            .all(|reference| reference.owner.starts_with("video/2/"))));
        let out = renderer.capture(&scene, &target).unwrap().0;
        let at = |x: usize| &out[(16 * 32 + x) * 4..(16 * 32 + x) * 4 + 4];
        assert_eq!(at(8), [0, 0, 255, 255]);
        assert!(at(24)
            .iter()
            .zip(&f.to_rgba().unwrap()[..4])
            .all(|(a, b)| a.abs_diff(*b) <= 1));
    }
    renderer.clear_assets();
    assert_eq!(renderer.texture_memory_breakdown().video_planes, 0);
    assert_eq!(renderer.texture_memory_breakdown().video_rgba, 0);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
    renderer.check_health().unwrap();
}

#[test]
fn same_stamp_rgba_handoff_reuses_pixels_but_releases_completed_planes() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let f = pixels();
    renderer.upload_video_yuv(1, 1, 0, &f).unwrap();
    let uploads = renderer.video_uploads;
    renderer
        .upload_video_frame(1, 1, 0, 16, 16, &f.to_rgba().unwrap())
        .unwrap();
    assert_eq!(
        renderer.video_uploads, uploads,
        "same frame should reuse the converted output"
    );
    assert_eq!(renderer.texture_memory_breakdown().video_planes, 0);
    assert_eq!(renderer.texture_memory_breakdown().video_rgba, 1024);
}

#[test]
fn rgba_growth_uses_the_budget_freed_by_its_retired_planes() {
    use motion_render::resource_policy::MemoryPolicy;
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer
        .set_memory_policy(MemoryPolicy {
            source_texture_budget_bytes: 4 + 32 * 16 * 4,
            ..Default::default()
        })
        .unwrap();
    renderer.upload_video_yuv(1, 1, 0, &pixels()).unwrap();
    renderer
        .upload_video_frame(1, 1, 1, 32, 16, &[0, 0, 255, 255].repeat(512))
        .unwrap();
    assert_eq!(renderer.texture_bytes(), 4 + 32 * 16 * 4);
    assert_eq!(renderer.texture_memory_breakdown().video_planes, 0);
}

#[test]
fn rgba_replacement_cannot_borrow_another_live_instances_plane_budget() {
    use motion_render::{resource_policy::MemoryPolicy, ResourceBudgetScope, ResourceRejection};
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    for object in [1, 2] {
        renderer.upload_video_yuv(object, 1, 0, &pixels()).unwrap();
    }
    let before = renderer.texture_memory_breakdown();
    // Fallback + new RGBA + peer RGBA + peer YUV; only object 1 retires planes.
    let required = 4 + 2048 + 1024 + 384;
    renderer
        .set_memory_policy(MemoryPolicy {
            source_texture_budget_bytes: required - 1,
            ..Default::default()
        })
        .unwrap();
    let rgba = [0, 0, 255, 255].repeat(512);
    let error = renderer
        .upload_video_frame(1, 1, 1, 32, 16, &rgba)
        .unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::SourceTextures,
            required_bytes: required,
            budget_bytes: required - 1,
        })
    );
    assert_eq!(renderer.texture_memory_breakdown(), before);
    renderer
        .set_memory_policy(MemoryPolicy {
            source_texture_budget_bytes: required,
            ..Default::default()
        })
        .unwrap();
    renderer.upload_video_frame(1, 1, 1, 32, 16, &rgba).unwrap();
    assert_eq!(renderer.texture_bytes(), required);
    assert_eq!(renderer.texture_memory_breakdown().video_planes, 384);
    let uploads = renderer.video_uploads;
    renderer.upload_video_yuv(2, 1, 0, &pixels()).unwrap();
    assert_eq!(
        renderer.video_uploads, uploads,
        "peer conversion was replaced"
    );
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
    renderer.check_health().unwrap();
}

#[test]
fn failed_rgba_handoff_preserves_cancelled_conversion_and_its_original_budget_error() {
    use motion_render::{resource_policy::MemoryPolicy, ResourceBudgetScope, ResourceRejection};
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let f = pixels();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let upload = renderer
        .encode_video_yuv(1, 1, 0, &f, &mut encoder)
        .unwrap()
        .unwrap();
    assert!(renderer
        .upload_video_frame(1, 1, 1, 16, 16, &[255; 1024])
        .is_err());
    drop(encoder);
    drop(upload);
    let before = renderer.texture_memory_breakdown();
    renderer
        .set_memory_policy(MemoryPolicy {
            source_texture_budget_bytes: 4,
            ..Default::default()
        })
        .unwrap();
    let error = renderer
        .upload_video_frame(1, 1, 1, 32, 16, &[255; 2048])
        .unwrap_err();
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::Budget {
            scope: ResourceBudgetScope::SourceTextures,
            budget_bytes: 4,
            ..
        })
    ));
    assert_eq!(renderer.texture_memory_breakdown(), before);
    renderer.set_memory_policy(MemoryPolicy::default()).unwrap();
    let mut retry = renderer.device.create_command_encoder(&Default::default());
    let upload = renderer.encode_video_yuv(1, 1, 0, &f, &mut retry).unwrap();
    assert!(
        upload.is_some(),
        "failed RGBA replacement turned a cancelled upload into a ready cache hit"
    );
    drop(retry);
    drop(upload);
    renderer
        .upload_video_frame(1, 1, 1, 16, 16, &[255; 1024])
        .unwrap();
    assert_eq!(renderer.texture_memory_breakdown().video_planes, 0);
}
