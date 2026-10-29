//! Mask coverage follows the source UVs of calibrated, genuinely 3D geometry.
use super::*;
use motion_core::{
    masks::LayerMask,
    vector::{PathNode, VectorPath},
    Track,
};

pub(super) fn right_source_half() -> LayerMask {
    // Masks use the authored 64x40 layer source rectangle before projection;
    // the independently uploaded red image happens to contain only 2x1 texels.
    LayerMask::new(
        1,
        VectorPath {
            id: 1,
            closed: true,
            nodes: [[32., 0.], [64., 0.], [64., 40.], [32., 40.]]
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

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_masks_are_immutable_for_reverse_submission_and_budget_recovery() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    p.layers[0].masks.push(right_source_half());
    p.layers[0].masks[0].feather = Track::constant([2., 2.]);
    let mut a_scene = sampled(&p);
    attach(&mut a_scene);
    p.layers[0].masks[0].inverted = true;
    p.layers[0].masks[0].opacity = Track::constant(50.);
    let mut b_scene = sampled(&p);
    attach(&mut b_scene);
    let expected_a = renderer.capture(&a_scene, &a).unwrap().0;
    let expected_b = renderer.capture(&b_scene, &b).unwrap().0;
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
    // Queue writes retain their reservations until a submission completes.
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
    assert_eq!(budget.info().unwrap().admitted_bytes, used);
    assert_eq!(renderer.read_target(&b).unwrap(), expected_a);
}
fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
    &pixels[(y * 96 + x) * 4..][..4]
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_source_mask_reveals_real_depth_and_inversion_recovers_in_four_precisions() {
    use motion_effects::image_contract::ChannelPrecision::*;
    let root = tempfile::tempdir().unwrap();
    let authored = project(root.path());
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let mut p = authored.clone();
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
        let mut base = sampled(&p);
        attach(&mut base);
        let baseline = renderer.capture_sdr(&base, &target).unwrap().0.pixels;
        assert_eq!(pixel(&baseline, 36, 32), &[255, 0, 0, 255]);
        assert_eq!(pixel(&baseline, 60, 32), &[0, 0, 255, 255]);
        p.layers[0].masks.push(right_source_half());
        motion_core::storage::save(root.path(), &p).unwrap();
        p = motion_core::storage::load(root.path()).unwrap();
        let mut masked = sampled(&p);
        attach(&mut masked);
        let actual = renderer.capture_sdr(&masked, &target).unwrap().0.pixels;
        // Removing the near red source must reveal the spatial blue plane,
        // rather than cutting a hole in the already combined red/blue raster.
        assert_eq!(pixel(&actual, 36, 32), &[0, 0, 255, 255], "{precision:?}");
        assert_eq!(pixel(&actual, 60, 32), &[0, 0, 255, 255], "{precision:?}");
        assert_ne!(actual, baseline);
        retain_fixture(&format!("calibrated-mask-{precision:?}.png"), &actual);
        p.layers[0].masks[0].inverted = true;
        let mut inverted = sampled(&p);
        attach(&mut inverted);
        let actual = renderer.capture_sdr(&inverted, &target).unwrap().0.pixels;
        assert_eq!(pixel(&actual, 36, 32), &[255, 0, 0, 255], "{precision:?}");
        assert_eq!(pixel(&actual, 60, 32), &[0, 0, 255, 255], "{precision:?}");
        p.layers[0].masks[0].opacity = Track::constant(50.);
        let mut half = sampled(&p);
        attach(&mut half);
        let actual = renderer.capture_sdr(&half, &target).unwrap().0.pixels;
        let ray = pixel(&actual, 36, 32);
        // Half linear red over opaque blue, then SDR sRGB conversion. One
        // code point permits the declared UNORM8 intermediate quantization.
        assert!(
            ray[0].abs_diff(188) <= 1 && ray[1] == 0 && ray[2].abs_diff(188) <= 1 && ray[3] == 255,
            "half-coverage physical blend {precision:?}: {ray:?}"
        );
        p.layers[0].masks.clear();
        let mut restored = sampled(&p);
        attach(&mut restored);
        assert_eq!(
            renderer.capture_sdr(&restored, &target).unwrap().0.pixels,
            baseline
        );
    }
}
