use motion_core::{render_sampling::FrameBlend, Content, Layer, Project, VideoAsset, VideoClip};
use motion_render::{Renderer, Scene};

fn fixture() -> (Project, Scene) {
    let mut p = Project::new(16, 16, 30, 60).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.background = [0.; 4];
    p.video_assets.push(VideoAsset {
        id: 1,
        path: "assets/authored.mp4".into(),
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
        frame_count: 2,
        variable_frame_rate: true,
        nominal_frame_rate: 24.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    let mut layer = Layer::solid(1, "Authored video", [16.; 2], [8., 8., 0.], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip::new(1),
    };
    layer.sampling.frame_blend = FrameBlend::Linear;
    p.layers.push(layer);
    p.validate().unwrap();
    let mut scene = Scene::new(&p).unwrap();
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    (p, scene)
}
fn cache(renderer: &mut Renderer, slot: u8, pts: u64, rgba: [u8; 4]) {
    renderer
        .upload_video_frame(1, 1, pts, 16, 16, &rgba.repeat(256))
        .unwrap();
    renderer.cache_video_blend_source(1, 1, pts, slot).unwrap();
}
fn pixel(renderer: &mut Renderer, scene: &Scene) -> [u8; 4] {
    let target = renderer.capture_target(16, 16).unwrap();
    let data = renderer.capture(scene, &target).unwrap().0;
    data[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4]
        .try_into()
        .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_gpu_blend_is_linear_and_caches_weights_without_stale_hold_pixels() {
    let (mut project, scene) = fixture();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    cache(&mut renderer, 0, 100_000, [0, 0, 0, 255]);
    cache(&mut renderer, 1, 170_000, [255; 4]);
    renderer
        .blend_video_sources(1, 1, 100_000, 170_000, 0.5)
        .unwrap();
    let mid = pixel(&mut renderer, &scene);
    assert!((187..=189).contains(&mid[0]), "linear midpoint={mid:?}");
    assert_eq!(mid[3], 255);
    let submissions = renderer.video_blend_submissions;
    let uploads = renderer.video_uploads;
    for _ in 0..5 {
        renderer
            .blend_video_sources(1, 1, 100_000, 170_000, 0.5)
            .unwrap();
    }
    assert_eq!(submissions, renderer.video_blend_submissions);
    renderer
        .blend_video_sources(1, 1, 100_000, 170_000, 0.25)
        .unwrap();
    let quarter = pixel(&mut renderer, &scene);
    assert!((136..=138).contains(&quarter[0]), "quarter={quarter:?}");
    assert_eq!(uploads, renderer.video_uploads);
    assert_eq!(submissions + 1, renderer.video_blend_submissions);
    assert_eq!(
        renderer.texture_memory_breakdown().video_rgba,
        3 * 16 * 16 * 4
    );
    let ledger_bytes: u64 = renderer
        .texture_resource_ledger()
        .resources()
        .filter(|r| {
            r.references
                .iter()
                .any(|r| r.category == motion_render::TextureCategory::VideoRgba)
        })
        .map(|r| r.logical_bytes.unwrap())
        .sum();
    assert_eq!(ledger_bytes, 3 * 16 * 16 * 4);
    project.layers[0].sampling.frame_blend = FrameBlend::Hold;
    let mut hold = Scene::new(&project).unwrap();
    hold.sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.retain_video_instances(&hold);
    assert!(!renderer.video_blend_source_ready(1, 1, 100_000, 0));
    renderer
        .upload_video_frame(1, 1, 170_000, 16, 16, &[255; 4].repeat(256))
        .unwrap();
    assert_eq!(pixel(&mut renderer, &hold), [255; 4]);
    assert_eq!(renderer.texture_memory_breakdown().video_rgba, 16 * 16 * 4);
    renderer.clear_assets();
    assert!(!renderer.video_blend_source_ready(1, 1, 170_000, 1));
    assert!(renderer
        .blend_video_sources(1, 1, 100_000, 170_000, 0.5)
        .is_err());
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn opaque_blend_quantizes_srgb_bytes_in_both_temporal_directions() {
    let (_, scene) = fixture();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    // Endpoint channels decode exactly. The expected bytes are calculated from
    // the transfer function in f64, independently of GPU attachment encoding.
    for (index, (left, right)) in [([255; 4], [255, 0, 0, 255]), ([255, 0, 0, 255], [255; 4])]
        .into_iter()
        .enumerate()
    {
        let left_pts = index as u64 * 200 + 100;
        let right_pts = left_pts + 100;
        cache(&mut renderer, 0, left_pts, left);
        cache(&mut renderer, 1, right_pts, right);
        for weight in [0.0f32, 0.25, 0.499, 0.5, 0.501, 0.75, 1.0] {
            renderer
                .blend_video_sources(1, 1, left_pts, right_pts, weight)
                .unwrap();
            let expected = std::array::from_fn(|channel| {
                if channel == 3 {
                    return 255;
                }
                let value = f64::from(left[channel]) / 255.0 * (1.0 - f64::from(weight))
                    + f64::from(right[channel]) / 255.0 * f64::from(weight);
                let encoded = if value <= 0.0031308 {
                    value * 12.92
                } else {
                    1.055 * value.powf(1.0 / 2.4) - 0.055
                };
                (encoded * 255.0).round() as u8
            });
            assert_eq!(
                pixel(&mut renderer, &scene),
                expected,
                "left={left:?} right={right:?} weight={weight} adapter={:?}",
                renderer.adapter_info
            );
        }
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn alpha_budget_refusal_recovery_and_invalid_sources_do_not_alias_objects() {
    let (_, scene) = fixture();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    cache(&mut renderer, 0, 100, [0; 4]);
    let mut policy = renderer.memory_policy();
    let original = policy;
    policy.source_texture_budget_bytes = 2 * 16 * 16 * 4 + 4;
    renderer.set_memory_policy(policy).unwrap();
    renderer
        .upload_video_frame(1, 1, 200, 16, 16, &[255, 0, 0, 255].repeat(256))
        .unwrap();
    let failure = renderer.cache_video_blend_source(1, 1, 200, 1).unwrap_err();
    assert!(failure.resource_rejection().is_some(), "{failure}");
    assert!(failure
        .to_string()
        .contains("target=VideoBlendSource(1, 1) source=1 phase=blend_allocate"));
    assert!(renderer.video_blend_source_ready(1, 1, 100, 0));
    assert!(!renderer.video_blend_source_ready(1, 1, 200, 1));
    renderer.set_memory_policy(original).unwrap();
    renderer.cache_video_blend_source(1, 1, 200, 1).unwrap();
    for (object, source, left, right, weight) in [
        (2, 1, 100, 200, 0.5),
        (1, 2, 100, 200, 0.5),
        (1, 1, 100, 200, f32::NAN),
        (1, 1, 200, 100, 0.5),
    ] {
        assert!(renderer
            .blend_video_sources(object, source, left, right, weight)
            .is_err());
    }
    renderer.blend_video_sources(1, 1, 100, 200, 0.5).unwrap();
    let alpha = pixel(&mut renderer, &scene);
    // capture returns straight sRGB, not the premultiplied texture bytes.
    assert!(
        alpha[0] >= 253 && (127..=128).contains(&alpha[3]),
        "alpha={alpha:?}"
    );
    renderer.reclaim_preview_working_set().unwrap();
    assert_eq!(renderer.texture_memory_breakdown().video_rgba, 0);
    assert!(!renderer.video_blend_source_ready(1, 1, 100, 0));
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn authored_moving_shape_records_transparent_images_and_resident_capture_cost() {
    let (mut project, _) = fixture();
    project.width = 192;
    project.height = 96;
    project.video_assets[0].width = 192;
    project.video_assets[0].height = 96;
    project.video_assets[0].display_width = 192;
    project.video_assets[0].display_height = 96;
    project.layers[0].size = [192., 96.];
    project.layers[0].transform.position.value = [96., 48., 0.];
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(192, 96).unwrap();
    let source = |x: usize| {
        let mut bytes = vec![0; 192 * 96 * 4];
        for y in 16..80 {
            for x in x..x + 32 {
                bytes[(y * 192 + x) * 4..(y * 192 + x) * 4 + 4]
                    .copy_from_slice(&[48, 192, 160, 255]);
            }
        }
        bytes
    };
    let left = source(32);
    let right = source(128);
    let began = std::time::Instant::now();
    renderer
        .upload_video_frame(1, 1, 0, 192, 96, &left)
        .unwrap();
    let hold = renderer.capture(&scene, &target).unwrap().0;
    let hold_us = began.elapsed().as_micros();
    let hold_bytes = renderer.texture_bytes();
    let mut hold_warm = Vec::new();
    for _ in 0..3 {
        let began = std::time::Instant::now();
        renderer
            .upload_video_frame(1, 1, 0, 192, 96, &left)
            .unwrap();
        assert_eq!(hold, renderer.capture(&scene, &target).unwrap().0);
        hold_warm.push(began.elapsed().as_micros());
    }
    let began = std::time::Instant::now();
    renderer.cache_video_blend_source(1, 1, 0, 0).unwrap();
    renderer
        .upload_video_frame(1, 1, 70_000, 192, 96, &right)
        .unwrap();
    renderer.cache_video_blend_source(1, 1, 70_000, 1).unwrap();
    renderer.blend_video_sources(1, 1, 0, 70_000, 0.5).unwrap();
    let blended = renderer.capture(&scene, &target).unwrap().0;
    let blend_us = began.elapsed().as_micros();
    let blend_bytes = renderer.texture_bytes();
    let mut blend_warm = Vec::new();
    for weight in [0.25, 0.75, 0.5] {
        let began = std::time::Instant::now();
        renderer
            .blend_video_sources(1, 1, 0, 70_000, weight)
            .unwrap();
        renderer.capture(&scene, &target).unwrap();
        blend_warm.push(began.elapsed().as_micros());
    }
    let alpha = |pixels: &[u8]| pixels.chunks_exact(4).map(|p| u64::from(p[3])).sum::<u64>();
    assert!(alpha(&hold).abs_diff(alpha(&blended)) <= 4096);
    for x in [48, 144] {
        let p = &blended[(48 * 192 + x) * 4..(48 * 192 + x) * 4 + 4];
        assert!((127..=128).contains(&p[3]), "{p:?}");
        assert!(p[..3]
            .iter()
            .zip([48u8, 192, 160])
            .all(|(a, b)| a.abs_diff(b) <= 1));
    }
    assert_eq!(&blended[..4], &[0; 4]);
    assert_eq!(renderer.video_uploads, 2);
    assert_eq!(blend_bytes - hold_bytes, 2 * 192 * 96 * 4);
    if let Some(folder) = std::env::var_os("MOTION_SAMPLING_ARTIFACT_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (name, pixels) in [
            ("hold-frame.png", hold),
            ("linear-frame-blend.png", blended),
        ] {
            image::RgbaImage::from_raw(192, 96, pixels)
                .unwrap()
                .save(folder.join(name))
                .unwrap();
        }
        std::fs::write(folder.join("frame-blend-cost.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "width":192,"height":96,"actual_pts_us":[0,70000],"hold":{"first_capture_us":hold_us,"warm_capture_us":hold_warm,"tracked_texture_bytes":hold_bytes},
            "linear":{"first_capture_us":blend_us,"warm_weight_change_capture_us":blend_warm,"tracked_texture_bytes":blend_bytes,"uploads":renderer.video_uploads,"blend_submissions":renderer.video_blend_submissions},
            "scope":"authored transparent moving rectangle, first and three resident captures; GPU readback included, not playback FPS or process VRAM"
        })).unwrap()).unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn desktop_bgra_target_effects_sample_weight_changes_from_the_same_video_view() {
    let (mut project, _) = fixture();
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "brightness_contrast")
        .unwrap();
    project.layers[0]
        .effects
        .push(motion_core::EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [16.; 2],
        ));
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ))
    .unwrap();
    cache(&mut renderer, 0, 100, [255, 0, 0, 255]);
    cache(&mut renderer, 1, 200, [0, 0, 255, 255]);
    for (weight, red, blue) in [(0.5, 188u8, 188u8), (0.25, 225, 137), (0.75, 137, 225)] {
        renderer
            .blend_video_sources(1, 1, 100, 200, weight)
            .unwrap();
        let actual = pixel(&mut renderer, &scene);
        assert!(
            actual[0].abs_diff(red) <= 1
                && actual[1] == 0
                && actual[2].abs_diff(blue) <= 1
                && actual[3] == 255,
            "{actual:?}"
        );
    }
    assert_eq!(renderer.video_uploads, 2);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unsubmitted_and_cancelled_yuv_are_not_cached_as_completed_blend_inputs() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let frame = motion_render::Yuv420Frame {
        width: 16,
        height: 16,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: motion_render::ChromaLayout::Uv,
        y: vec![235; 256],
        uv: vec![128; 128],
    };
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let upload = renderer
        .encode_video_yuv(1, 1, 100, &frame, &mut encoder)
        .unwrap()
        .unwrap();
    assert!(renderer
        .cache_video_blend_source(1, 1, 100, 0)
        .unwrap_err()
        .to_string()
        .contains("unsubmitted"));
    drop(encoder);
    drop(upload);
    let error = renderer.cache_video_blend_source(1, 1, 100, 0).unwrap_err();
    assert!(
        error.to_string().contains("completed source missing"),
        "{error}"
    );
    assert!(!renderer.video_blend_source_ready(1, 1, 100, 0));
    renderer.upload_video_yuv(1, 1, 100, &frame).unwrap();
    renderer.cache_video_blend_source(1, 1, 100, 0).unwrap();
    assert!(renderer.video_blend_source_ready(1, 1, 100, 0));
    renderer.check_health().unwrap();
}
