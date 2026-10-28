//! Real frozen outputs, using independently authored imported geometry/camera.
use super::*;
use motion_effects::image_contract::ChannelPrecision;
use motion_render::{OutputTransform, Renderer};
#[path = "../../../motion-render/tests/support/imported_camera.rs"]
mod fixture;

const PRECISIONS: [ChannelPrecision; 4] = [
    ChannelPrecision::Unorm8,
    ChannelPrecision::Unorm16,
    ChannelPrecision::Float16,
    ChannelPrecision::Float32,
];

fn native_frames(session: &mut Session, frames: &[u32], video: bool) -> Vec<Vec<u8>> {
    let project = session.engine.project();
    let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
        project.working_precision,
        motion_render::AllocationBudget::default(),
    ))
    .unwrap();
    renderer
        .configure_assets_with_model_catalog(
            project,
            &session.root,
            &session.scene.model_source_catalog(),
        )
        .unwrap();
    let target = if video {
        renderer.sdr_video_target(96, 64, OutputTransform::SDR_CLIP)
    } else {
        renderer.sdr_capture_target(96, 64, OutputTransform::SDR_CLIP)
    }
    .unwrap();
    frames
        .iter()
        .map(|frame| {
            session.frame = f64::from(*frame);
            session.sample().unwrap();
            // Imported animation has a seconds clock independent of layer Euler tracks.
            for (actual, expected) in
                session
                    .scene
                    .camera
                    .eye
                    .to_array()
                    .into_iter()
                    .zip([*frame as f32 / 3., 0., 60.])
            {
                assert!((actual - expected).abs() < 0.00001);
            }
            renderer
                .capture_sdr(&session.scene, &target)
                .unwrap()
                .0
                .pixels
        })
        .collect()
}

#[cfg(feature = "ffmpeg")]
fn red_centroid(pixels: &[u8]) -> (f64, f64, usize) {
    let mut x = 0.;
    let mut y = 0.;
    let mut count = 0;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        if pixel[0] > 160 && pixel[1] < 60 && pixel[2] < 60 {
            x += (index % 96) as f64;
            y += (index / 96) as f64;
            count += 1;
        }
    }
    assert!(count > 100, "the encoded model must actually be visible");
    (x / count as f64, y / count as f64, count)
}

fn retain(
    name: &str,
    output: &Path,
    expected: &[Vec<u8>],
    actual: &[Vec<u8>],
    report: serde_json::Value,
) {
    let Some(root) = std::env::var_os("MOTION_IMPORTED_CAMERA_OUTPUT_ARTIFACTS") else {
        return;
    };
    let root = PathBuf::from(root).join(name);
    fs::create_dir_all(&root).unwrap();
    fs::copy(output, root.join(output.file_name().unwrap())).unwrap();
    for index in [0, expected.len() / 2, expected.len() - 1]
        .into_iter()
        .collect::<BTreeSet<_>>()
    {
        for (label, frames) in [("native", expected), ("decoded", actual)] {
            image::save_buffer(
                root.join(format!("{label}-{index:03}.png")),
                &frames[index],
                96,
                64,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    fs::write(
        root.join("comparison.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
#[cfg(feature = "ffmpeg")]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn frozen_imported_camera_mp4_restores_after_source_and_session_removal_at_all_precisions() {
    for precision in PRECISIONS {
        let source = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let queue = tempfile::tempdir().unwrap();
        let mut project = fixture::project(source.path(), true);
        project.working_precision = precision;
        project.frames = 31;
        project.work_area = motion_core::WorkArea::full(31);
        let mut session =
            Session::new(project.clone(), source.path().into(), crate::platform()).unwrap();
        let frames: Vec<_> = (0..31).collect();
        let expected = native_frames(&mut session, &frames, true);
        let first = red_centroid(&expected[0]);
        let last = red_centroid(&expected[30]);
        assert!(
            first.0 - last.0 > 8.,
            "the camera must visibly move the model"
        );
        let frozen = FrozenMovie::from_session(&session).unwrap();
        let revision = session.engine.revision();
        let saved = frozen
            .prepare_persistent(queue.path(), &ExportControl::default())
            .unwrap()
            .publish()
            .unwrap();
        let mut binding = project.camera.imported.clone().unwrap();
        binding.animation.as_mut().unwrap().offset_seconds = 1.;
        session
            .apply_command_batch(vec![motion_core::Command::CameraImported {
                object: 0,
                binding: Some(binding),
            }])
            .unwrap();
        session.frame = 0.;
        session.sample().unwrap();
        assert_eq!(session.scene.camera.eye.to_array(), [10., 0., 60.]);
        assert_ne!(session.engine.project(), &project);
        drop(session);
        drop(frozen);
        source.close().unwrap();
        let restored =
            FrozenMovie::restore_persistent_with_request(&saved, &ExportControl::default(), 8701)
                .unwrap();
        assert_eq!(restored.project, project);
        assert_eq!(restored.revision, revision);
        let path = output.path().join("imported-camera.mp4");
        let result = restored
            .export(&path, &ExportControl::default(), |_| {})
            .unwrap();
        assert_eq!(
            (result.width, result.height, result.frames, result.fps),
            (96, 64, 31, 30)
        );
        assert!(result.bytes > 0);
        let probe = crate::av::probe(&path, None, None, &|| Ok(())).unwrap();
        assert_eq!(
            (
                probe.asset.frame_count,
                probe.asset.display_width,
                probe.asset.display_height
            ),
            (31, 96, 64)
        );
        assert_eq!(probe.asset.video_end_us, 1_033_333);
        assert_eq!(
            (probe.asset.color_standard, probe.asset.color_range),
            (1, 2)
        );
        let times = probe.timestamps.clone();
        assert_eq!(times.len(), 31);
        let mut decoder = crate::av::Decoder::open(&path, probe.asset, probe.timestamps).unwrap();
        let mut decoded = Vec::new();
        let mut comparisons = Vec::new();
        for (index, pts) in times.into_iter().enumerate() {
            assert!(pts.abs_diff(index as u64 * 1_000_000 / 30) <= 1);
            let frame = decoder.frame(pts, &|| Ok(())).unwrap();
            assert_eq!(frame.pts, pts);
            let pixels = frame.rgba().unwrap().into_owned();
            assert_eq!(pixels.len(), 96 * 64 * 4);
            let actual = red_centroid(&pixels);
            let reference = red_centroid(&expected[index]);
            assert!(
                (actual.0 - reference.0).abs() <= 1.5 && (actual.1 - reference.1).abs() <= 1.5,
                "precision={precision:?} frame={index} actual={actual:?} native={reference:?}"
            );
            assert!(
                actual.2.abs_diff(reference.2) <= reference.2 / 5,
                "encoded triangle coverage drift: {actual:?} vs {reference:?}"
            );
            // Fixed-aspect source camera leaves pillars in a wider composition.
            for x in [0, 8, 88, 95] {
                let p = &pixels[(32 * 96 + x) * 4..][..4];
                assert!(
                    p[..3].iter().all(|c| *c < 12) && p[3] == 255,
                    "pillar {p:?}"
                );
            }
            comparisons.push(
                serde_json::json!({"frame":index,"pts":pts,"native":reference,"decoded":actual}),
            );
            decoded.push(pixels);
        }
        assert!(red_centroid(&decoded[0]).0 - red_centroid(&decoded[30]).0 > 8.);
        retain(
            &format!("{precision:?}-mp4"),
            &path,
            &expected,
            &decoded,
            serde_json::json!({"result":result,"frames":comparisons,"sourceRemoved":true,"restored":true}),
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn frozen_imported_camera_png_matches_native_after_live_clock_edit_and_source_removal() {
    for precision in PRECISIONS {
        let source = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let mut project = fixture::project(source.path(), true);
        project.working_precision = precision;
        let mut session =
            Session::new(project.clone(), source.path().into(), crate::platform()).unwrap();
        let expected = native_frames(&mut session, &[15], false);
        let path = output.path().join("imported-camera.png");
        let plan = frame::FrameFreezePlan::capture(
            &session,
            Some(path.clone()),
            &ExportControl::default(),
        )
        .unwrap();
        let mut binding = project.camera.imported.unwrap();
        binding.animation.as_mut().unwrap().offset_seconds = 1.;
        session
            .apply_command_batch(vec![motion_core::Command::CameraImported {
                object: 0,
                binding: Some(binding),
            }])
            .unwrap();
        drop(session);
        source.close().unwrap();
        plan.export(&ExportControl::default()).unwrap();
        let actual = image::open(&path).unwrap().to_rgba8();
        assert_eq!(actual.dimensions(), (96, 64));
        assert_eq!(actual.as_raw(), &expected[0], "precision={precision:?}");
        retain(
            &format!("{precision:?}-png"),
            &path,
            &expected,
            &[actual.into_raw()],
            serde_json::json!({"exactNativePixels":true,"sourceRemoved":true,"frame":15}),
        );
    }
}
