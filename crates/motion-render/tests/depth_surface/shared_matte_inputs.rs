//! Shared consumers read an independent image without losing source depth.
use super::spatial_matte_materials::authored;
use super::*;
use motion_model::compositing::{MatteMode, TrackMatte};
fn scene_for(p: &Project) -> Scene {
    let near = DepthResource::new(field(1).descriptor().clone(), vec![1., 1.], None, None).unwrap();
    let mut s = sampled(p);
    s.attach_calibrated_depth(1, &near.descriptor().source, &near, input())
        .unwrap();
    s
}
fn ray(p: &[u8], x: usize) -> &[u8] {
    &p[(32 * 96 + x) * 4..][..4]
}
fn srgb(c: f32) -> u8 {
    let c = if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    };
    (255. * c).round() as u8
}
fn receiver(mode: MatteMode) -> Layer {
    let mut l = Layer::solid(
        90,
        "Independent ordinary consumer",
        [96., 64.],
        [48., 32., 0.],
        [1.; 4],
    );
    l.track_matte = Some(TrackMatte {
        source: 91,
        mode,
        hide_source: false,
    });
    l
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_matte_image_feeds_ordinary_and_calibrated_receivers_in_four_precisions() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let root = tempfile::tempdir().unwrap();
        let mut p = authored(root.path());
        p.working_precision = precision;
        p.layers.push(receiver(MatteMode::Alpha));
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
        for mode in [
            MatteMode::Alpha,
            MatteMode::AlphaInverted,
            MatteMode::Luma,
            MatteMode::LumaInverted,
        ] {
            p.layers
                .iter_mut()
                .find(|l| l.id == 90)
                .unwrap()
                .track_matte
                .as_mut()
                .unwrap()
                .mode = mode;
            let pixels = renderer
                .capture_sdr(&scene_for(&p), &target)
                .unwrap()
                .0
                .pixels;
            let c = 0.5 * (0.2126 + 0.0722);
            let mut ordinary = if mode.luma() { c } else { 0.5 };
            if mode.inverted() {
                ordinary = 1. - ordinary;
            }
            let expected = [
                srgb(ordinary + c * (1. - ordinary)),
                srgb(ordinary),
                srgb(1. - c * (1. - ordinary)),
                255,
            ];
            assert!(
                ray(&pixels, 56)
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "{precision:?} {mode:?} actual={:?} expected={expected:?}",
                ray(&pixels, 56)
            );
            assert_eq!(
                ray(&pixels, 40),
                if mode.inverted() {
                    [255; 4]
                } else {
                    [0, 0, 255, 255]
                }
            );
            // Reorder only physical layers; the ordinary 2D overlay remains on top.
            p.layers[..3].reverse();
            assert_eq!(
                renderer
                    .capture_sdr(&scene_for(&p), &target)
                    .unwrap()
                    .0
                    .pixels,
                pixels
            );
            p.layers[..3].reverse();
            retain_fixture(
                &format!("shared-matte-image-{precision:?}-{mode:?}.png"),
                &pixels,
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_matte_image_ordinary_parent_and_authored_boundary_keep_independent_inputs() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let root = tempfile::tempdir().unwrap();
        let mut p = authored(root.path());
        p.working_precision = precision;
        let mut middle = Layer::solid(
            93,
            "Ordinary matte parent",
            [96., 64.],
            [48., 32., 0.],
            [1.; 4],
        );
        middle.visible = false;
        middle.transform.opacity.value = 0.5;
        middle.track_matte = Some(TrackMatte {
            source: 91,
            mode: MatteMode::Luma,
            hide_source: false,
        });
        p.layers[0].track_matte.as_mut().unwrap().source = 93;
        p.layers.insert(2, middle);
        p.layers.push(receiver(MatteMode::Alpha));
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            motion_render::AllocationBudget::new(128 << 20).unwrap(),
        ))
        .unwrap();
        prepare(&mut renderer, &p, root.path());
        let target = renderer
            .sdr_capture_target(96, 64, motion_render::OutputTransform::SDR_CLIP)
            .unwrap();
        let wide = renderer
            .sdr_capture_target(160, 96, motion_render::OutputTransform::SDR_CLIP)
            .unwrap();
        for boundary in [false, true] {
            if boundary {
                // A real 2D authored boundary separates physical runs even off screen.
                let separator = Layer::solid(
                    94,
                    "Authored 2D boundary",
                    [1.; 2],
                    [-100., -100., 0.],
                    [1.; 4],
                );
                p.layers.insert(3, separator);
            }
            p.validate().unwrap();
            let scene = scene_for(&p);
            let actual = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
            let c = 0.5 * (0.2126 + 0.0722);
            let red = 0.5 * c;
            let (red, blue) = if boundary {
                (0.5 + 0.5 * red, 0.5 + 0.5 * (1. - red))
            } else {
                (red, 1. - red)
            };
            let expected = [
                srgb(0.5 + 0.5 * red),
                srgb(0.5),
                srgb(0.5 + 0.5 * blue),
                255,
            ];
            assert!(
                ray(&actual, 56)
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "{precision:?} boundary={boundary} actual={:?} expected={expected:?}",
                ray(&actual, 56)
            );
            assert_eq!(ray(&actual, 40), [0, 0, 255, 255]);
            let letterboxed = renderer.capture_sdr(&scene, &wide).unwrap().0.pixels;
            assert!(
                letterboxed[(48 * 160 + 92) * 4..][..4]
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "{precision:?} boundary={boundary} letterbox={:?} expected={expected:?}",
                &letterboxed[(48 * 160 + 92) * 4..][..4]
            );
            retain_fixture(
                &format!("shared-matte-parent-{precision:?}-{boundary}.png"),
                &actual,
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_matte_image_max_id_pending_aliases_budget_and_abandonment_keep_owned_pixels() {
    let root = tempfile::tempdir().unwrap();
    let mut p = authored(root.path());
    p.layers.push(receiver(MatteMode::Alpha));
    p.layers.iter_mut().find(|l| l.id == 91).unwrap().id = u64::MAX;
    for l in &mut p.layers {
        if let Some(m) = &mut l.track_matte {
            if m.source == 91 {
                m.source = u64::MAX;
            }
        }
    }
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let first = scene_for(&p);
    let expected_a = renderer.capture(&first, &a).unwrap().0;
    let c = 0.5 * (0.2126 + 0.0722);
    let expected = [srgb(0.5 + 0.5 * c), srgb(0.5), srgb(1. - 0.5 * c), 255];
    assert!(ray(&expected_a, 56)
        .iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= 1));
    assert_eq!(ray(&expected_a, 40), [0, 0, 255, 255]);
    p.layers
        .iter_mut()
        .find(|l| l.id == u64::MAX)
        .unwrap()
        .masks[0]
        .inverted = true;
    let last = scene_for(&p);
    let expected_b = renderer.capture(&last, &b).unwrap().0;
    assert!(ray(&expected_b, 40)
        .iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= 1));
    assert_eq!(ray(&expected_b, 56), [0, 0, 255, 255]);
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
    let admitted = budget.info().unwrap().admitted_bytes;
    let mut abandoned = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&last, &a.view, 96, 64, &mut abandoned, None)
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
    assert_eq!(renderer.read_target(&a).unwrap(), expected_a);
    renderer.clear_assets();
    prepare(&mut renderer, &p, root.path());
    assert_eq!(renderer.capture(&first, &a).unwrap().0, expected_a);
    assert_eq!(renderer.capture(&last, &b).unwrap().0, expected_b);
}
