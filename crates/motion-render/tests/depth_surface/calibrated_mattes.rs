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
fn calibrated_source_for_calibrated_receiver_refuses_flat_materialization_and_keeps_front() {
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
