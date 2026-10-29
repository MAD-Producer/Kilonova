//! A calibrated matte is its own projected geometry, independent of the backdrop.
use super::*;
use motion_model::compositing::{MatteMode, TrackMatte};

fn srgb(c: f32) -> u8 {
    let c = if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    };
    (c * 255.).round() as u8
}

fn source_project(root: &std::path::Path) -> (Project, DepthResource) {
    let mut p = project(root);
    image::save_buffer(
        root.join("assets/green.png"),
        &[0, 255, 0, 255, 0, 255, 0, 255],
        2,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    p.assets.push(Asset {
        id: 8,
        path: "assets/green.png".into(),
        width: 2,
        height: 1,
    });
    let mut source = p.layers[0].clone();
    source.id = 91;
    source.content = Content::Image { asset: 8 };
    source.depth_calibration = None;
    // Keep the unreferenced layer in this reference-only sample. Hide it
    // after installing the matte dependency below.
    source.visible = true;
    source.transform.position.set_at(0, [58., 32., 0.]).unwrap();
    p.layers.push(source);
    let reference = sampled(&p);
    let layer = reference.layers.iter().find(|l| l.id == 91).unwrap();
    let mut descriptor = field(1).descriptor().clone();
    descriptor.source.asset = 8;
    let resource = DepthResource::new(descriptor, vec![1., 1.], None, None).unwrap();
    p.layers.last_mut().unwrap().depth_calibration = Some(
        SavedDepthCalibration::new(
            &resource,
            input(),
            [
                DepthAnchor {
                    value: 0.,
                    camera_z_meters: 12.,
                },
                DepthAnchor {
                    value: 1.,
                    camera_z_meters: 8.,
                },
            ],
            CalibrationCurve::CameraZ,
            CalibrationExtrapolation::Reject,
            10.,
        )
        .unwrap()
        .with_reference(DepthReference {
            camera: reference.native_camera(),
            world: layer.model,
            size: layer.size,
        })
        .unwrap(),
    );
    p.layers[0].track_matte = Some(TrackMatte {
        source: 91,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    p.layers.last_mut().unwrap().visible = false;
    p.validate().unwrap();
    (p, resource)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_matte_source_matches_its_own_geometry_after_camera_edits() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let root = tempfile::tempdir().unwrap();
        let (mut p, resource) = source_project(root.path());
        p.working_precision = precision;
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            motion_render::AllocationBudget::new(128 << 20).unwrap(),
        ))
        .unwrap();
        prepare(&mut renderer, &p, root.path());
        let target = renderer
            .sdr_capture_target(96, 64, motion_render::OutputTransform::SDR_CLIP)
            .unwrap();
        for moved in [false, true] {
            if moved {
                p.camera.position.set_at(0, [61., 27., -100.]).unwrap();
            }
            for mode in [
                MatteMode::Alpha,
                MatteMode::AlphaInverted,
                MatteMode::Luma,
                MatteMode::LumaInverted,
            ] {
                p.layers[0].track_matte.as_mut().unwrap().mode = mode;
                let mut scene = sampled(&p);
                attach(&mut scene);
                scene
                    .attach_calibrated_depth(91, &resource.descriptor().source, &resource, input())
                    .unwrap();
                let pixels = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
                let red = scene.layers.iter().find(|l| l.id == 1).unwrap();
                let blue = scene.layers.iter().find(|l| l.id == 2).unwrap();
                let source = scene.layers.iter().find(|l| l.id == 91).unwrap();
                let inverse = scene.camera.view_projection.as_dmat4().inverse();
                let mut checked = [0usize; 3];
                for y in 0..64usize {
                    for x in 0..96usize {
                        let ndc = [(x as f64 + 0.5) / 48. - 1., 1. - (y as f64 + 0.5) / 32.];
                        let a = inverse * DVec4::new(ndc[0], ndc[1], 0., 1.);
                        let b = inverse * DVec4::new(ndc[0], ndc[1], 0.5, 1.);
                        let origin = a.truncate() / a.w;
                        let direction = (b.truncate() / b.w - origin).normalize();
                        let Some(red_t) = surface_hit(origin, direction, red.model.as_dmat4())
                        else {
                            continue;
                        };
                        let plane = |world: glam::DMat4, z: f64| {
                            let inv = world.inverse();
                            let o = inv.transform_point3(origin);
                            let d = inv.transform_vector3(direction);
                            let t = (z - o.z) / d.z;
                            (t, o + d * t)
                        };
                        let (blue_t, q) = plane(blue.model.as_dmat4(), 0.);
                        if blue_t <= 0.
                            || q.x.abs() > 30.
                            || q.y.abs() > 18.
                            || (blue_t - red_t).abs() < 2.
                        {
                            continue;
                        }
                        // Source depth is uniformly near: reference Z=100 -> 80,
                        // local Z=20 and XY=0.8 times the authored 64x40 rectangle.
                        // Its reference centre is ten pixels off the optical
                        // axis, so unprojection moves its local X centre -2.
                        let (source_t, q) = plane(source.model.as_dmat4(), 20.);
                        let source_x = q.x + 2.;
                        if (source_x.abs() - 25.6).abs() < 2. || (q.y.abs() - 16.).abs() < 2. {
                            continue;
                        }
                        let covered = source_t > 0. && source_x.abs() < 25.6 && q.y.abs() < 16.;
                        let category = if red_t > blue_t {
                            0
                        } else if covered {
                            1
                        } else {
                            2
                        };
                        let mut c = if covered {
                            if mode.luma() {
                                0.7152
                            } else {
                                1.
                            }
                        } else {
                            0.
                        };
                        if mode.inverted() {
                            c = 1. - c;
                        }
                        if red_t > blue_t {
                            c = 0.;
                        }
                        let expected = [srgb(c), 0, srgb(1. - c), 255];
                        let actual = &pixels[(y * 96 + x) * 4..][..4];
                        assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1), "{precision:?} {mode:?} moved={moved} ray={x},{y} actual={actual:?} expected={expected:?}");
                        checked[category] += 1;
                    }
                }
                assert!(
                    checked.iter().all(|n| *n >= 10),
                    "each geometric region needs interior evidence: {checked:?}"
                );
                retain_fixture(
                    &format!("native-calibrated-matte-{precision:?}-{mode:?}-{moved}.png"),
                    &pixels,
                );
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_source_mask_chain_preserves_reverse_frames_and_admission_recovery() {
    let root = tempfile::tempdir().unwrap();
    let (mut p, resource) = source_project(root.path());
    let source = p.layers.iter_mut().find(|l| l.id == 91).unwrap();
    source.transform.position.set_at(0, [40., 32., 0.]).unwrap();
    source.masks.push(calibrated_masks::right_source_half());
    let mut child = source.clone();
    child.id = 92;
    source.track_matte = Some(TrackMatte {
        source: 92,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    p.layers.push(child);
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let near = DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
    let sample = |p: &Project| {
        let mut scene = sampled(p);
        scene
            .attach_calibrated_depth(1, &near.descriptor().source, &near, input())
            .unwrap();
        for source in [91, 92] {
            scene
                .attach_calibrated_depth(source, &resource.descriptor().source, &resource, input())
                .unwrap();
        }
        scene
    };
    let a_scene = sample(&p);
    let source = p.layers.iter_mut().find(|l| l.id == 91).unwrap();
    source.masks[0].inverted = true;
    source.track_matte.as_mut().unwrap().mode = MatteMode::AlphaInverted;
    let b_scene = sample(&p);
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let expected_a = renderer.capture(&a_scene, &a).unwrap().0;
    let expected_b = renderer.capture(&b_scene, &b).unwrap().0;
    let ray = |pixels: &[u8], x: usize| pixels[(32 * 96 + x) * 4..][..4].to_vec();
    assert_eq!(ray(&expected_a, 44), [255, 0, 0, 255]);
    assert_eq!(ray(&expected_a, 36), [0, 0, 255, 255]);
    assert_eq!(ray(&expected_b, 44), [0, 0, 255, 255]);
    assert_eq!(ray(&expected_b, 36), [255, 0, 0, 255]);
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
    let admitted = budget.info().unwrap().admitted_bytes;
    let mut abandoned = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&b_scene, &b.view, 96, 64, &mut abandoned, None)
        .unwrap();
    drop(abandoned);
    let flush = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(flush.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(renderer.read_target(&b).unwrap(), expected_a);
    assert_eq!(budget.info().unwrap().admitted_bytes, admitted);
    retain_fixture("calibrated-matte-chain-a.png", &expected_a);
    retain_fixture("calibrated-matte-chain-b.png", &expected_b);
    renderer.clear_assets();
    renderer.configure_assets(&p, root.path()).unwrap();
    for (scene, expected) in [(&a_scene, &expected_a), (&b_scene, &expected_b)] {
        assert!(renderer
            .prepare_scene_assets(
                scene,
                motion_render::image_resources::Resolution::Full,
                false
            )
            .unwrap());
        assert_eq!(&renderer.capture(scene, &a).unwrap().0, expected);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn visible_calibrated_matte_source_keeps_its_physical_depth_behind_the_backdrop() {
    let root = tempfile::tempdir().unwrap();
    let (mut p, resource) = source_project(root.path());
    let far = DepthResource::new(resource.descriptor().clone(), vec![0., 0.], None, None).unwrap();
    p.layers.iter_mut().find(|l| l.id == 91).unwrap().visible = true;
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    for hide_source in [true, false] {
        p.layers[0].track_matte.as_mut().unwrap().hide_source = hide_source;
        let mut scene = sampled(&p);
        attach(&mut scene);
        scene
            .attach_calibrated_depth(91, &far.descriptor().source, &far, input())
            .unwrap();
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        // The green matte has depth120, behind blue100. Its independently
        // projected alpha still reveals red80 at x40; red120 at x60 remains
        // behind blue. Making the matte visible must not paint green on top.
        assert_eq!(&pixels[(32 * 96 + 40) * 4..][..4], &[255, 0, 0, 255]);
        assert_eq!(&pixels[(32 * 96 + 60) * 4..][..4], &[0, 0, 255, 255]);
        retain_fixture(
            &format!("visible-calibrated-matte-hidden-{hide_source}.png"),
            &pixels,
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn owned_video_calibrated_matte_keeps_color_revisions_and_refuses_missing_owned_pixels() {
    use motion_render::{image_resources::Resolution, DepthImage};
    let root = tempfile::tempdir().unwrap();
    let (mut p, resource) = source_project(root.path());
    p.assets.retain(|a| a.id != 8);
    let mut asset = video::video_project(root.path()).video_assets.remove(0);
    asset.id = 8;
    p.video_assets.push(asset);
    p.layers.iter_mut().find(|l| l.id == 91).unwrap().content = Content::Video {
        video: motion_core::VideoClip::new(8),
    };
    p.layers[0].track_matte.as_mut().unwrap().mode = MatteMode::Luma;
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&p, root.path()).unwrap();
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let mut scenes = Vec::new();
    for (i, color, luminance) in [(0, [0, 255, 0, 255], 0.7152), (1, [255, 0, 0, 255], 0.2126)] {
        let mut scene = sampled(&p);
        attach(&mut scene);
        scene
            .attach_calibrated_depth_image(
                91,
                &resource.descriptor().source,
                &resource,
                input(),
                DepthImage::new([2, 1], color.repeat(2)).unwrap(),
            )
            .unwrap();
        assert!(
            scene.video_decode_layers().is_empty(),
            "the absent codec file must not be reopened"
        );
        assert!(renderer
            .prepare_scene_assets(&scene, Resolution::Full, false)
            .unwrap());
        let pixels = renderer
            .capture(&scene, if i == 0 { &a } else { &b })
            .unwrap()
            .0;
        let expected = [srgb(luminance), 0, srgb(1. - luminance), 255];
        assert!(pixels[(32 * 96 + 40) * 4..][..4]
            .iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(b) <= 1));
        assert_eq!(&pixels[(32 * 96 + 60) * 4..][..4], &[0, 0, 255, 255]);
        retain_fixture(&format!("owned-video-calibrated-matte-{i}.png"), &pixels);
        scenes.push((scene, pixels));
    }
    assert_ne!(scenes[0].1, scenes[1].1);
    assert!(renderer
        .prepare_scene_assets(&scenes[0].0, Resolution::Full, false)
        .unwrap());
    let mut first = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scenes[0].0, &a.view, 96, 64, &mut first, None)
        .unwrap();
    assert!(renderer
        .prepare_scene_assets(&scenes[1].0, Resolution::Full, false)
        .unwrap());
    let mut last = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scenes[1].0, &b.view, 96, 64, &mut last, None)
        .unwrap();
    renderer.submit_allocated(last.finish(), |_| {}).unwrap();
    renderer.submit_allocated(first.finish(), |_| {}).unwrap();
    assert_eq!(renderer.read_target(&a).unwrap(), scenes[0].1);
    assert_eq!(renderer.read_target(&b).unwrap(), scenes[1].1);
    // Geometry alone cannot authorize reusing a previous video color revision.
    let mut missing = sampled(&p);
    attach(&mut missing);
    missing
        .attach_calibrated_depth(91, &resource.descriptor().source, &resource, input())
        .unwrap();
    assert_eq!(missing.video_decode_layers().len(), 1);
    let result = renderer.capture(&missing, &b);
    assert!(
        result.is_err(),
        "stale video pixels must not substitute for the missing owned image"
    );
    let cause = result.unwrap_err().to_string();
    assert!(
        cause.contains("object=91") && cause.contains("source=8"),
        "{cause}"
    );
    assert_eq!(renderer.read_target(&b).unwrap(), scenes[1].1);
    renderer.clear_assets();
    renderer.configure_assets(&p, root.path()).unwrap();
    for (scene, expected) in &scenes {
        assert!(renderer
            .prepare_scene_assets(scene, Resolution::Full, false)
            .unwrap());
        assert_eq!(&renderer.capture(scene, &a).unwrap().0, expected);
    }
}
