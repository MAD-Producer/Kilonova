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
    renderer.device.poll(wgpu::Maintain::Wait);
    renderer.check_health().unwrap();
    eprintln!(
        "video logical texture baseline={baseline}, peak={peak}, gap={}",
        renderer.texture_bytes()
    );
}

#[test]
fn hidden_matte_source_is_live_but_unreferenced_hidden_video_is_reclaimed() {
    let mut p = project();
    p.version = 10;
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
    renderer.device.poll(wgpu::Maintain::Wait);
    renderer.check_health().unwrap();
}
