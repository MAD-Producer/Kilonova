//! Ordinary receivers consume independent native projected depth matte images.
use super::calibrated_matte_sources::source_project;
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

fn ray(pixels: &[u8], x: usize) -> &[u8] {
    &pixels[(32 * 96 + x) * 4..][..4]
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn ordinary_matte_chain_without_visible_geometry_keeps_pending_frames_and_recovers() {
    let root = tempfile::tempdir().unwrap();
    let (mut p, resource) = source_project(root.path());
    p.background = [0., 0., 1., 1.];
    let mut source = p.layers.pop().unwrap();
    // Exercise the entire user ID range; aliases cannot use max_id + 1.
    source.id = u64::MAX;
    source.transform.position.set_at(0, [40., 32., 0.]).unwrap();
    source.masks.push(calibrated_masks::right_source_half());
    let mut middle = Layer::solid(
        93,
        "Ordinary masked dependency",
        [96., 64.],
        [48., 32., 0.],
        [1.; 4],
    );
    middle.visible = false;
    middle.track_matte = Some(TrackMatte {
        source: source.id,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    let mut receiver = Layer::solid(
        90,
        "Ordinary chain receiver",
        [96., 64.],
        [48., 32., 0.],
        [1.; 4],
    );
    receiver.track_matte = Some(TrackMatte {
        source: 93,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    // There are no visible calibrated blocks. Only the independent source
    // projection should enter the ordinary compositing path.
    p.layers = vec![source, middle, receiver];
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let sample = |p: &Project| {
        let mut scene = sampled(p);
        scene
            .attach_calibrated_depth(u64::MAX, &resource.descriptor().source, &resource, input())
            .unwrap();
        scene
    };
    let a_scene = sample(&p);
    p.layers[0].masks[0].inverted = true;
    let b_scene = sample(&p);
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let expected_a = renderer.capture(&a_scene, &a).unwrap().0;
    let expected_b = renderer.capture(&b_scene, &b).unwrap().0;
    assert_eq!(ray(&expected_a, 44), [255; 4]);
    assert_eq!(ray(&expected_a, 36), [0, 0, 255, 255]);
    assert_eq!(ray(&expected_b, 44), [0, 0, 255, 255]);
    assert_eq!(ray(&expected_b, 36), [255; 4]);
    let letterbox = renderer.capture_target(160, 96).unwrap();
    let letterboxed = renderer.capture(&a_scene, &letterbox).unwrap().0;
    for (x, expected) in [(74, [255; 4]), (62, [0, 0, 255, 255])] {
        assert_eq!(
            &letterboxed[(48 * 160 + x) * 4..][..4],
            expected,
            "letterbox ray={x}"
        );
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
        .encode(&b_scene, &a.view, 96, 64, &mut abandoned, None)
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
    assert_eq!(budget.info().unwrap().admitted_bytes, admitted);
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
fn visible_calibrated_source_is_independent_of_ordinary_receiver_and_physical_backdrop() {
    let root = tempfile::tempdir().unwrap();
    let (mut p, resource) = source_project(root.path());
    p.layers[0].track_matte = None;
    p.layers.last_mut().unwrap().visible = true;
    let mut receiver = Layer::solid(
        90,
        "Ordinary half-opacity receiver",
        [96., 64.],
        [48., 32., 0.],
        [1.; 4],
    );
    receiver.transform.opacity.value = 0.5;
    receiver.track_matte = Some(TrackMatte {
        source: 91,
        mode: MatteMode::Luma,
        hide_source: false,
    });
    p.layers.push(receiver);
    p.validate().unwrap();
    let far = DepthResource::new(resource.descriptor().clone(), vec![0., 0.], None, None).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    for hidden in [false, true] {
        p.layers
            .last_mut()
            .unwrap()
            .track_matte
            .as_mut()
            .unwrap()
            .hide_source = hidden;
        let mut scene = sampled(&p);
        attach(&mut scene);
        scene
            .attach_calibrated_depth(91, &far.descriptor().source, &far, input())
            .unwrap();
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        let c = srgb(0.7152 * 0.5);
        // Green is behind blue, yet its independent luma still masks the
        // ordinary white receiver. Neither source alias nor group may overpaint.
        for (x, expected) in [(40, [255, c, c, 255]), (60, [c, c, 255, 255])] {
            assert!(
                ray(&pixels, x)
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "hidden={hidden} ray={x} actual={:?} expected={expected:?}",
                ray(&pixels, x)
            );
        }
        retain_fixture(
            &format!("ordinary-visible-calibrated-source-{hidden}.png"),
            &pixels,
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_receiver_projects_an_ordinary_parent_of_a_calibrated_matte() {
    let root = tempfile::tempdir().unwrap();
    let (mut p, resource) = source_project(root.path());
    let source = p.layers.last_mut().unwrap();
    source.transform.position.set_at(0, [40., 32., 0.]).unwrap();
    source.masks.push(calibrated_masks::right_source_half());
    let mut middle = Layer::solid(
        93,
        "Ordinary matte parent",
        [96., 64.],
        [48., 32., 0.],
        [1.; 4],
    );
    middle.visible = false;
    middle.track_matte = Some(TrackMatte {
        source: 91,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    p.layers[0].track_matte.as_mut().unwrap().source = 93;
    p.layers.push(middle);
    p.validate().unwrap();
    let near = DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    for inverted in [false, true] {
        p.layers.iter_mut().find(|l| l.id == 91).unwrap().masks[0].inverted = inverted;
        let mut scene = sampled(&p);
        scene
            .attach_calibrated_depth(1, &near.descriptor().source, &near, input())
            .unwrap();
        scene
            .attach_calibrated_depth(91, &resource.descriptor().source, &resource, input())
            .unwrap();
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        for (x, covered) in [(44, !inverted), (36, inverted)] {
            assert_eq!(
                ray(&pixels, x),
                if covered {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                },
                "inverted={inverted} ray={x}"
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn independent_matte_input_refuses_excess_draw_capacity_without_changing_front() {
    let root = tempfile::tempdir().unwrap();
    let (mut p, resource) = source_project(root.path());
    let mut source = p.layers.pop().unwrap();
    source.visible = true;
    let mut receiver = Layer::solid(
        90,
        "Ordinary capacity receiver",
        [96., 64.],
        [48., 32., 0.],
        [1.; 4],
    );
    receiver.track_matte = Some(TrackMatte {
        source: 91,
        mode: MatteMode::Alpha,
        hide_source: false,
    });
    p.layers = vec![source, receiver];
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let mut scene = sampled(&p);
    scene
        .attach_calibrated_depth(91, &resource.descriptor().source, &resource, input())
        .unwrap();
    let target = renderer.capture_target(96, 64).unwrap();
    let expected = renderer.capture(&scene, &target).unwrap().0;
    let mut full = scene.clone();
    for i in 0..motion_model::MAX_LAYERS - full.layers.len() {
        let mut layer = scene.layers.iter().find(|l| l.id == 90).unwrap().clone();
        layer.id = 1000 + i as u64;
        layer.order = full.layers.len();
        layer.composite_visible = false;
        layer.track_matte = None;
        full.layers.push(layer);
    }
    let error = renderer.capture(&full, &target).unwrap_err().to_string();
    assert!(
        error.contains("phase=input_alias") && error.contains("draw capacity"),
        "{error}"
    );
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, expected);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn ordinary_receiver_reads_independent_calibrated_matte_after_camera_edits() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let root = tempfile::tempdir().unwrap();
        let (mut p, resource) = source_project(root.path());
        p.working_precision = precision;
        p.layers[0].track_matte = None;
        let mut receiver = Layer::solid(
            90,
            "Ordinary white matte receiver",
            [96., 64.],
            [48., 32., 0.],
            [1.; 4],
        );
        receiver.track_matte = Some(TrackMatte {
            source: 91,
            mode: MatteMode::Alpha,
            hide_source: true,
        });
        p.layers.push(receiver);
        p.validate().unwrap();
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
                p.layers
                    .last_mut()
                    .unwrap()
                    .track_matte
                    .as_mut()
                    .unwrap()
                    .mode = mode;
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
                        // The ordinary receiver overlays the independent physical
                        // backdrop. Its matte is never the combined depth image.
                        let expected = if red_t < blue_t {
                            [255, srgb(c), srgb(c), 255]
                        } else {
                            [srgb(c), srgb(c), 255, 255]
                        };
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
                    &format!("ordinary-calibrated-matte-{precision:?}-{mode:?}-{moved}.png"),
                    &pixels,
                );
            }
        }
    }
}
