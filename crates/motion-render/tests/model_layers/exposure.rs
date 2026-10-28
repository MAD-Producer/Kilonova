//! Regression coverage for native model sources in retained temporal receivers.
use super::*;
use motion_core::{render_sampling::Shutter, Ease, ExpressionEvaluator};
use motion_render::{
    exposure_scene::{ExposureDocument, ExposureScenes},
    ExposureFrame, ExposurePoll, LayerRaster,
};
use std::{sync::Arc, time::Duration};

fn sample(project: &Project, frame: f64) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, frame, None, &ExpressionEvaluator)
        .unwrap();
    scene
}

fn retained_pixels(renderer: &mut Renderer, scene: &Scene, raster: &LayerRaster) -> Vec<u8> {
    let mut exposure = renderer.exposure_target(96, 64, &[1.]).unwrap();
    renderer
        .accumulate_layer_exposure(&mut exposure, scene, &[raster])
        .unwrap();
    renderer.capture_exposure(&exposure).unwrap()
}

fn pixels_within_one(actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len());
    let maximum = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(maximum <= 1, "maximum native pixel difference={maximum}");
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cold_model_receivers_materialize_each_pose_and_keep_earlier_snapshots() {
    // Every receiver starts before any ordinary draw; a warm model image must
    // not conceal the absent preparation or supply an earlier pose by accident.
    for kind in 0..4 {
        let root = tempfile::tempdir().unwrap();
        let authored = match kind {
            0 => project(root.path()),
            1 => animated_project_features(root.path(), false, false),
            2 => animated_project_features(root.path(), true, false),
            _ => animated_project_features(root.path(), false, true),
        };
        motion_core::storage::save(root.path(), &authored).unwrap();
        let restored = motion_core::storage::load(root.path()).unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        renderer.configure_assets(&restored, root.path()).unwrap();
        std::fs::remove_file(root.path().join(&restored.model_assets[0].path)).unwrap();
        let first_scene = sample(&restored, 0.);
        renderer.retain_video_instances(&first_scene);
        let (first, first_stats) = renderer
            .materialize_layer(&first_scene, 1, 96, 64, false)
            .unwrap()
            .expect("visible model must have a retained receiver");
        let last_scene = sample(&restored, 60.);
        renderer.retain_video_instances(&last_scene);
        let (last, last_stats) = renderer
            .materialize_layer(&last_scene, 1, 96, 64, false)
            .unwrap()
            .expect("later pose must have a retained receiver");
        assert!(first_stats.draw_calls > 0 && last_stats.draw_calls > 0);
        assert_eq!(first.frame(), 0.);
        assert_eq!(last.frame(), 60.);
        let expected_first = render(&mut renderer, &restored, 0.);
        let expected_last = render(&mut renderer, &restored, 60.);
        let actual_first = retained_pixels(&mut renderer, &first_scene, &first);
        let actual_last = retained_pixels(&mut renderer, &last_scene, &last);
        pixels_within_one(&actual_first, &expected_first);
        pixels_within_one(&actual_last, &expected_last);
        let expected_motion = if kind == 0 { 30. } else { 40. };
        assert!(
            (center(&actual_last) - center(&actual_first) - expected_motion).abs() < 0.25,
            "retained receiver must use its own sampled native pose kind={kind}"
        );
        assert_eq!(restored, authored, "sampling must preserve saved authoring");
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn model_receiver_shutter_matches_independent_native_sample_average() {
    let root = tempfile::tempdir().unwrap();
    let mut authored = project(root.path());
    let layer = &mut authored.layers[0];
    layer.transform.position = motion_core::Track::constant([20., 32., 0.]);
    layer
        .transform
        .position
        .upsert(0, [20., 32., 0.], Ease::Linear)
        .unwrap();
    layer
        .transform
        .position
        .upsert(2, [70., 32., 0.], Ease::Linear)
        .unwrap();
    layer.sampling.motion_blur = Some(Shutter {
        frames: 2.,
        samples: 4,
    });
    authored.validate().unwrap();
    motion_core::storage::save(root.path(), &authored).unwrap();
    let restored = motion_core::storage::load(root.path()).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&restored, root.path()).unwrap();
    std::fs::remove_file(root.path().join(&restored.model_assets[0].path)).unwrap();
    let document = ExposureDocument::new(Arc::new(restored.clone())).unwrap();
    let scenes = ExposureScenes::new(
        document,
        &restored.composition_id,
        1.,
        false,
        None,
        ExpressionEvaluator,
    )
    .unwrap();
    let mut task = ExposureFrame::new(&mut renderer, scenes, 96, 64).unwrap();
    let mut ready = false;
    for _ in 0..16 {
        if matches!(
            task.poll(&mut renderer, Duration::from_secs(1), |r, scene| {
                r.retain_video_instances(scene);
                Ok(true)
            })
            .unwrap(),
            ExposurePoll::Ready(_)
        ) {
            ready = true;
            break;
        }
    }
    assert!(ready, "finite receiver exposure must complete");
    let actual = renderer.capture_exposure(task.target()).unwrap();
    let samples: Vec<_> = [0.25, 0.75, 1.25, 1.75]
        .into_iter()
        .map(|frame| render(&mut renderer, &restored, frame))
        .collect();
    let expected: Vec<_> = (0..actual.len())
        .map(|i| {
            // Screenshot readback is straight-alpha sRGB. Independently apply
            // the defined linear premultiplied mean before converting back.
            let base = i / 4 * 4;
            let alpha = samples
                .iter()
                .map(|sample| f64::from(sample[base + 3]) / 255.)
                .sum::<f64>()
                / 4.;
            if i % 4 == 3 {
                return (alpha * 255.).round() as u8;
            }
            if alpha == 0. {
                return 0;
            }
            let decode = |value: f64| {
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            let linear = samples
                .iter()
                .map(|sample| {
                    decode(f64::from(sample[i]) / 255.) * f64::from(sample[base + 3]) / 255.
                })
                .sum::<f64>()
                / (4. * alpha);
            let encoded = if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1. / 2.4) - 0.055
            };
            (encoded * 255.).round() as u8
        })
        .collect();
    pixels_within_one(&actual, &expected);
    let sharp = render(&mut renderer, &restored, 1.);
    let coverage = |pixels: &[u8]| pixels.chunks_exact(4).filter(|pixel| pixel[3] > 0).count();
    assert!(
        coverage(&actual) > coverage(&sharp),
        "motion blur must contain the sampled trajectory, not a repeated center image"
    );
    assert_eq!(restored, authored);
}
