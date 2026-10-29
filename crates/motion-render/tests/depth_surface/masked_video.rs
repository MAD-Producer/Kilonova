//! Indexed owned video colors, geometry and masks change together per sample.
use super::*;
use motion_render::{image_resources::Resolution, DepthImage};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn indexed_video_masks_preserve_source_time_depth_color_and_pending_frames() {
    let root = tempfile::tempdir().unwrap();
    let mut p = video::video_project(root.path());
    let reference = p.layers[0]
        .depth_calibration
        .take()
        .unwrap()
        .reference()
        .unwrap()
        .clone();
    let mut track = VideoDepthTrack::default();
    let mut resources = vec![];
    for index in 0..2 {
        let mut descriptor = field(index + 1).descriptor().clone();
        descriptor.source.pts = DepthTime {
            ticks: index as i64 * 1_000_000,
            timescale: 1_000_000,
        };
        let resource = DepthResource::new(
            descriptor,
            if index == 0 {
                vec![1., 0.]
            } else {
                vec![0., 1.]
            },
            None,
            None,
        )
        .unwrap();
        let calibration = SavedDepthCalibration::new(
            &resource,
            input(),
            [
                DepthAnchor {
                    value: 0.,
                    camera_z_meters: 12.,
                },
                DepthAnchor {
                    value: 1.,
                    camera_z_meters: 8.,
                },
            ],
            CalibrationCurve::CameraZ,
            CalibrationExtrapolation::Reject,
            10.,
        )
        .unwrap()
        .with_reference(reference.clone())
        .unwrap();
        track
            .upsert(VideoDepthSample::new(calibration, (index + 1) * 1_000_000).unwrap())
            .unwrap();
        resources.push(resource);
    }
    p.layers[0].video_depth_calibration = Some(track);
    p.layers[0]
        .masks
        .push(calibrated_masks::right_source_half());
    p.layers[0].masks[0].inverted = true;
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&p, root.path()).unwrap();
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let mut scenes = vec![];
    for (index, resource) in resources.iter().enumerate() {
        p.layers[0].masks[0].inverted = index == 0;
        let mut scene = Scene::new(&p).unwrap();
        scene
            .sample(
                &p,
                (index * 30) as f64,
                None,
                &motion_core::ExpressionEvaluator,
            )
            .unwrap();
        assert_eq!(
            scene.video_layers()[0]
                .video
                .as_ref()
                .unwrap()
                .source_time_us,
            index as u64 * 1_000_000
        );
        let color = if index == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        };
        scene
            .attach_calibrated_depth_image(
                1,
                &resource.descriptor().source,
                resource,
                input(),
                DepthImage::new([2, 1], color.repeat(2)).unwrap(),
            )
            .unwrap();
        assert!(scene.video_decode_layers().is_empty());
        renderer
            .prepare_scene_assets(&scene, Resolution::Full, false)
            .unwrap();
        let pixels = renderer
            .capture(&scene, if index == 0 { &a } else { &b })
            .unwrap()
            .0;
        let expected = if index == 0 {
            &[255, 0, 0, 255]
        } else {
            &[0, 0, 255, 255]
        };
        assert_eq!(
            &pixels[(32 * 96 + 36) * 4..][..4],
            expected,
            "the retained left half must use the current physical depth"
        );
        assert_eq!(
            &pixels[(32 * 96 + 60) * 4..][..4],
            if index == 0 {
                &[0, 0, 255, 255]
            } else {
                &[0, 255, 0, 255]
            },
            "the animated mask must expose the current near source color"
        );
        retain_fixture(&format!("masked-video-{index}.png"), &pixels);
        scenes.push((scene, pixels));
    }
    assert_ne!(scenes[0].1, scenes[1].1);
    renderer
        .prepare_scene_assets(&scenes[0].0, Resolution::Full, false)
        .unwrap();
    let mut first = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scenes[0].0, &a.view, 96, 64, &mut first, None)
        .unwrap();
    renderer
        .prepare_scene_assets(&scenes[1].0, Resolution::Full, false)
        .unwrap();
    let mut last = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scenes[1].0, &b.view, 96, 64, &mut last, None)
        .unwrap();
    renderer.submit_allocated(last.finish(), |_| {}).unwrap();
    renderer.submit_allocated(first.finish(), |_| {}).unwrap();
    assert_eq!(renderer.read_target(&a).unwrap(), scenes[0].1);
    assert_eq!(renderer.read_target(&b).unwrap(), scenes[1].1);
    renderer.clear_assets();
    renderer.configure_assets(&p, root.path()).unwrap();
    for mode in [Resolution::Full, Resolution::Preview(1), Resolution::Full] {
        for (scene, expected) in &scenes {
            renderer.prepare_scene_assets(scene, mode, false).unwrap();
            assert_eq!(&renderer.capture(scene, &a).unwrap().0, expected);
        }
    }
}
