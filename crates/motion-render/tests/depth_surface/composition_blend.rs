//! Non-normal 2D receivers isolate, rather than disable, native depth runs.
use super::*;
use motion_model::compositing::{BlendMode, BlendSpace, LayerBlend};

fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
    &pixels[(y * 96 + x) * 4..][..4]
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unrelated_blend_preserves_depth_and_authored_order_in_both_color_spaces() {
    for space in [BlendSpace::Linear, BlendSpace::Srgb] {
        for front in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let mut p = project(root.path());
            let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
            prepare(&mut renderer, &p, root.path());
            let target = renderer.capture_target(96, 64).unwrap();
            let mut base = sampled(&p);
            attach(&mut base);
            let expected = renderer.capture(&base, &target).unwrap().0;
            // These independent opaque rays establish actual near/far occlusion.
            assert_eq!(pixel(&expected, 36, 32), &[255, 0, 0, 255]);
            assert_eq!(pixel(&expected, 60, 32), &[0, 0, 255, 255]);
            let mut overlay = Layer::solid(
                90,
                "Independent screen blend",
                [96., 64.],
                [48., 32., 0.],
                // Use an exactly specified byte coverage, avoiding a midpoint
                // rounding tie across native attachment implementations.
                [0., 1., 0., 128. / 255.],
            );
            overlay.blend = LayerBlend {
                mode: BlendMode::Screen,
                space,
            };
            if front {
                p.layers.push(overlay);
            } else {
                p.layers.insert(0, overlay);
            }
            p.validate().unwrap();
            let mut scene = sampled(&p);
            attach(&mut scene);
            let actual = renderer.capture(&scene, &target).unwrap().0;
            // Screen with byte coverage 128/255: green encodes near188 in
            // linear space or exactly128 in sRGB. Opaque red/blue stay intact.
            let green = if !front {
                0
            } else if space == BlendSpace::Linear {
                188
            } else {
                128
            };
            for (x, expected) in [(36, [255u8, green, 0, 255]), (60, [0, green, 255, 255])] {
                for (channel, (&actual, expected)) in
                    pixel(&actual, x, 32).iter().zip(expected).enumerate()
                {
                    assert!(actual.abs_diff(expected) <= 1,
                        "space={space:?} front={front} x={x} channel={channel}: actual={actual} expected={expected}");
                }
            }
            // The authored receiver must still appear outside opaque geometry.
            assert_eq!(pixel(&actual, 10, 10), &[0, 255, 0, 128]);
            retain_fixture(
                &format!("unrelated-blend-{space:?}-front-{front}.png"),
                &actual,
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_non_normal_blend_keeps_explicit_error_and_completed_front() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut base = sampled(&p);
    attach(&mut base);
    let expected = renderer.capture(&base, &target).unwrap().0;
    p.layers[0].blend.mode = BlendMode::Screen;
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
