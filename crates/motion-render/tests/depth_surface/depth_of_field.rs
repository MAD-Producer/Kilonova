//! Saved camera aperture must use the displaced physical surface, not its flat
//! source plane or uncalibrated relative samples.
use super::*;
use motion_model::{DepthOfField, Track};

fn camera(focus: f32) -> DepthOfField {
    DepthOfField {
        focus_distance: Track::constant(focus),
        aperture_diameter: Track::constant(30.),
        max_radius_pixels: 8.,
    }
}
fn with_depth(p: &Project, frame: f64) -> Scene {
    let mut scene = Scene::new(p).unwrap();
    scene
        .sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    attach(&mut scene);
    scene
}
fn differences(a: &[u8], b: &[u8], left: bool) -> usize {
    (0..64)
        .flat_map(|y| (0..96).map(move |x| (x, y)))
        .filter(|&(x, _)| (x < 48) == left)
        .filter(|&(x, y)| a[(y * 96 + x) * 4 + 3].abs_diff(b[(y * 96 + x) * 4 + 3]) > 12)
        .count()
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn saved_animated_aperture_focuses_calibrated_near_and_far_surface_in_four_precisions() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let root = tempfile::tempdir().unwrap();
        let mut p = project(root.path());
        p.layers.retain(|layer| layer.id == 1);
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
        let sharp = renderer
            .capture_sdr(&with_depth(&p, 0.), &target)
            .unwrap()
            .0
            .pixels;
        let mut dof = camera(80.);
        dof.focus_distance.set_animated(0, true).unwrap();
        dof.focus_distance.set_at(30, 120.).unwrap();
        p.camera.depth_of_field = Some(dof);
        p.validate().unwrap();
        let encoded = serde_json::to_vec(&p).unwrap();
        let frozen: Project = serde_json::from_slice(&encoded).unwrap();
        let near = renderer
            .capture_sdr(&with_depth(&frozen, 0.), &target)
            .unwrap()
            .0
            .pixels;
        let far = renderer
            .capture_sdr(&with_depth(&frozen, 30.), &target)
            .unwrap()
            .0
            .pixels;
        assert!(
            differences(&far, &sharp, true) > differences(&near, &sharp, true) + 20,
            "{precision:?}: the 80-world-unit half must sharpen at near focus"
        );
        assert!(
            differences(&near, &sharp, false) > differences(&far, &sharp, false) + 20,
            "{precision:?}: the 120-world-unit half must sharpen at far focus"
        );
        retain_fixture(
            &format!("camera-aperture-{precision:?}-focus-near.png"),
            &near,
        );
        retain_fixture(
            &format!("camera-aperture-{precision:?}-focus-far.png"),
            &far,
        );
        // An ordinary authored 2D item still paints sharply after the spatial run.
        let overlay = Layer::solid(
            90,
            "Crisp authored overlay",
            [8., 8.],
            [48., 32., 0.],
            [0., 1., 0., 1.],
        );
        p.layers.push(overlay);
        let over = renderer
            .capture_sdr(&with_depth(&p, 0.), &target)
            .unwrap()
            .0
            .pixels;
        assert_eq!(&over[(32 * 96 + 48) * 4..][..4], &[0, 255, 0, 255]);
    }
}
