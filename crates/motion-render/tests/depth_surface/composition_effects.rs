//! Independent image receivers and their input DAG stay outside native runs.
use super::*;
use motion_core::{EffectImageInput, EffectImageStage, EffectInstance};

fn effect(id: &str) -> EffectInstance {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == id)
        .unwrap();
    EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [24., 20.],
    )
}
fn receiver() -> Layer {
    Layer::solid(
        90,
        "Independent image receiver",
        [24., 20.],
        [12., 10., 0.],
        [1.; 4],
    )
}
fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
    &pixels[(y * 96 + x) * 4..][..4]
}
fn check(actual: &[u8], baseline: &[u8], interior: [u8; 4]) {
    assert_eq!(pixel(actual, 10, 10), &interior);
    for y in 22..62 {
        for x in 2..94 {
            assert_eq!(
                pixel(actual, x, y),
                pixel(baseline, x, y),
                "outside receiver at {x},{y}"
            );
        }
    }
    assert_ne!(actual, baseline);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unrelated_image_effect_keeps_its_pixels_without_disabling_calibrated_depth() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut base = sampled(&p);
    attach(&mut base);
    let baseline = renderer.capture(&base, &target).unwrap().0;
    let mut top = receiver();
    let mut fill = effect("fill");
    fill.params
        .values_mut()
        .find(|p| p.kind == motion_effects::ParamKind::Color)
        .unwrap()
        .track
        .value = [0., 1., 0., 1.];
    top.effects.push(fill);
    p.layers.push(top);
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let mut scene = sampled(&p);
    attach(&mut scene);
    let actual = renderer.capture(&scene, &target).unwrap().0;
    check(&actual, &baseline, [0, 255, 0, 255]);
    retain_fixture("unrelated-image-fill-and-depth.png", &actual);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unrelated_external_image_inputs_keep_source_and_effects_stages_independent() {
    for stage in [EffectImageStage::Source, EffectImageStage::Effects] {
        for visible in [false, true] {
            for spatial in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let mut p = project(root.path());
                let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
                prepare(&mut renderer, &p, root.path());
                let target = renderer.capture_target(96, 64).unwrap();
                let mut base = sampled(&p);
                attach(&mut base);
                let baseline = renderer.capture(&base, &target).unwrap().0;
                let scale = if spatial {
                    64. / (2. * 30_f32.to_radians().tan()) / 100.
                } else {
                    1.
                };
                let mut source = Layer::solid(
                    91,
                    "Independent blue source",
                    [24. / scale, 20. / scale],
                    [48. + (12. - 48.) / scale, 32. + (10. - 32.) / scale, 0.],
                    [0., 0., 1., 1.],
                );
                source.visible = visible;
                source.three_d = spatial;
                let mut invert = effect("displacement_map");
                invert.params.get_mut("amount").unwrap().track.value = [8., 0., 0., 0.];
                invert.image_input = Some(EffectImageInput::Layer { layer: 91, stage });
                let mut top = receiver();
                top.content = Content::Solid {
                    color: [0., 1., 0., 1.],
                };
                top.effects.push(invert);
                p.layers.push(source);
                p.layers.push(top);
                p.rebuild_plugin_dependencies();
                p.validate().unwrap();
                let mut scene = sampled(&p);
                attach(&mut scene);
                let actual = renderer.capture(&scene, &target).unwrap().0;
                // Blue has red=0: (0 - 0.5) * 8 samples four pixels left,
                // moving green coverage four pixels right within the declared
                // unchanged receiver bounds (the manifest has zero padding).
                // Neutral fallback cannot reveal its backdrop at x=2.
                check(&actual, &baseline, [0, 255, 0, 255]);
                let uncovered = if visible { [0, 0, 255, 255] } else { [0; 4] };
                assert_eq!(
                    pixel(&actual, 2, 10),
                    &uncovered,
                    "stage={stage:?} visible={visible} spatial={spatial}"
                );
                assert_eq!(pixel(&actual, 26, 10), &[0; 4], "unchanged receiver bounds");
                retain_fixture(
                    &format!("external-input-{stage:?}-{visible}-{spatial}.png"),
                    &actual,
                );
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unproven_calibrated_material_or_external_input_retains_explicit_rejection() {
    for as_input in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut p = project(root.path());
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        prepare(&mut renderer, &p, root.path());
        let target = renderer.capture_target(96, 64).unwrap();
        let mut base = sampled(&p);
        attach(&mut base);
        let baseline = renderer.capture(&base, &target).unwrap().0;
        let mut invert = effect(if as_input {
            "displacement_map"
        } else {
            // Proven pointwise Invert/Tint now have a native source-material
            // contract. Fill is deliberately outside that checked contract.
            "fill"
        });
        if as_input {
            invert.image_input = Some(EffectImageInput::Layer {
                layer: 1,
                stage: EffectImageStage::Effects,
            });
            let mut top = receiver();
            top.effects.push(invert);
            p.layers.push(top);
        } else {
            p.layers[0].effects.push(invert);
        }
        p.rebuild_plugin_dependencies();
        p.validate().unwrap();
        let mut scene = sampled(&p);
        attach(&mut scene);
        let error = renderer.capture(&scene, &target).unwrap_err().to_string();
        assert!(
            error.contains("object=1") && error.contains("depth composition path"),
            "{error}"
        );
        assert_eq!(renderer.read_target(&target).unwrap(), baseline);
    }
}
