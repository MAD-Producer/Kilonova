//! Pointwise source materials keep actual calibrated geometry and depth.
use super::*;
fn material(id: &str) -> motion_core::EffectInstance {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == id)
        .unwrap();
    let mut instance = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64., 40.],
    );
    if id == "invert" {
        instance.params.get_mut("channel").unwrap().track.value = [1., 0., 0., 0.];
    }
    if id == "tint" {
        for id in ["p0001", "p0002"] {
            instance.params.get_mut(id).unwrap().track.value = [0., 1., 0., 1.];
        }
        instance.params.get_mut("p0003").unwrap().track.value = [100., 0., 0., 0.];
    }
    instance
}

fn ray(pixels: &[u8], x: usize) -> &[u8] {
    &pixels[(32 * 96 + x) * 4..][..4]
}
fn srgb(c: f32) -> u8 {
    let s = if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    };
    (s * 255.).round() as u8
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_material_masks_precede_effects_and_projected_matte_and_opacity_follow() {
    use motion_effects::image_contract::ChannelPrecision::*;
    use motion_model::compositing::{MatteMode, TrackMatte};
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let root = tempfile::tempdir().unwrap();
        let mut p = project(root.path());
        p.working_precision = precision;
        p.layers[0]
            .masks
            .push(calibrated_masks::right_source_half());
        p.layers[0].effects.push(material("invert"));
        p.rebuild_plugin_dependencies();
        p.validate().unwrap();
        let near =
            DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
        let sample = |p: &Project| {
            let mut scene = sampled(p);
            scene
                .attach_calibrated_depth(1, &near.descriptor().source, &near, input())
                .unwrap();
            scene
        };
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            motion_render::AllocationBudget::new(128 << 20).unwrap(),
        ))
        .unwrap();
        prepare(&mut renderer, &p, root.path());
        let target = renderer
            .sdr_capture_target(96, 64, motion_render::OutputTransform::SDR_CLIP)
            .unwrap();
        let pixels = renderer.capture_sdr(&sample(&p), &target).unwrap().0.pixels;
        assert_eq!(ray(&pixels, 40), [0, 0, 255, 255]);
        assert_eq!(ray(&pixels, 56), [0, 255, 255, 255]);
        // Alpha inversion fills the masked hole with black and removes the
        // previously opaque right half. Applying a mask again would be wrong.
        p.layers[0].effects[0]
            .params
            .get_mut("channel")
            .unwrap()
            .track
            .value = [5., 0., 0., 0.];
        let inverted = renderer.capture_sdr(&sample(&p), &target).unwrap().0.pixels;
        assert_eq!(ray(&inverted, 40), [0, 0, 0, 255]);
        assert_eq!(ray(&inverted, 56), [0, 0, 255, 255]);
        p.layers[0].effects[0]
            .params
            .get_mut("channel")
            .unwrap()
            .track
            .value = [1., 0., 0., 0.];
        p.layers[0].transform.opacity.value = 0.5;
        let mut matte = Layer::solid(
            91,
            "Independent half-alpha luma",
            [96., 64.],
            [48., 32., 0.],
            [0., 1., 0., 1.],
        );
        matte.transform.opacity.value = 0.5;
        matte.visible = false;
        p.layers.push(matte);
        p.layers[0].track_matte = Some(TrackMatte {
            source: 91,
            mode: MatteMode::Luma,
            hide_source: true,
        });
        p.validate().unwrap();
        let pixels = renderer.capture_sdr(&sample(&p), &target).unwrap().0.pixels;
        assert_eq!(ray(&pixels, 40), [0, 0, 255, 255]);
        let expected = [0, srgb(0.5 * 0.5 * 0.7152), 255, 255];
        assert!(
            ray(&pixels, 56)
                .iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "{precision:?} actual={:?} expected={expected:?}",
            ray(&pixels, 56)
        );
        retain_fixture(
            &format!("pointwise-mask-opacity-matte-{precision:?}.png"),
            &pixels,
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn pointwise_material_pending_revisions_admission_and_eviction_keep_depth_front() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    p.layers[0].effects.push(material("invert"));
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let sample = |p: &Project| {
        let mut s = sampled(p);
        attach(&mut s);
        s
    };
    let a_scene = sample(&p);
    p.layers[0].effects[0]
        .params
        .get_mut("blend")
        .unwrap()
        .track
        .value = [100., 0., 0., 0.];
    let b_scene = sample(&p);
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let expected_a = renderer.capture(&a_scene, &a).unwrap().0;
    let expected_b = renderer.capture(&b_scene, &b).unwrap().0;
    assert_eq!(ray(&expected_a, 40), [0, 255, 255, 255]);
    assert_eq!(ray(&expected_a, 60), [0, 0, 255, 255]);
    check_oracle(&b_scene, &expected_b);
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
fn effected_calibrated_matte_projects_processed_source_color_for_both_receiver_kinds() {
    use motion_model::compositing::{MatteMode, TrackMatte};
    for ordinary in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (mut p, source_depth) = calibrated_matte_sources::source_project(root.path());
        let source = p.layers.iter_mut().find(|l| l.id == 91).unwrap();
        source.transform.position.set_at(0, [40., 32., 0.]).unwrap();
        source.transform.opacity.value = 0.5;
        source.masks.push(calibrated_masks::right_source_half());
        source.effects.push(material("invert"));
        if ordinary {
            p.layers[0].track_matte = None;
            let mut receiver = Layer::solid(
                90,
                "Ordinary processed matte receiver",
                [96., 64.],
                [48., 32., 0.],
                [1.; 4],
            );
            receiver.track_matte = Some(TrackMatte {
                source: 91,
                mode: MatteMode::Luma,
                hide_source: true,
            });
            p.layers.push(receiver);
        } else {
            p.layers[0].track_matte.as_mut().unwrap().mode = MatteMode::Luma;
        }
        p.rebuild_plugin_dependencies();
        p.validate().unwrap();
        let near =
            DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
        let mut scene = sampled(&p);
        scene
            .attach_calibrated_depth(1, &near.descriptor().source, &near, input())
            .unwrap();
        scene
            .attach_calibrated_depth(
                91,
                &source_depth.descriptor().source,
                &source_depth,
                input(),
            )
            .unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        prepare(&mut renderer, &p, root.path());
        let target = renderer.capture_target(96, 64).unwrap();
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        // Green source -> magenta material, then half opacity. Matte luma is
        // independent of both receiver kind and the physical red/blue backdrop.
        let c = 0.5 * (0.2126 + 0.0722);
        let expected = if ordinary {
            [255, srgb(c), srgb(c), 255]
        } else {
            [srgb(c), 0, srgb(1. - c), 255]
        };
        assert!(
            ray(&pixels, 44)
                .iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "ordinary={ordinary} actual={:?} expected={expected:?}",
            ray(&pixels, 44)
        );
        assert_eq!(
            ray(&pixels, 36),
            if ordinary {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        );
        retain_fixture(
            &format!("processed-calibrated-source-ordinary-{ordinary}.png"),
            &pixels,
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn owned_video_material_keeps_exact_display_revision_and_pending_depth() {
    let root = tempfile::tempdir().unwrap();
    let mut p = video::video_project(root.path());
    p.layers[0].effects.push(material("invert"));
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let resource = field(1);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&p, root.path()).unwrap();
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let mut samples = Vec::new();
    for (color, expected, target) in [
        ([255, 0, 0, 255], [0, 255, 255, 255], &a),
        ([0, 255, 0, 255], [255, 0, 255, 255], &b),
    ] {
        let mut scene = sampled(&p);
        scene
            .attach_calibrated_depth_image(
                1,
                &resource.descriptor().source,
                &resource,
                input(),
                motion_render::DepthImage::new([2, 1], color.repeat(2)).unwrap(),
            )
            .unwrap();
        assert!(scene.video_decode_layers().is_empty());
        assert!(renderer
            .prepare_scene_assets(
                &scene,
                motion_render::image_resources::Resolution::Full,
                false
            )
            .unwrap());
        let pixels = renderer.capture(&scene, target).unwrap().0;
        assert_eq!(ray(&pixels, 40), expected);
        assert_eq!(ray(&pixels, 60), [0, 0, 255, 255]);
        samples.push((scene, pixels));
    }
    assert!(renderer
        .prepare_scene_assets(
            &samples[0].0,
            motion_render::image_resources::Resolution::Full,
            false
        )
        .unwrap());
    let mut first = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&samples[0].0, &a.view, 96, 64, &mut first, None)
        .unwrap();
    assert!(renderer
        .prepare_scene_assets(
            &samples[1].0,
            motion_render::image_resources::Resolution::Full,
            false
        )
        .unwrap());
    let mut last = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&samples[1].0, &b.view, 96, 64, &mut last, None)
        .unwrap();
    renderer.submit_allocated(last.finish(), |_| {}).unwrap();
    renderer.submit_allocated(first.finish(), |_| {}).unwrap();
    assert_eq!(renderer.read_target(&a).unwrap(), samples[0].1);
    assert_eq!(renderer.read_target(&b).unwrap(), samples[1].1);
    let mut absent = sampled(&p);
    absent
        .attach_calibrated_depth(1, &resource.descriptor().source, &resource, input())
        .unwrap();
    let error = renderer.capture(&absent, &b).unwrap_err().to_string();
    assert!(
        error.contains("object=1") && error.contains("source=7"),
        "{error}"
    );
    assert_eq!(renderer.read_target(&b).unwrap(), samples[1].1);
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_pointwise_materials_keep_geometry_and_opaque_depth_in_four_precisions() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let root = tempfile::tempdir().unwrap();
        let mut p = project(root.path());
        p.working_precision = precision;
        p.layers[0].effects.push(material("invert"));
        p.rebuild_plugin_dependencies();
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
        for tinted in [false, true] {
            p.camera.position.set_at(0, [48., 32., -100.]).unwrap();
            if tinted {
                let mut tint = material("tint");
                tint.id = 2;
                p.layers[0].effects.push(tint);
                p.rebuild_plugin_dependencies();
            }
            for moved in [false, true] {
                if moved {
                    p.camera.position.set_at(0, [60., 29., -100.]).unwrap();
                }
                let mut scene = sampled(&p);
                attach(&mut scene);
                let pixels = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
                check_color_geometry(
                    &scene,
                    &pixels,
                    if tinted {
                        [0, 255, 0, 255]
                    } else {
                        [0, 255, 255, 255]
                    },
                );
                p.layers.reverse();
                let mut reversed = sampled(&p);
                attach(&mut reversed);
                assert_eq!(
                    renderer.capture_sdr(&reversed, &target).unwrap().0.pixels,
                    pixels,
                    "pointwise opaque depth cannot depend on authored order"
                );
                p.layers.reverse();
                retain_fixture(
                    &format!("pointwise-material-{precision:?}-{tinted}-{moved}.png"),
                    &pixels,
                );
            }
        }
    }
}
fn check_color_geometry(scene: &Scene, pixels: &[u8], color: [u8; 4]) {
    let image = scene.layers.iter().find(|l| l.id == 1).unwrap();
    let blue = scene.layers.iter().find(|l| l.id == 2).unwrap();
    let inverse = scene.camera.view_projection.as_dmat4().inverse();
    let blue_inverse = blue.model.as_dmat4().inverse();
    let mut checked = [0usize; 2];
    for y in 0..64usize {
        for x in 0..96usize {
            let ndc = [(x as f64 + 0.5) / 48. - 1., 1. - (y as f64 + 0.5) / 32.];
            let a = inverse * DVec4::new(ndc[0], ndc[1], 0., 1.);
            let b = inverse * DVec4::new(ndc[0], ndc[1], 0.5, 1.);
            let origin = a.truncate() / a.w;
            let direction = (b.truncate() / b.w - origin).normalize();
            let authored_surface = image.model.as_dmat4()
                * glam::DMat4::from_scale(DVec3::new(
                    f64::from(image.size[0]) / 64.,
                    f64::from(image.size[1]) / 40.,
                    1.,
                ));
            let Some(surface) = surface_hit(origin, direction, authored_surface) else {
                continue;
            };
            let o = blue_inverse.transform_point3(origin);
            let d = blue_inverse.transform_vector3(direction);
            if d.z.abs() < 1e-12 {
                continue;
            }
            let plane = -o.z / d.z;
            let q = o + d * plane;
            if plane <= 0. || q.x.abs() > 30. || q.y.abs() > 18. || (surface - plane).abs() < 2. {
                continue;
            }
            let nearer = usize::from(surface < plane);
            let expected = if nearer == 1 { color } else { [0, 0, 255, 255] };
            assert_eq!(
                &pixels[(y * 96 + x) * 4..][..4],
                &expected,
                "ray x={x} y={y} surface={surface} plane={plane}"
            );
            checked[nearer] += 1;
        }
    }
    assert!(
        checked.iter().all(|n| *n >= 30),
        "both physical surfaces require interior coverage: {checked:?}"
    );
}
