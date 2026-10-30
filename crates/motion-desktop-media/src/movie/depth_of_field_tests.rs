//! Actual durable output, not merely serialization of camera fields.
use super::*;
use motion_model::{DepthOfField, Track};
use motion_render::{OutputTransform, Renderer, Scene};

fn authored(precision: motion_effects::image_contract::ChannelPrecision) -> Project {
    let mut p = Project::new(32, 32, 30, 6).unwrap();
    p.working_precision = precision;
    p.background = [0., 0., 0., 1.];
    p.camera.created = true;
    p.camera.position.value = [16., 16., -100.];
    p.camera.target.value = [16., 16., 0.];
    p.camera.fov.value = 60.;
    let mut focus = Track::constant(100.);
    focus.set_animated(0, true).unwrap();
    focus.set_at(5, 200.).unwrap();
    p.camera.depth_of_field = Some(DepthOfField {
        focus_distance: focus,
        aperture_diameter: Track::constant(100.),
        max_radius_pixels: 8.,
    });
    let mut card = motion_core::Layer::solid(
        1,
        "Owned aperture card",
        [60.; 2],
        [16., 16., 0.],
        [1., 0., 0., 1.],
    );
    card.three_d = true;
    p.layers.push(card);
    p.validate().unwrap();
    p
}
fn native(p: &Project, frame: f64) -> Vec<u8> {
    let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
        p.working_precision,
        Default::default(),
    ))
    .unwrap();
    let mut scene = Scene::new(p).unwrap();
    scene
        .sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let target = renderer
        .sdr_capture_target(32, 32, OutputTransform::SDR_CLIP)
        .unwrap();
    renderer.capture_sdr(&scene, &target).unwrap().0.pixels
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn frozen_aperture_png_and_restored_movie_keep_animated_focus_after_editor_close() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let original = tempfile::tempdir().unwrap();
        let outputs = tempfile::tempdir().unwrap();
        let p = authored(precision);
        let expected_first = native(&p, 0.);
        let expected_last = native(&p, 5.);
        assert_ne!(
            expected_first, expected_last,
            "native focus animation must be visible"
        );
        let mut session =
            Session::new(p.clone(), original.path().to_owned(), crate::platform()).unwrap();
        session.frame = 5.;
        session.sample().unwrap();
        let png = outputs.path().join("aperture.png");
        let plan =
            frame::FrameFreezePlan::capture(&session, Some(png.clone()), &ExportControl::default())
                .unwrap();
        let frozen = FrozenMovie::from_session(&session).unwrap();
        let persisted = frozen
            .prepare_persistent(outputs.path(), &ExportControl::default())
            .unwrap()
            .publish()
            .unwrap();
        session.frame = 0.;
        session.sample().unwrap();
        drop((session, frozen));
        original.close().unwrap();
        plan.export(&ExportControl::default()).unwrap();
        assert_eq!(
            image::open(&png).unwrap().into_rgba8().into_raw(),
            expected_last,
            "frozen PNG followed a live camera time"
        );
        let restored =
            FrozenMovie::restore_persistent(&persisted, &ExportControl::default()).unwrap();
        assert_eq!(restored.project, p);
        let mp4 = outputs.path().join("aperture.mp4");
        restored
            .export(&mp4, &ExportControl::default(), |_| {})
            .unwrap();
        let probe = crate::av::probe(&mp4, None, None, &|| Ok(())).unwrap();
        assert_eq!(probe.timestamps.len(), 6);
        let timestamps = probe.timestamps.clone();
        let mut decoder = crate::av::Decoder::open(&mp4, probe.asset, probe.timestamps).unwrap();
        let mut endpoints = Vec::new();
        for (pts, expected) in [
            (timestamps[0], &expected_first),
            (timestamps[5], &expected_last),
        ] {
            let frame = decoder.frame(pts, &|| Ok(())).unwrap();
            let pixels = frame.rgba().unwrap().into_owned();
            let error: u64 = pixels
                .iter()
                .zip(expected)
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            assert!(
                error < pixels.len() as u64 * 8,
                "{precision:?} decoded focus mismatch mean={}",
                error as f64 / pixels.len() as f64
            );
            endpoints.push(pixels);
        }
        assert_ne!(
            endpoints[0], endpoints[1],
            "encoded movie must retain the focus animation"
        );
        if let Some(root) = std::env::var_os("MOTION_FROZEN_DEPTH_ARTIFACTS") {
            let root = PathBuf::from(root).join(format!("camera-aperture-{precision:?}"));
            fs::create_dir_all(&root).unwrap();
            fs::copy(&png, root.join("frozen.png")).unwrap();
            fs::copy(&mp4, root.join("animated.mp4")).unwrap();
            for (name, pixels) in [
                ("native-first.png", expected_first),
                ("native-last.png", expected_last),
                ("decoded-first.png", endpoints.remove(0)),
                ("decoded-last.png", endpoints.remove(0)),
            ] {
                image::save_buffer(root.join(name), &pixels, 32, 32, image::ColorType::Rgba8)
                    .unwrap();
            }
        }
    }
}
