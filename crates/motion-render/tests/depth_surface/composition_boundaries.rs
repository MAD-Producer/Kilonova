//! Unrelated 2D receivers remain independent of a calibrated spatial run.
use super::*;
use motion_core::{
    masks::LayerMask,
    vector::{PathNode, VectorPath},
    Track,
};
use motion_model::compositing::{MatteMode, TrackMatte};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn effected_calibrated_matte_source_for_ordinary_receiver_preserves_completed_front() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut base = sampled(&p);
    attach(&mut base);
    let expected = renderer.capture(&base, &target).unwrap().0;
    let mut top = overlay();
    top.track_matte = Some(TrackMatte {
        source: 1,
        mode: MatteMode::Alpha,
        hide_source: false,
    });
    p.layers.push(top);
    // Plain native calibrated inputs now have a dedicated pixel oracle. An
    // effected calibrated source still needs its own material/depth contract.
    let package = motion_effects::builtin::package().unwrap();
    let definition = package.manifest.effects.iter().find(|e| e.id == "fill").unwrap();
    p.layers[0].effects.push(motion_core::EffectInstance::new(1, &package.manifest.id,
        &package.manifest.version, &package.hash, definition, [64., 40.]));
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let mut scene = sampled(&p);
    attach(&mut scene);
    let error = renderer.capture(&scene, &target).unwrap_err().to_string();
    assert!(
        error.contains("object=1") && error.contains("depth composition path"),
        "{error}"
    );
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
}

fn overlay() -> Layer {
    Layer::solid(
        90,
        "Independent green overlay",
        [96., 64.],
        [48., 32., 0.],
        [0., 1., 0., 1.],
    )
}
fn mask() -> LayerMask {
    LayerMask::new(
        1,
        VectorPath {
            id: 1,
            closed: true,
            nodes: [[0., 0.], [24., 0.], [24., 20.], [0., 20.]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    )
}
fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
    &pixels[(y * 96 + x) * 4..][..4]
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unrelated_masked_2d_overlay_preserves_shared_depth_and_its_own_coverage() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut base = sampled(&p);
    attach(&mut base);
    let expected = renderer.capture(&base, &target).unwrap().0;
    let mut top = overlay();
    top.masks.push(mask());
    p.layers.push(top);
    p.validate().unwrap();
    let mut scene = sampled(&p);
    attach(&mut scene);
    let actual = renderer.capture(&scene, &target).unwrap().0;
    assert_eq!(pixel(&actual, 10, 10), &[0, 255, 0, 255]);
    for y in 22..62 {
        for x in 2..94 {
            assert_eq!(
                pixel(&actual, x, y),
                pixel(&expected, x, y),
                "outside mask at {x},{y}"
            );
        }
    }
    assert_ne!(
        actual, expected,
        "the visible masked overlay must really render"
    );
    retain_fixture("unrelated-2d-mask-and-depth.png", &actual);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unrelated_matte_2d_overlay_keeps_source_and_depth_rasters_independent() {
    for hide_source in [true, false] {
        for spatial_source in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let mut p = project(root.path());
            let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
            prepare(&mut renderer, &p, root.path());
            let target = renderer.capture_target(96, 64).unwrap();
            let mut base = sampled(&p);
            attach(&mut base);
            let expected = renderer.capture(&base, &target).unwrap().0;
            // Reference camera has Z=100 and vertical FOV=60 degrees. Project
            // either source onto the same 24x20 top-left rectangle analytically.
            let scale = if spatial_source {
                64. / (2. * (60_f32.to_radians() / 2.).tan()) / 100.
            } else {
                1.
            };
            let mut source = Layer::solid(
                91,
                "Independent matte source",
                [24. / scale, 20. / scale],
                [48. + (12. - 48.) / scale, 32. + (10. - 32.) / scale, 0.],
                [1.; 4],
            );
            source.three_d = spatial_source;
            source.visible = !hide_source;
            let mut top = overlay();
            top.track_matte = Some(TrackMatte {
                source: 91,
                mode: MatteMode::Alpha,
                hide_source,
            });
            // Project order is back-to-front. The consumer follows its source,
            // so a visible source cannot cover the intended green output.
            p.layers.push(source);
            p.layers.push(top);
            p.validate().unwrap();
            let mut scene = sampled(&p);
            attach(&mut scene);
            let actual = renderer.capture(&scene, &target).unwrap().0;
            assert_eq!(
                pixel(&actual, 10, 10),
                &[0, 255, 0, 255],
                "hidden={hide_source}, spatial={spatial_source}"
            );
            for y in 22..62 {
                for x in 2..94 {
                    assert_eq!(
                        pixel(&actual, x, y),
                        pixel(&expected, x, y),
                        "outside matte at {x},{y}, hidden={hide_source}, spatial={spatial_source}"
                    );
                }
            }
            assert_ne!(actual, expected);
        }
    }
}
