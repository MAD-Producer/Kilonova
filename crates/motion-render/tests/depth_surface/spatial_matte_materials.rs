//! Processed visible planar matte sources retain their own physical depth.
use super::*;
use motion_model::compositing::{MatteMode, TrackMatte};
fn ray(pixels: &[u8], x: usize) -> &[u8] {
    &pixels[(32 * 96 + x) * 4..][..4]
}
fn srgb(c: f32) -> u8 {
    let c = if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    };
    (255. * c).round() as u8
}
pub(super) fn authored(root: &std::path::Path) -> Project {
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
    let mut source = Layer::solid(
        91,
        "Far processed spatial matte",
        [64., 40.],
        [48., 32., 20.],
        [1.; 4],
    );
    source.content = Content::Image { asset: 8 };
    source.three_d = true;
    source.transform.opacity.value = 0.5;
    source.masks.push(calibrated_masks::right_source_half());
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "invert")
        .unwrap();
    let mut effect = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64., 40.],
    );
    effect.params.get_mut("channel").unwrap().track.value = [1., 0., 0., 0.];
    source.effects.push(effect);
    p.layers.push(source);
    p.layers[0].track_matte = Some(TrackMatte {
        source: 91,
        mode: MatteMode::Luma,
        hide_source: false,
    });
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    p
}

fn scene_for(p: &Project) -> Scene {
    let near = DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
    let mut scene = sampled(p);
    scene
        .attach_calibrated_depth(1, &near.descriptor().source, &near, input())
        .unwrap();
    scene
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn spatial_matte_material_stays_behind_physical_backdrop_and_projects_independent_luma() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let root = tempfile::tempdir().unwrap();
        let mut p = authored(root.path());
        p.working_precision = precision;
        let near =
            DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
        let sample = |p: &Project| {
            let mut s = sampled(p);
            s.attach_calibrated_depth(1, &near.descriptor().source, &near, input())
                .unwrap();
            s
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
        let actual = renderer.capture_sdr(&sample(&p), &target).unwrap().0.pixels;
        assert_eq!(ray(&actual, 40), [0, 0, 255, 255]);
        // Independent magenta source luminance after its half opacity. The
        // visible source is behind opaque blue, so it cannot overpaint it.
        let c = 0.5 * (0.2126 + 0.0722);
        let expected = [srgb(c), 0, srgb(1. - c), 255];
        assert!(
            ray(&actual, 56)
                .iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "{precision:?} actual={:?} expected={expected:?}",
            ray(&actual, 56)
        );
        p.layers.reverse();
        assert_eq!(
            renderer.capture_sdr(&sample(&p), &target).unwrap().0.pixels,
            actual,
            "physical source depth must not depend on authored order"
        );
        retain_fixture(
            &format!("spatial-matte-material-{precision:?}.png"),
            &actual,
        );
        p.layers
            .iter_mut()
            .find(|l| l.id == 91)
            .unwrap()
            .transform
            .position
            .set_at(0, [48., 32., -40.])
            .unwrap();
        let front = renderer.capture_sdr(&sample(&p), &target).unwrap().0.pixels;
        let expected = [srgb(0.5 + 0.5 * c), 0, srgb(0.5 + 0.5 * (1. - c)), 255];
        assert!(
            ray(&front, 56)
                .iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "{precision:?} front actual={:?} expected={expected:?}",
            ray(&front, 56)
        );
        assert_eq!(ray(&front, 40), [0, 0, 255, 255]);
        p.layers.reverse();
        assert_eq!(
            renderer.capture_sdr(&sample(&p), &target).unwrap().0.pixels,
            front
        );
        retain_fixture(
            &format!("spatial-matte-material-front-{precision:?}.png"),
            &front,
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn spatial_matte_material_pending_revisions_budget_and_eviction_preserve_front() {
    let root = tempfile::tempdir().unwrap();
    let mut p = authored(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let first = scene_for(&p);
    let expected_a = renderer.capture(&first, &a).unwrap().0;
    p.layers.iter_mut().find(|l| l.id == 91).unwrap().effects[0]
        .params
        .get_mut("blend")
        .unwrap()
        .track
        .value = [100., 0., 0., 0.];
    let last = scene_for(&p);
    let expected_b = renderer.capture(&last, &b).unwrap().0;
    assert_ne!(ray(&expected_a, 56), ray(&expected_b, 56));
    let c = 0.5 * 0.7152;
    let expected = [srgb(c), 0, srgb(1. - c), 255];
    assert!(ray(&expected_b, 56)
        .iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= 1));
    let mut old = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&first, &a.view, 96, 64, &mut old, None)
        .unwrap();
    let mut new = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&last, &b.view, 96, 64, &mut new, None)
        .unwrap();
    renderer.submit_allocated(new.finish(), |_| {}).unwrap();
    renderer.submit_allocated(old.finish(), |_| {}).unwrap();
    assert_eq!(renderer.read_target(&a).unwrap(), expected_a);
    assert_eq!(renderer.read_target(&b).unwrap(), expected_b);
    let budget = renderer.allocation_budget().clone();
    let info = budget.info().unwrap();
    budget.set_limit(info.admitted_bytes).unwrap();
    let error = renderer.capture(&first, &b).unwrap_err();
    assert!(error.resource_rejection().is_some(), "{error}");
    budget.set_limit(info.budget_bytes).unwrap();
    assert_eq!(renderer.read_target(&b).unwrap(), expected_b);
    assert_eq!(renderer.capture(&first, &b).unwrap().0, expected_a);
    renderer.clear_assets();
    prepare(&mut renderer, &p, root.path());
    assert_eq!(renderer.capture(&last, &a).unwrap().0, expected_b);
    assert_eq!(renderer.capture(&first, &a).unwrap().0, expected_a);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn spatial_matte_material_mask_only_and_unproven_or_external_inputs_keep_boundaries() {
    let root = tempfile::tempdir().unwrap();
    let mut p = authored(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let effect = p
        .layers
        .iter_mut()
        .find(|l| l.id == 91)
        .unwrap()
        .effects
        .remove(0);
    p.rebuild_plugin_dependencies();
    let baseline = renderer.capture(&scene_for(&p), &target).unwrap().0;
    let c = 0.5 * 0.7152;
    let expected = [srgb(c), 0, srgb(1. - c), 255];
    assert!(ray(&baseline, 56)
        .iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= 1));
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "fill")
        .unwrap();
    p.layers
        .iter_mut()
        .find(|l| l.id == 91)
        .unwrap()
        .effects
        .push(motion_core::EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [64., 40.],
        ));
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let error = renderer
        .capture(&scene_for(&p), &target)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("object=91")
            && error.contains("source=8")
            && error.contains("depth composition path"),
        "{error}"
    );
    assert_eq!(renderer.read_target(&target).unwrap(), baseline);
    let source = p.layers.iter_mut().find(|l| l.id == 91).unwrap();
    source.effects.clear();
    source.effects.push(effect);
    p.rebuild_plugin_dependencies();
    let mut ordinary = Layer::solid(
        90,
        "Independent ordinary receiver",
        [96., 64.],
        [48., 32., 0.],
        [1.; 4],
    );
    ordinary.track_matte = Some(TrackMatte {
        source: 91,
        mode: MatteMode::Alpha,
        hide_source: false,
    });
    // Shared ordinary matte consumers now use an independent projected alias.
    // Keep the original refusal/front assertions for still-unsupported explicit
    // effect image reads instead of rejecting the newly supported consumer.
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "displacement_map")
        .unwrap();
    let mut external = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [96., 64.],
    );
    external.image_input = Some(motion_model::EffectImageInput::Layer {
        layer: 91,
        stage: motion_model::EffectImageStage::Effects,
    });
    ordinary.effects.push(external);
    p.layers.push(ordinary);
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let error = renderer
        .capture(&scene_for(&p), &target)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("object=1")
            && error.contains("source=7")
            && error.contains("matte source=91")
            && error.contains("depth composition path"),
        "{error}"
    );
    assert_eq!(renderer.read_target(&target).unwrap(), baseline);
    p.layers.pop();
    p.layers
        .iter_mut()
        .find(|l| l.id == 91)
        .unwrap()
        .effects
        .clear();
    p.rebuild_plugin_dependencies();
    assert_eq!(
        renderer.capture(&scene_for(&p), &target).unwrap().0,
        baseline
    );
}
