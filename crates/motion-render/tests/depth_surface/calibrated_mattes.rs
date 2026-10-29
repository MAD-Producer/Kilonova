//! A track matte is projected in screen coordinates before physical composition.
use super::*;
use motion_model::compositing::{MatteMode, TrackMatte};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn processed_visible_spatial_matte_refuses_without_flat_overpainting() {
    let root = tempfile::tempdir().unwrap();
    let authored = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &authored, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut baseline = sampled(&authored);
    attach(&mut baseline);
    let expected = renderer.capture(&baseline, &target).unwrap().0;
    for masked in [false, true] {
        let mut p = authored.clone();
        p.layers[0].track_matte = Some(TrackMatte {
            source: 2,
            mode: MatteMode::Alpha,
            hide_source: false,
        });
        if masked {
            p.layers[1]
                .masks
                .push(calibrated_masks::right_source_half());
        } else {
            let package = motion_effects::builtin::package().unwrap();
            let definition = package
                .manifest
                .effects
                .iter()
                .find(|effect| effect.id == "fill")
                .unwrap();
            p.layers[1].effects.push(motion_core::EffectInstance::new(
                1,
                &package.manifest.id,
                &package.manifest.version,
                &package.hash,
                definition,
                [64., 40.],
            ));
            p.rebuild_plugin_dependencies();
        }
        p.validate().unwrap();
        let mut scene = sampled(&p);
        attach(&mut scene);
        let result = renderer.capture(&scene, &target);
        assert!(
            result.is_err(),
            "processed spatial matte must keep its physical depth"
        );
        let cause = result.unwrap_err().to_string();
        assert!(
            cause.contains("object=1")
                && cause.contains("source=7")
                && cause.contains("matte source=2")
                && cause.contains("depth composition path"),
            "{cause}"
        );
        assert_eq!(renderer.read_target(&target).unwrap(), expected);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn effected_calibrated_source_for_calibrated_receiver_refuses_flat_materialization_and_keeps_front() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut baseline = sampled(&p);
    attach(&mut baseline);
    let expected = renderer.capture(&baseline, &target).unwrap().0;
    p.layers[1].content = Content::Image { asset: 7 };
    p.layers[1].depth_calibration = p.layers[0].depth_calibration.clone();
    p.layers[1].track_matte = Some(TrackMatte {
        source: 1,
        mode: MatteMode::Alpha,
        hide_source: false,
    });
    // Plain calibrated sources now have an independent geometry path. An
    // effected calibrated source still requires its separate effects contract.
    let package = motion_effects::builtin::package().unwrap();
    let definition = package.manifest.effects.iter().find(|e| e.id == "fill").unwrap();
    p.layers[0].effects.push(motion_core::EffectInstance::new(1, &package.manifest.id,
        &package.manifest.version, &package.hash, definition, [64., 40.]));
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let mut scene = sampled(&p);
    attach(&mut scene);
    let resource = field(1);
    scene
        .attach_calibrated_depth(2, &resource.descriptor().source, &resource, input())
        .unwrap();
    let result = renderer.capture(&scene, &target);
    assert!(
        result.is_err(),
        "must not flatten a calibrated matte source"
    );
    let cause = result.unwrap_err().to_string();
    assert!(
        cause.contains("object=1")
            && cause.contains("source=7")
            && cause.contains("depth composition path"),
        "{cause}"
    );
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn hidden_matte_between_spatial_layers_does_not_split_physical_depth() {
    let root = tempfile::tempdir().unwrap();
    let authored = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &authored, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut baseline = sampled(&authored);
    attach(&mut baseline);
    let expected = renderer.capture(&baseline, &target).unwrap().0;
    for position in 0..3 {
        let mut p = authored.clone();
        let mut source = Layer::solid(
            91,
            "Hidden full coverage",
            [96., 64.],
            [48., 32., 0.],
            [1.; 4],
        );
        source.visible = false;
        p.layers[0].track_matte = Some(TrackMatte {
            source: 91,
            mode: MatteMode::Alpha,
            hide_source: true,
        });
        p.layers.insert(position, source);
        p.validate().unwrap();
        let mut scene = sampled(&p);
        attach(&mut scene);
        assert_eq!(
            renderer.capture(&scene, &target).unwrap().0,
            expected,
            "hidden source order={position}"
        );
    }
}

fn srgb(linear: f32) -> u8 {
    let v = if linear <= 0.0031308 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1. / 2.4) - 0.055
    };
    (v * 255.).round() as u8
}
fn ray(pixels: &[u8], x: usize) -> &[u8] {
    &pixels[(32 * 96 + x) * 4..][..4]
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_receiver_four_matte_modes_use_screen_coverage_before_depth() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let root = tempfile::tempdir().unwrap();
        let mut p = project(root.path());
        p.working_precision = precision;
        let near =
            DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
        let mut source = Layer::solid(
            91,
            "Independent green half-alpha matte",
            [48., 64.],
            [24., 32., 0.],
            [0., 1., 0., 1.],
        );
        source.transform.opacity.value = 0.5;
        source.visible = false;
        p.layers.push(source);
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            motion_render::AllocationBudget::new(128 << 20).unwrap(),
        ))
        .unwrap();
        prepare(&mut renderer, &p, root.path());
        let target = renderer
            .sdr_capture_target(96, 64, motion_render::OutputTransform::SDR_CLIP)
            .unwrap();
        for mode in [
            MatteMode::Alpha,
            MatteMode::AlphaInverted,
            MatteMode::Luma,
            MatteMode::LumaInverted,
        ] {
            p.layers[0].track_matte = Some(TrackMatte {
                source: 91,
                mode,
                hide_source: true,
            });
            p.validate().unwrap();
            let mut scene = sampled(&p);
            scene
                .attach_calibrated_depth(1, &near.descriptor().source, &near, input())
                .unwrap();
            let actual = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
            // The matte is a 2D left half of the composition. Both red depth
            // samples are in front of blue; uncovered red must reveal blue.
            let mut inside = if mode.luma() { 0.7152 * 0.5 } else { 0.5 };
            let outside = if mode.inverted() {
                inside = 1. - inside;
                1.
            } else {
                0.
            };
            for (x, coverage) in [(36, inside), (60, outside)] {
                let expected = [srgb(coverage), 0, srgb(1. - coverage), 255];
                let actual = ray(&actual, x);
                assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                    "{precision:?} {mode:?} screen ray={x}: actual={actual:?} expected={expected:?}");
            }
            retain_fixture(
                &format!("calibrated-matte-{precision:?}-{mode:?}.png"),
                &actual,
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn effected_matte_and_source_mask_keep_reverse_frames_and_budget_recovery() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let mut source = Layer::solid(
        91,
        "Effected matte source",
        [48., 64.],
        [24., 32., 0.],
        [1.; 4],
    );
    source.visible = false;
    source.transform.opacity.value = 0.5;
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "fill")
        .unwrap();
    let mut fill = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [48., 64.],
    );
    fill.params
        .values_mut()
        .find(|p| p.kind == motion_effects::ParamKind::Color)
        .unwrap()
        .track
        .value = [0., 1., 0., 1.];
    source.effects.push(fill);
    p.layers.push(source);
    p.layers[0]
        .masks
        .push(calibrated_masks::right_source_half());
    p.layers[0].masks[0].inverted = true;
    p.layers[0].track_matte = Some(TrackMatte {
        source: 91,
        mode: MatteMode::Luma,
        hide_source: true,
    });
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let near = DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
    let sample = |p: &Project| {
        let mut scene = sampled(p);
        scene
            .attach_calibrated_depth(1, &near.descriptor().source, &near, input())
            .unwrap();
        scene
    };
    let a_scene = sample(&p);
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let expected_a = renderer.capture(&a_scene, &a).unwrap().0;
    let expected = [srgb(0.7152 * 0.5), 0, srgb(1. - 0.7152 * 0.5), 255];
    assert!(ray(&expected_a, 36)
        .iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= 1));
    assert_eq!(ray(&expected_a, 60), &[0, 0, 255, 255]);
    p.layers[0].track_matte.as_mut().unwrap().mode = MatteMode::AlphaInverted;
    p.layers[0].masks[0].inverted = false;
    let b_scene = sample(&p);
    let expected_b = renderer.capture(&b_scene, &b).unwrap().0;
    assert_eq!(ray(&expected_b, 36), &[0, 0, 255, 255]);
    assert_eq!(ray(&expected_b, 60), &[255, 0, 0, 255]);
    assert_ne!(expected_a, expected_b);
    let mut first = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&a_scene, &a.view, 96, 64, &mut first, None)
        .unwrap();
    let mut last = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&b_scene, &b.view, 96, 64, &mut last, None)
        .unwrap();
    renderer.submit_allocated(last.finish(), |_| {}).unwrap();
    renderer.submit_allocated(first.finish(), |_| {}).unwrap();
    assert_eq!(renderer.read_target(&a).unwrap(), expected_a);
    assert_eq!(renderer.read_target(&b).unwrap(), expected_b);
    let budget = renderer.allocation_budget().clone();
    let info = budget.info().unwrap();
    budget.set_limit(info.admitted_bytes).unwrap();
    let error = renderer.capture(&a_scene, &b).unwrap_err();
    assert!(error.resource_rejection().is_some(), "{error}");
    budget.set_limit(info.budget_bytes).unwrap();
    assert_eq!(renderer.read_target(&b).unwrap(), expected_b);
    assert_eq!(renderer.capture(&a_scene, &b).unwrap().0, expected_a);
    let used = budget.info().unwrap().admitted_bytes;
    let mut abandoned = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&b_scene, &b.view, 96, 64, &mut abandoned, None)
        .unwrap();
    drop(abandoned);
    renderer
        .submit_allocated(
            renderer
                .allocation_command_encoder(&Default::default())
                .unwrap()
                .finish(),
            |_| {},
        )
        .unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(renderer.read_target(&b).unwrap(), expected_a);
    assert_eq!(budget.info().unwrap().admitted_bytes, used);
    retain_fixture("effected-matte-mask-first.png", &expected_a);
    retain_fixture("effected-matte-mask-last.png", &expected_b);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn masked_chained_matte_keeps_each_sources_coordinates_and_pending_revision() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let mut source = Layer::solid(
        91,
        "Masked green source",
        [64., 40.],
        [48., 32., 0.],
        [0., 1., 0., 1.],
    );
    source.visible = false;
    source.transform.opacity.value = 0.5;
    source.masks.push(calibrated_masks::right_source_half());
    source.track_matte = Some(TrackMatte {
        source: 92,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    let mut cut = Layer::solid(92, "Right screen half", [48., 64.], [72., 32., 0.], [1.; 4]);
    cut.visible = false;
    p.layers.extend([source, cut]);
    p.layers[0].track_matte = Some(TrackMatte {
        source: 91,
        mode: MatteMode::Luma,
        hide_source: true,
    });
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let near = DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
    let sample = |p: &Project| {
        let mut scene = sampled(p);
        scene
            .attach_calibrated_depth(1, &near.descriptor().source, &near, input())
            .unwrap();
        scene
    };
    let a_scene = sample(&p);
    p.layers.iter_mut().find(|l| l.id == 91).unwrap().masks[0].inverted = true;
    p.layers
        .iter_mut()
        .find(|l| l.id == 91)
        .unwrap()
        .track_matte
        .as_mut()
        .unwrap()
        .mode = MatteMode::AlphaInverted;
    let b_scene = sample(&p);
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let expected_a = renderer.capture(&a_scene, &a).unwrap().0;
    let expected_b = renderer.capture(&b_scene, &b).unwrap().0;
    let blended = [srgb(0.7152 * 0.5), 0, srgb(1. - 0.7152 * 0.5), 255];
    for (pixels, covered, uncovered) in [(&expected_a, 60, 36), (&expected_b, 36, 60)] {
        assert!(
            ray(pixels, covered)
                .iter()
                .zip(blended)
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "chain coverage: {:?}",
            ray(pixels, covered)
        );
        assert_eq!(ray(pixels, uncovered), &[0, 0, 255, 255]);
    }
    let mut first = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&a_scene, &a.view, 96, 64, &mut first, None)
        .unwrap();
    let mut last = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&b_scene, &b.view, 96, 64, &mut last, None)
        .unwrap();
    renderer.submit_allocated(last.finish(), |_| {}).unwrap();
    renderer.submit_allocated(first.finish(), |_| {}).unwrap();
    assert_eq!(renderer.read_target(&a).unwrap(), expected_a);
    assert_eq!(renderer.read_target(&b).unwrap(), expected_b);
    retain_fixture("chained-matte-source-mask-a.png", &expected_a);
    retain_fixture("chained-matte-source-mask-b.png", &expected_b);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn spatial_matte_uses_edited_camera_and_letterboxed_native_pixels() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let mut source = Layer::solid(
        91,
        "Spatial green half",
        [32., 64.],
        [40., 32., 0.],
        [0., 1., 0., 1.],
    );
    source.three_d = true;
    source.visible = false;
    p.layers.push(source);
    p.layers[0].track_matte = Some(TrackMatte {
        source: 91,
        mode: MatteMode::Luma,
        hide_source: true,
    });
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let (width, height) = (192usize, 160usize);
    let target = renderer
        .capture_target(width as u32, height as u32)
        .unwrap();
    for moved in [false, true] {
        if moved {
            p.camera.position.set_at(0, [61., 27., -100.]).unwrap();
            p.layers
                .iter_mut()
                .find(|l| l.id == 91)
                .unwrap()
                .transform
                .rotation
                .set_at(0, [0., 18., 12.])
                .unwrap();
        }
        p.validate().unwrap();
        let mut scene = sampled(&p);
        attach(&mut scene);
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        // Composition 96x64 fits 192x128, with 16 transparent rows per side.
        assert!(pixels[..16 * width * 4].iter().all(|v| *v == 0));
        assert!(pixels[144 * width * 4..].iter().all(|v| *v == 0));
        let inverse = scene.camera.view_projection.as_dmat4().inverse();
        let red = scene.layers.iter().find(|l| l.id == 1).unwrap();
        let blue = scene.layers.iter().find(|l| l.id == 2).unwrap();
        let source = scene.layers.iter().find(|l| l.id == 91).unwrap();
        let mut checked = [0usize; 3];
        for y in 18..142usize {
            for x in 2..190usize {
                let ndc = [
                    (x as f64 + 0.5) / 96. - 1.,
                    1. - (y as f64 + 0.5 - 16.) / 64.,
                ];
                let a = inverse * DVec4::new(ndc[0], ndc[1], 0., 1.);
                let b = inverse * DVec4::new(ndc[0], ndc[1], 0.5, 1.);
                let origin = a.truncate() / a.w;
                let direction = (b.truncate() / b.w - origin).normalize();
                let Some(red_t) = surface_hit(origin, direction, red.model.as_dmat4()) else {
                    continue;
                };
                let intersect = |layer: &motion_render::DrawLayer| {
                    let inv = layer.model.as_dmat4().inverse();
                    let o = inv.transform_point3(origin);
                    let d = inv.transform_vector3(direction);
                    let t = -o.z / d.z;
                    (t, o + d * t)
                };
                let (blue_t, q) = intersect(blue);
                if blue_t <= 0. || q.x.abs() > 30. || q.y.abs() > 18. || (red_t - blue_t).abs() < 2.
                {
                    continue;
                }
                let (matte_t, q) = intersect(source);
                // Skip the projected AA/filter footprint, test both interiors.
                if (q.x.abs() - 16.).abs() < 2. || (q.y.abs() - 32.).abs() < 2. {
                    continue;
                }
                let covered = matte_t > 0. && q.x.abs() < 16. && q.y.abs() < 32.;
                let category = if red_t > blue_t {
                    0
                } else if covered {
                    1
                } else {
                    2
                };
                let c = if category == 1 { 0.7152 } else { 0. };
                let expected = [srgb(c), 0, srgb(1. - c), 255];
                let actual = &pixels[(y * width + x) * 4..][..4];
                assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1), "moved={moved} ray={x},{y} category={category} actual={actual:?} expected={expected:?}");
                checked[category] += 1;
            }
        }
        assert!(
            checked.iter().all(|n| *n >= 30),
            "each physical/matte region needs coverage: {checked:?}"
        );
        if let Some(dir) = std::env::var_os("MOTION_DEPTH_ACCEPTANCE_DIR") {
            image::save_buffer(
                std::path::PathBuf::from(dir).join(format!("spatial-matte-letterbox-{moved}.png")),
                &pixels,
                width as u32,
                height as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn changing_video_matte_does_not_replace_a_pending_projected_source() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let mut video = video::video_project(root.path()).video_assets.remove(0);
    video.id = 8;
    video.width = 8;
    video.display_width = 8;
    p.video_assets.push(video);
    let mut source = Layer::solid(
        91,
        "Changing video matte",
        [64., 40.],
        [48., 32., 0.],
        [1.; 4],
    );
    source.visible = false;
    source.content = Content::Video {
        video: motion_core::VideoClip::new(8),
    };
    p.layers.push(source);
    p.layers[0].track_matte = Some(TrackMatte {
        source: 91,
        mode: MatteMode::Luma,
        hide_source: true,
    });
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let near = DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let mut scenes = Vec::new();
    for i in 0..2usize {
        let mut scene = Scene::new(&p).unwrap();
        scene
            .sample(&p, (i * 30) as f64, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        scene
            .attach_calibrated_depth(1, &near.descriptor().source, &near, input())
            .unwrap();
        assert_eq!(scene.video_decode_layers().len(), 1);
        assert_eq!(
            scene.video_decode_layers()[0]
                .video
                .as_ref()
                .unwrap()
                .source_time_us,
            i as u64 * 1_000_000
        );
        let colors: Vec<u8> = (0..8)
            .flat_map(|x| {
                if (x < 4) == (i == 0) {
                    [0, 255, 0, 255]
                } else {
                    [0; 4]
                }
            })
            .collect();
        renderer
            .upload_video_frame(91, 8, i as u64 * 1_000_000, 8, 1, &colors)
            .unwrap();
        let target = if i == 0 { &a } else { &b };
        let pixels = renderer.capture(&scene, target).unwrap().0;
        let blended = [srgb(0.7152), 0, srgb(1. - 0.7152), 255];
        let (covered, uncovered) = if i == 0 { (36, 60) } else { (60, 36) };
        assert!(
            ray(&pixels, covered)
                .iter()
                .zip(blended)
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "video matte frame={i}: {:?}",
            ray(&pixels, covered)
        );
        assert_eq!(ray(&pixels, uncovered), &[0, 0, 255, 255]);
        retain_fixture(&format!("changing-video-matte-{i}.png"), &pixels);
        scenes.push((scene, colors, pixels));
    }
    renderer
        .upload_video_frame(91, 8, 0, 8, 1, &scenes[0].1)
        .unwrap();
    let mut first = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scenes[0].0, &a.view, 96, 64, &mut first, None)
        .unwrap();
    renderer
        .upload_video_frame(91, 8, 1_000_000, 8, 1, &scenes[1].1)
        .unwrap();
    let mut last = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scenes[1].0, &b.view, 96, 64, &mut last, None)
        .unwrap();
    renderer.submit_allocated(last.finish(), |_| {}).unwrap();
    renderer.submit_allocated(first.finish(), |_| {}).unwrap();
    assert_eq!(renderer.read_target(&a).unwrap(), scenes[0].2);
    assert_eq!(renderer.read_target(&b).unwrap(), scenes[1].2);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn visible_matte_source_keeps_its_independent_alpha_and_authored_stacking() {
    let root = tempfile::tempdir().unwrap();
    let authored = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &authored, root.path());
    let near = DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
    let target = renderer.capture_target(96, 64).unwrap();
    for front in [false, true] {
        let mut p = authored.clone();
        let mut source = Layer::solid(
            91,
            "Visible half-alpha green matte",
            [64., 40.],
            [48., 32., 0.],
            [0., 1., 0., 1.],
        );
        source.transform.opacity.value = 0.5;
        p.layers[0].track_matte = Some(TrackMatte {
            source: 91,
            mode: MatteMode::Alpha,
            hide_source: false,
        });
        // Authored project layers are stored back-to-front.
        p.layers.insert(if front { 2 } else { 0 }, source);
        p.validate().unwrap();
        let mut scene = sampled(&p);
        scene
            .attach_calibrated_depth(1, &near.descriptor().source, &near, input())
            .unwrap();
        let actual = renderer.capture(&scene, &target).unwrap().0;
        // Independent alpha first leaves red/blue at 1/2 each. A visible
        // foreground green then contributes 1/2, leaving red/blue at 1/4;
        // behind the opaque blue plane it contributes nothing to these rays.
        let linear = if front {
            [0.25, 0.5, 0.25]
        } else {
            [0.5, 0., 0.5]
        };
        let expected = [srgb(linear[0]), srgb(linear[1]), srgb(linear[2]), 255];
        for x in [36, 60] {
            assert!(
                ray(&actual, x)
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "front={front} x={x}: {:?} expected={expected:?}",
                ray(&actual, x)
            );
        }
        retain_fixture(&format!("visible-matte-front-{front}.png"), &actual);
    }
}
