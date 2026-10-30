use motion_core::{Content, EffectInstance, Layer, LayerTimeline, Project, VideoAsset, VideoClip};
use motion_render::Scene;
use motion_render::Renderer;
fn fixture() -> Project {
    let mut p = Project::new(64, 32, 60, 120).unwrap();
    p.background = [0., 0., 0., 1.];
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
        frame_count: 48,
        variable_frame_rate: false,
        nominal_frame_rate: 24.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    for (id, x) in [(1, 16.), (2, 48.)] {
        let mut l = Layer::solid(id, "video", [32., 32.], [x, 16., 0.], [1.; 4]);
        l.content = Content::Video {
            video: VideoClip::new(1),
        };
        l.timeline = Some(LayerTimeline::full(120));
        p.layers.push(l);
    }
    p.validate().unwrap();
    p
}

#[test]
fn held_first_video_frame_preserves_source_clock_and_legacy_blank_head() {
    let mut p = fixture();
    p.video_assets[0].video_start_us = 250_000;
    let Content::Video { video } = &mut p.layers[0].content else { unreachable!() };
    video.hold_first_frame = true;
    let mut scene = Scene::new(&p).unwrap();
    for (frame, expected) in [(0., 250_000), (15., 250_000), (30., 500_000)] {
        scene.sample(&p, frame, None, &motion_core::ExpressionEvaluator).unwrap();
        let layer = scene.layers.iter().find(|layer| layer.id == 1).unwrap();
        assert_eq!(layer.video.as_ref().unwrap().source_time_us, expected);
        let legacy = scene.layers.iter().find(|layer| layer.id == 2);
        assert_eq!(legacy.is_some(), frame >= 15.);
    }
    let serialized = serde_json::to_value(&p).unwrap();
    assert_eq!(serialized["layers"][0]["content"]["video"]["hold_first_frame"], true);
    let restored: Project = serde_json::from_value(serialized).unwrap();
    assert_eq!(restored, p);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn held_first_frame_renders_pixels_before_the_picture_start() {
    let mut p = fixture();
    p.layers.truncate(1);
    p.video_assets[0].video_start_us = 250_000;
    let Content::Video { video } = &mut p.layers[0].content else { unreachable!() };
    video.hold_first_frame = true;
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(64, 32).unwrap();
    let mut scene = Scene::new(&p).unwrap();
    for (frame, pts, color) in [(0., 250_000, [255, 0, 0, 255]),
        (15., 250_000, [255, 0, 0, 255]), (30., 500_000, [0, 255, 0, 255])]
    {
        scene.sample(&p, frame, None, &motion_core::ExpressionEvaluator).unwrap();
        assert_eq!(scene.layers.iter().find(|layer| layer.id == 1).unwrap()
            .video.as_ref().unwrap().source_time_us, pts);
        renderer.upload_video_frame(1, 1, pts, 16, 16, &color.repeat(256)).unwrap();
        let (pixels, _) = renderer.capture(&scene, &target).unwrap();
        assert_eq!(&pixels[(16 * 64 + 16) * 4..(16 * 64 + 16) * 4 + 4], &color);
    }
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn different_source_times_use_distinct_video_textures_and_reuse_allocations() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let p = fixture();
    let mut scene = Scene::new(&p).unwrap();
    scene.sample(&p, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let red = [255, 0, 0, 255].repeat(256);
    let green = [0, 255, 0, 255].repeat(256);
    renderer.upload_video_frame(1, 1, 0, 16, 16, &red).unwrap();
    renderer
        .upload_video_frame(2, 1, 500000, 16, 16, &green)
        .unwrap();
    let target = renderer.capture_target(64, 32).unwrap();
    let (out, _) = renderer.capture(&scene, &target).unwrap();
    assert_eq!(
        &out[(16 * 64 + 16) * 4..(16 * 64 + 16) * 4 + 4],
        &[255, 0, 0, 255]
    );
    assert_eq!(
        &out[(16 * 64 + 48) * 4..(16 * 64 + 48) * 4 + 4],
        &[0, 255, 0, 255]
    );
    let bytes = renderer.texture_bytes();
    let count = renderer.image_count();
    for _ in 0..100 {
        renderer.upload_video_frame(1, 1, 0, 16, 16, &red).unwrap();
        renderer
            .upload_video_frame(2, 1, 500000, 16, 16, &green)
            .unwrap();
    }
    assert_eq!(bytes, renderer.texture_bytes());
    assert_eq!(count, renderer.image_count());
    assert_eq!(count, 3);
    renderer
        .upload_video_frame(1, 1, 1000000, 16, 16, &green)
        .unwrap();
    let (out, _) = renderer.capture(&scene, &target).unwrap();
    assert_eq!(
        &out[(16 * 64 + 16) * 4..(16 * 64 + 16) * 4 + 4],
        &[0, 255, 0, 255]
    );
    assert_eq!(bytes, renderer.texture_bytes());
}

fn effect(name: &str) -> EffectInstance {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package.manifest.effects.iter().find(|e| e.id == name).unwrap();
    EffectInstance::new(1, &package.manifest.id, &package.manifest.version,
        &package.hash, definition, [32.; 2])
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn encoded_same_source_instances_keep_distinct_pts_through_effects_and_seek() {
    use motion_render::{ChromaLayout, Yuv420Frame};
    let mut p = fixture();
    for layer in &mut p.layers { layer.effects.push(effect("brightness_contrast")); }
    p.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&p).unwrap();
    scene.sample(&p, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut standalone = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(64, 32).unwrap();
    let reference_target = standalone.capture_target(64, 32).unwrap();
    let mut memory = None;
    for (pts, left, right) in [(0, 200, 40), (750_000, 70, 190), (0, 200, 40)] {
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        let mut uploads = Vec::new();
        for (object, value, offset) in [(1, left, 0), (2, right, 450_000)] {
            let f = Yuv420Frame { width:16,height:16,rotation:0,standard:1,range:2,
                phase:[0,0],chroma_layout:ChromaLayout::Uv,y:vec![value;256],uv:vec![128;128] };
            standalone.upload_video_yuv(object, 1, pts+offset, &f).unwrap();
            uploads.push(renderer.encode_video_yuv(object,1,pts+offset,&f,&mut encoder)
                .unwrap().unwrap());
        }
        renderer.encode(&scene, &target.view, 64, 32, &mut encoder, None).unwrap();
        renderer.queue.submit(Some(encoder.finish()));
        for upload in uploads { upload.mark_submitted(); }
        let out = renderer.read_target(&target).unwrap();
        let reference = standalone.capture(&scene, &reference_target).unwrap().0;
        assert!(out.iter().zip(reference).all(|(a,b)| a.abs_diff(b) <= 1));
        let pixel = |x: usize| &out[(16*64+x)*4..(16*64+x)*4+4];
        assert_ne!(pixel(16), pixel(48));
        if let Some(bytes) = memory { assert_eq!(renderer.texture_bytes(), bytes); }
        memory = Some(renderer.texture_bytes());
    }
    assert_eq!(renderer.video_encoded_conversions, 6);
    assert_eq!(renderer.video_conversion_submissions, 0);
    assert_eq!(standalone.video_conversion_submissions, 6);
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn video_effects_keep_live_instance_pixels_and_rebind_after_visibility_changes() {
    let mut p = fixture();
    for layer in &mut p.layers {
        layer.effects.push(effect("brightness_contrast"));
    }
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(64, 32).unwrap();
    let mut scene = Scene::new(&p).unwrap();
    let red = [255, 0, 0, 255].repeat(256);
    let green = [0, 255, 0, 255].repeat(256);
    let blue = [0, 0, 255, 255].repeat(256);
    let pixel = |out: &[u8], x: usize| out[(16 * 64 + x) * 4..(16 * 64 + x) * 4 + 4].to_vec();
    renderer.upload_video_frame(1, 1, 0, 16, 16, &red).unwrap();
    renderer.upload_video_frame(2, 1, 500_000, 16, 16, &green).unwrap();
    scene.sample(&p, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let (out, _) = renderer.capture(&scene, &target).unwrap();
    assert_eq!(pixel(&out, 16), [255, 0, 0, 255]);
    assert_eq!(pixel(&out, 48), [0, 255, 0, 255]);
    renderer.upload_video_frame(1, 1, 1_000_000, 16, 16, &blue).unwrap();
    renderer.upload_video_frame(2, 1, 1_500_000, 16, 16, &red).unwrap();
    let (out, _) = renderer.capture(&scene, &target).unwrap();
    assert_eq!(pixel(&out, 16), [0, 0, 255, 255]);
    assert_eq!(pixel(&out, 48), [255, 0, 0, 255]);
    // The remaining instance now occupies the first pass's cached binding index.
    p.layers[0].visible = false;
    scene.sample(&p, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let (out, _) = renderer.capture(&scene, &target).unwrap();
    assert_eq!(pixel(&out, 16), [0, 0, 0, 255]);
    assert_eq!(pixel(&out, 48), [255, 0, 0, 255]);
    p.layers[0].visible = true;
    p.layers[0].effects[0] = effect("tint");
    p.rebuild_plugin_dependencies();
    scene.sample(&p, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let (out, _) = renderer.capture(&scene, &target).unwrap();
    let tinted = pixel(&out, 16);
    assert!(tinted[0].abs_diff(29) <= 3 && tinted[0] == tinted[1] && tinted[1] == tinted[2]);
    assert_eq!(pixel(&out, 48), [255, 0, 0, 255]);
}
