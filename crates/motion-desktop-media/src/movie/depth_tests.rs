//! Actual frozen outputs with scalar depth owned independently of the editor.
use super::*;
use motion_host::depth::{Backend, Cancellation, Costs, Estimate, Input};
use motion_model::depth::*;
use motion_render::{OutputTransform, Renderer};
use std::time::{Duration, Instant};

struct OwnedDepth;
impl Backend for OwnedDepth {
    fn infer(&mut self, input: &Input, _: &Cancellation) -> motion_host::depth::Result<Estimate> {
        Ok(Estimate {
            resource: DepthResource::new(
                DepthDescriptor {
                    size: input.size(),
                    source: input.key().source.clone(),
                    geometry: input.key().geometry.clone(),
                    model: input.key().model.clone(),
                    measure: DepthMeasure::Relative {
                        near: NearDirection::Increasing,
                    },
                    scale: 1.,
                    offset: 0.,
                    valid_range: None,
                },
                input
                    .rgb()
                    .chunks_exact(3)
                    .map(|c| f32::from(c[0]))
                    .collect(),
                None,
                None,
            )?,
            provider: "owned_controlled_cpu".into(),
            costs: Costs::default(),
        })
    }
}
const PRECISIONS: [motion_effects::image_contract::ChannelPrecision; 4] = [
    motion_effects::image_contract::ChannelPrecision::Unorm8,
    motion_effects::image_contract::ChannelPrecision::Unorm16,
    motion_effects::image_contract::ChannelPrecision::Float16,
    motion_effects::image_contract::ChannelPrecision::Float32,
];

fn infer(session: &mut Session) {
    session
        .request_image_depth(
            7,
            DepthGeometry {
                source_size: [3, 3],
                crop: [0, 0, 3, 3],
                quarter_turns: 0,
                mirror_x: false,
            },
            DepthModel {
                id: "owned-output-test".into(),
                revision: "1".into(),
                sha256: [7; 32],
                processing: "rgb-test".into(),
            },
        )
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if session.poll_depth().unwrap().is_some() {
            break;
        }
        assert!(
            Instant::now() < until,
            "controlled depth worker did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn session(root: &Path, precision: motion_effects::image_contract::ChannelPrecision) -> Session {
    fs::create_dir_all(root.join("assets")).unwrap();
    let mut source = image::RgbaImage::new(3, 3);
    for (x, y, pixel) in source.enumerate_pixels_mut() {
        *pixel = image::Rgba([((y * 3 + x + 1) * 10) as u8, 2, 3, 255]);
    }
    source.save(root.join("assets/input.png")).unwrap();
    let mut p = Project::new(32, 32, 30, 31).unwrap();
    p.working_precision = precision;
    p.assets.push(motion_core::Asset {
        id: 7,
        path: "assets/input.png".into(),
        width: 3,
        height: 3,
    });
    let mut photo =
        motion_core::Layer::solid(1, "Owned depth photo", [80.; 2], [16., 16., 0.], [1.; 4]);
    photo.content = motion_core::Content::Image { asset: 7 };
    photo.three_d = true;
    let mut blue = motion_core::Layer::solid(
        2,
        "Physical blue occluder",
        [80.; 2],
        [16., 16., 0.],
        [0., 0., 1., 1.],
    );
    blue.three_d = true;
    p.layers.extend([photo, blue]);
    p.camera.created = true;
    p.camera.position.set_at(0, [16., 16., -120.]).unwrap();
    p.camera.target.set_at(0, [16., 16., 0.]).unwrap();
    p.camera.fov.set_at(0, 60.).unwrap();
    let mut session = Session::new(p, root.into(), crate::platform()).unwrap();
    session.configure_depth(OwnedDepth).unwrap();
    infer(&mut session);
    let calibration = session
        .depth_calibration_command(
            1,
            [
                DepthAnchor {
                    value: 10.,
                    camera_z_meters: 8.,
                },
                DepthAnchor {
                    value: 90.,
                    camera_z_meters: 4.,
                },
            ],
            CalibrationCurve::CameraZ,
            CalibrationExtrapolation::Reject,
            20.,
        )
        .unwrap();
    session.apply_command_batch(vec![calibration]).unwrap();
    infer(&mut session);
    session
}
fn pixel(p: &[u8], x: usize, y: usize) -> &[u8] {
    &p[(y * 32 + x) * 4..][..4]
}
fn occlusion(p: &[u8], lossless: bool) {
    let far = pixel(p, 16, 10);
    let near = pixel(p, 16, 22);
    if lossless {
        assert_eq!(far, &[0, 0, 255, 255]);
    } else {
        assert!(
            far[0] < 20 && far[1] < 20 && far[2] > 220,
            "far region {far:?}"
        );
    }
    assert!(
        near[0] > 40 && near[1] < 20 && near[2] < 20 && near[3] == 255,
        "near region {near:?}"
    );
}
fn retain(name: &str, path: &Path, expected: &[u8], pixels: &[u8], report: serde_json::Value) {
    let Some(root) = std::env::var_os("MOTION_FROZEN_DEPTH_ARTIFACTS") else {
        return;
    };
    let root = PathBuf::from(root).join(name);
    fs::create_dir_all(&root).unwrap();
    fs::copy(path, root.join(path.file_name().unwrap())).unwrap();
    for (name, data) in [("native.png", expected), ("decoded.png", pixels)] {
        image::save_buffer(root.join(name), data, 32, 32, image::ColorType::Rgba8).unwrap();
    }
    fs::write(
        root.join("comparison.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_png_keeps_calibrated_geometry_after_seek_close_and_original_source_removal() {
    for precision in PRECISIONS {
        let original = tempfile::tempdir().unwrap();
        let outputs = tempfile::tempdir().unwrap();
        let mut session = session(original.path(), precision);
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            Default::default(),
        ))
        .unwrap();
        let estimate = session.poll_depth().unwrap().unwrap();
        let input = session.depth_input_fingerprint().unwrap().unwrap();
        let p = session.engine.project();
        renderer.configure_assets(p, original.path()).unwrap();
        let mut scene = motion_render::Scene::new(p).unwrap();
        scene
            .sample(p, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        scene
            .attach_calibrated_depth(
                1,
                &estimate.resource.descriptor().source,
                &estimate.resource,
                input,
            )
            .unwrap();
        renderer
            .prepare_scene_assets(
                &scene,
                motion_render::image_resources::Resolution::Full,
                false,
            )
            .unwrap();
        let target = renderer
            .sdr_capture_target(32, 32, OutputTransform::SDR_CLIP)
            .unwrap();
        let expected = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
        drop(estimate);
        let output = outputs.path().join("frozen.png");
        let plan = frame::FrameFreezePlan::capture(
            &session,
            Some(output.clone()),
            &ExportControl::default(),
        )
        .unwrap();
        session.frame = 15.;
        session.sample().unwrap();
        assert!(
            session.poll_depth().unwrap().is_none(),
            "live result remains frame-bound"
        );
        assert!(frame::FrameFreezePlan::capture(&session, None, &ExportControl::default()).is_ok(),
            "immutable admitted static sources can freeze at another camera time");
        session.cancel_depth().unwrap();
        assert!(
            frame::FrameFreezePlan::capture(&session, None, &ExportControl::default())
                .err()
                .unwrap()
                .contains("pending")
        );
        drop(session);
        original.close().unwrap();
        plan.export(&ExportControl::default()).unwrap();
        let actual = image::open(&output).unwrap().into_rgba8().into_raw();
        occlusion(&actual, true);
        assert_eq!(actual, expected);
        retain(
            &format!("png-{precision:?}"),
            &output,
            &expected,
            &actual,
            serde_json::json!({"precision":format!("{precision:?}"),"exactNativePixels":true,"sessionClosed":true,"originalRemoved":true}),
        );
    }
}

#[test]
#[cfg(all(feature = "ffmpeg", feature = "gpu-tests"))]
fn durable_mp4_restores_scalar_depth_after_all_owners_exit_and_decodes_every_frame() {
    for precision in PRECISIONS {
        let original = tempfile::tempdir().unwrap();
        let queue = tempfile::tempdir().unwrap();
        let mut session = session(original.path(), precision);
        let frozen = FrozenMovie::from_session(&session).unwrap();
        assert_eq!(frozen.depth.layer_count(), 1);
        let persisted = frozen
            .prepare_persistent(queue.path(), &ExportControl::default())
            .unwrap()
            .publish()
            .unwrap();
        drop(frozen);
        session.frame = 1.;
        session.sample().unwrap();
        drop(session);
        original.close().unwrap();
        let restored =
            FrozenMovie::restore_persistent(&persisted, &ExportControl::default()).unwrap();
        assert_eq!(restored.depth.layer_count(), 1);
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            Default::default(),
        ))
        .unwrap();
        renderer
            .configure_assets(&restored.project, restored.assets.path())
            .unwrap();
        let target = renderer
            .sdr_video_target(32, 32, OutputTransform::SDR_CLIP)
            .unwrap();
        let mut scene = motion_render::Scene::new(&restored.project).unwrap();
        scene
            .sample(
                &restored.project,
                0.,
                None,
                &motion_core::ExpressionEvaluator,
            )
            .unwrap();
        restored
            .depth
            .verify_sources(restored.assets.path(), &|| Ok(()))
            .unwrap()
            .attach(&mut scene)
            .unwrap();
        renderer
            .prepare_scene_assets(
                &scene,
                motion_render::image_resources::Resolution::Full,
                false,
            )
            .unwrap();
        let expected = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
        let output = queue.path().join("depth.mp4");
        let result = restored
            .export(&output, &ExportControl::default(), |_| {})
            .unwrap();
        assert!(result.bytes > 0);
        let probe = crate::av::probe(&output, None, None, &|| Ok(())).unwrap();
        assert_eq!(probe.timestamps.len(), 31);
        let mut decoder =
            crate::av::Decoder::open(&output, probe.asset, probe.timestamps.clone()).unwrap();
        let mut last = Vec::new();
        let mut max_difference = 0;
        for pts in probe.timestamps {
            let decoded = decoder.frame(pts, &|| Ok(())).unwrap();
            let pixels = decoded.rgba().unwrap();
            occlusion(&pixels, false);
            let difference: u64 = pixels
                .iter()
                .zip(&expected)
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            assert!(
                difference < 32 * 32 * 4 * 8,
                "encoded mean absolute error exceeds eight: {difference}"
            );
            max_difference = max_difference.max(difference);
            last = pixels.into_owned();
        }
        retain(
            &format!("mp4-{precision:?}"),
            &output,
            &expected,
            &last,
            serde_json::json!({"precision":format!("{precision:?}"),"decodedFrames":31,"maxAbsoluteDifferenceSum":max_difference,"sessionAndOriginalGone":true,"restoredFromDisk":true}),
        );
        // Scalar payload integrity is verified before any output publication.
        let payload = persisted.join("depth.snapshot");
        let mut bytes = fs::read(&payload).unwrap();
        let end = bytes.len() - 1;
        bytes[end] ^= 1;
        fs::write(payload, bytes).unwrap();
        assert!(
            FrozenMovie::restore_persistent(&persisted, &ExportControl::default())
                .err()
                .unwrap()
                .contains("integrity failure: depth.snapshot")
        );
    }
}
