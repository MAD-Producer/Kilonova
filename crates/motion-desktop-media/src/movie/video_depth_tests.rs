//! Actual H.264 source decoding, indexed calibration and native GPU output.
//! The controlled scalar estimator consumes real codec RGB; no second decoder
//! supplies the admitted surface's colors during playback.
use super::*;
use motion_core::{Content, Layer, VideoClip};
use motion_host::{
    depth::{Backend, Cancellation, Costs, Estimate, Input},
    platform::VideoDecoder,
};
use motion_model::depth::*;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
mod cancellation;
mod storage;

struct RedDepth;
impl Backend for RedDepth {
    fn infer(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        cancel.check()?;
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
                    .map(|rgb| f32::from(rgb[0]))
                    .collect(),
                None,
                None,
            )?,
            provider: "owned-red-channel-test".into(),
            costs: Costs::default(),
        })
    }
}

fn silence(encoder: &mut crate::encode::Mp4Encoder, frames: u32) {
    let count = u64::from(frames) * 48_000 / 30;
    let mut offset = 0;
    while offset < count {
        let length = encoder.audio_frame_size().min((count - offset) as usize);
        encoder.audio_frame(&vec![0.; length * 2], offset).unwrap();
        offset += length as u64;
    }
}

fn encode(path: &Path, pixels: &[Vec<u8>]) {
    let mut encoder = crate::encode::Mp4Encoder::new(path, 32, 32, 30).unwrap();
    for (index, frame) in pixels.iter().enumerate() {
        encoder.video_frame(frame, index as u32).unwrap();
    }
    silence(&mut encoder, pixels.len() as u32);
    encoder.finish().unwrap();
}

fn source(root: &Path) -> (Project, Vec<u64>, Vec<Vec<u8>>) {
    source_frames(root, 3)
}

fn source_frames(root: &Path, count: usize) -> (Project, Vec<u64>, Vec<Vec<u8>>) {
    fs::create_dir_all(root.join("assets")).unwrap();
    let path = root.join("assets/source.mp4");
    let frames: Vec<_> = (0..count)
        .map(|index| {
            (0..32)
                .flat_map(|y| (0..32).flat_map(move |_| [24 + (index % 3) as u8 * 40 + y * 2, 4, 4, 255]))
                .collect()
        })
        .collect();
    encode(&path, &frames);
    let mut probe = crate::av::probe(&path, None, None, &|| Ok(())).unwrap();
    assert_eq!(probe.timestamps.len(), count);
    probe.asset.id = 74;
    probe.asset.path = "assets/source.mp4".into();
    let index = motion_media::video_cache_path(root, &probe.asset).unwrap();
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
    let mut decoder =
        crate::av::Decoder::open(&path, probe.asset.clone(), probe.timestamps.clone()).unwrap();
    let decoded: Vec<_> = probe
        .timestamps
        .iter()
        .map(|pts| {
            let frame = decoder.frame(*pts, &|| Ok(())).unwrap();
            assert_eq!(frame.pts, *pts);
            assert_eq!(
                decoder.rgb_encoding().unwrap(),
                motion_host::video_frame::RgbEncoding::Srgb
            );
            frame.rgba().unwrap().into_owned()
        })
        .collect();
    let mut project = Project::new(32, 32, 30, count as u32).unwrap();
    let mut video = Layer::solid(
        1,
        "Owned real-codec video",
        [80.; 2],
        [16., 16., 0.],
        [1.; 4],
    );
    video.content = Content::Video {
        video: VideoClip::new(74),
    };
    video.three_d = true;
    let mut plane = Layer::solid(
        2,
        "Independent blue occluder",
        [80.; 2],
        [16., 16., 0.],
        [0., 0., 1., 1.],
    );
    plane.three_d = true;
    project.layers.extend([video, plane]);
    project.video_assets.push(probe.asset);
    project.camera.created = true;
    project
        .camera
        .position
        .set_at(0, [16., 16., -120.])
        .unwrap();
    project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
    project.camera.fov.set_at(0, 60.).unwrap();
    (project, probe.timestamps, decoded)
}

fn wait(session: &mut Session) -> Arc<Estimate> {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(result) = session.poll_depth().unwrap() {
            return result;
        }
        assert!(Instant::now() < until, "real-codec depth did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn stage(session: &mut Session, index: usize) -> Arc<Estimate> {
    session.frame = index as f64;
    session
        .request_video_depth(
            1,
            DepthGeometry {
                source_size: [32; 2],
                crop: [0, 0, 32, 32],
                quarter_turns: 0,
                mirror_x: false,
            },
            DepthModel {
                id: "owned-real-codec-test".into(),
                revision: "1".into(),
                sha256: [7; 32],
                processing: "red-channel".into(),
            },
        )
        .unwrap()
        .unwrap();
    wait(session)
}

fn author(session: &mut Session, index: usize, pts: u64, decoded: &[u8]) {
    let estimate = stage(session, index);
    assert_eq!(
        estimate.resource.descriptor().source.pts,
        DepthTime {
            ticks: pts as i64,
            timescale: 1_000_000
        }
    );
    let rgb: Vec<_> = decoded
        .chunks_exact(4)
        .flat_map(|pixel| pixel[..3].iter().copied())
        .collect();
    // PreparedDepthInput hashes the declared RGB contract and little-endian
    // dimensions before the pixels; a bare pixel digest is a different format.
    let mut fingerprint = Sha256::new();
    fingerprint.update(b"MotionStudio/depth/rgb8-srgb-opaque/v1\0");
    fingerprint.update(32u32.to_le_bytes());
    fingerprint.update(32u32.to_le_bytes());
    fingerprint.update(&rgb);
    assert_eq!(
        session.depth_input_fingerprint().unwrap().unwrap(),
        PreparedDepthInput {
            rgb_size: [32; 2],
            rgb_sha256: fingerprint.finalize().into(),
        },
        "the worker must consume the actual independently decoded RGB"
    );
    assert_eq!(
        estimate.resource.values(),
        rgb.chunks_exact(3)
            .map(|pixel| f32::from(pixel[0]))
            .collect::<Vec<_>>(),
        "each scalar must match the independently decoded source red channel"
    );
    let low = estimate
        .resource
        .values()
        .iter()
        .copied()
        .fold(f32::INFINITY, f32::min);
    let high = estimate
        .resource
        .values()
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(high - low > 40.);
    let command = session
        .video_depth_calibration_command(
            1,
            [
                DepthAnchor {
                    value: f64::from(low),
                    camera_z_meters: 8.,
                },
                DepthAnchor {
                    value: f64::from(high),
                    camera_z_meters: 4.,
                },
            ],
            CalibrationCurve::CameraZ,
            CalibrationExtrapolation::Reject,
            20.,
        )
        .unwrap();
    session.apply_command_batch(vec![command]).unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_h264_depth_playback_matches_indexed_rgb_and_encodes_every_native_frame() {
    use motion_effects::image_contract::ChannelPrecision::{Float16, Float32, Unorm16, Unorm8};
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let root = tempfile::tempdir().unwrap();
        let (mut project, pts, decoded) = source(root.path());
        project.working_precision = precision;
        let id =
            motion_host::session::open(root.path().into(), project, crate::platform()).unwrap();
        struct Close(i64);
        impl Drop for Close {
            fn drop(&mut self) {
                motion_host::session::close(self.0);
            }
        }
        let close = Close(id);
        motion_host::session::with_session(id, |s| {
            s.configure_depth(RedDepth)?;
            for index in 0..3 {
                author(s, index, pts[index], &decoded[index]);
            }
            assert_eq!(
                s.engine.project().layers[0]
                    .video_depth_calibration
                    .as_ref()
                    .unwrap()
                    .samples()
                    .len(),
                3
            );
            Ok(())
        })
        .unwrap();
        let mut native = Vec::new();
        for index in 0..3 {
            motion_host::session::with_session(id, |s| {
                s.frame = index as f64;
                Ok(())
            })
            .unwrap();
            assert!(motion_host::ops::export::capture(id)
                .unwrap_err()
                .to_string()
                .contains("pending"));
            motion_host::session::with_session(id, |s| {
                wait(s);
                Ok(())
            })
            .unwrap();
            let capture = motion_host::ops::export::capture(id).unwrap();
            let pixels = image::open(capture["path"].as_str().unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(pixels.get_pixel(16, 10).0, [0, 0, 255, 255]);
            let near = pixels.get_pixel(16, 22).0;
            assert!(
                near[0] > 30 + index as u8 * 40 && near[1] < 20 && near[2] < 20 && near[3] == 255,
                "{precision:?} frame={index}: near real-codec geometry/color is wrong: {near:?}"
            );
            native.push(pixels.into_raw());
        }
        assert_ne!(native[0], native[1]);
        assert_ne!(native[1], native[2]);
        let output = root.path().join("native-depth.mp4");
        encode(&output, &native);
        let probe = crate::av::probe(&output, None, None, &|| Ok(())).unwrap();
        assert_eq!(probe.timestamps.len(), 3);
        let mut reader =
            crate::av::Decoder::open(&output, probe.asset, probe.timestamps.clone()).unwrap();
        let mut comparisons = Vec::new();
        for (index, pts) in probe.timestamps.iter().enumerate() {
            let frame = reader.frame(*pts, &|| Ok(())).unwrap();
            let pixels = frame.rgba().unwrap();
            let difference: u64 = pixels
                .iter()
                .zip(&native[index])
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            assert!(
                difference < 32 * 32 * 4 * 8,
                "encoded native mean absolute difference exceeds eight: {difference}"
            );
            comparisons.push(difference);
        }
        drop(reader);
        if let Some(directory) = std::env::var_os("MOTION_REAL_VIDEO_DEPTH_ARTIFACTS") {
            let directory = PathBuf::from(directory).join(format!("{precision:?}"));
            fs::create_dir_all(&directory).unwrap();
            fs::copy(
                root.path().join("assets/source.mp4"),
                directory.join("source.mp4"),
            )
            .unwrap();
            fs::copy(&output, directory.join("native-depth.mp4")).unwrap();
            for (index, pixels) in native.iter().enumerate() {
                image::save_buffer(
                    directory.join(format!("native-{index}.png")),
                    pixels,
                    32,
                    32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            fs::write(directory.join("comparison.json"),serde_json::to_vec_pretty(&serde_json::json!({"precision":format!("{precision:?}"),"decodedFrames":3,"sourcePts":pts,"absoluteDifferences":comparisons,"depthBackend":"controlled actual decoded RGB","durableFrozenVideoExport":false})).unwrap()).unwrap();
        }
        fs::remove_file(root.path().join("assets/source.mp4")).unwrap();
        for (frame, index) in [(0.5, 0), (1.5, 1), (2.5, 2), (0., 0)] {
            motion_host::session::with_session(id, |s| {
                s.frame = frame;
                Ok(())
            })
            .unwrap();
            let capture = motion_host::ops::export::capture(id).unwrap();
            assert_eq!(
                image::open(capture["path"].as_str().unwrap())
                    .unwrap()
                    .to_rgba8()
                    .into_raw(),
                native[index]
            );
        }
        motion_host::session::with_session(id, |s| {
            let status = s.depth_status()?.unwrap();
            assert_eq!(
                (
                    status.counts.submitted,
                    status.retained_samples,
                    status.retained_image_bytes
                ),
                (6, 3, 3 * 32 * 32 * 4)
            );
            Ok(())
        })
        .unwrap();
        drop(close);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_frozen_temporal_depth_restores_and_exports_after_session_and_original_are_gone() {
    use motion_effects::image_contract::ChannelPrecision::{Float16, Float32, Unorm16, Unorm8};
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let original = tempfile::tempdir().unwrap();
        let queue = tempfile::tempdir().unwrap();
        let (mut project, pts, decoded) = source(original.path());
        project.working_precision = precision;
        let id =
            motion_host::session::open(original.path().into(), project, crate::platform()).unwrap();
        motion_host::session::with_session(id, |s| {
            s.configure_depth(RedDepth)?;
            for index in 0..3 {
                author(s, index, pts[index], &decoded[index]);
            }
            // Saving calibrations invalidates the prior admissions. Metadata
            // alone must not become an exportable scalar surface.
            assert!(FrozenMovie::capture_plan(s, &ExportControl::default())
                .err()
                .unwrap()
                .contains("pending or absent"));
            Ok(())
        })
        .unwrap();
        let mut expected = Vec::new();
        for index in 0..3 {
            motion_host::session::with_session(id, |s| {
                s.frame = index as f64;
                Ok(())
            })
            .unwrap();
            assert!(motion_host::ops::export::capture(id)
                .unwrap_err()
                .to_string()
                .contains("pending"));
            motion_host::session::with_session(id, |s| {
                wait(s);
                Ok(())
            })
            .unwrap();
            let capture = motion_host::ops::export::capture(id).unwrap();
            expected.push(
                image::open(capture["path"].as_str().unwrap())
                    .unwrap()
                    .into_rgba8()
                    .into_raw(),
            );
        }
        let (plan, png_plan) = motion_host::session::with_session(id, |s| {
            s.frame = 1.;
            Ok((
                FrozenMovie::capture_plan(s, &ExportControl::default()).unwrap(),
                frame::FrameFreezePlan::capture(
                    s,
                    Some(queue.path().join("frozen.png")),
                    &ExportControl::default(),
                )
                .unwrap(),
            ))
        })
        .unwrap();
        let frozen = plan.freeze(&ExportControl::default()).unwrap();
        assert_eq!(frozen.depth.layer_count(), 1);
        let prepared = frozen
            .prepare_persistent(queue.path(), &ExportControl::default())
            .unwrap();
        let persisted = prepared.publish().unwrap();
        motion_host::session::close(id);
        drop(frozen);
        original.close().unwrap();
        png_plan.export(&ExportControl::default()).unwrap();
        assert_eq!(
            image::open(queue.path().join("frozen.png"))
                .unwrap()
                .into_rgba8()
                .into_raw(),
            expected[1]
        );
        let restored =
            FrozenMovie::restore_persistent(&persisted, &ExportControl::default()).unwrap();
        let output = queue.path().join("frozen-temporal.mp4");
        let result = restored
            .export(&output, &ExportControl::default(), |_| {})
            .unwrap();
        assert!(result.bytes > 0);
        let probe = crate::av::probe(&output, None, None, &|| Ok(())).unwrap();
        assert_eq!(probe.timestamps.len(), 3);
        let mut reader =
            crate::av::Decoder::open(&output, probe.asset, probe.timestamps.clone()).unwrap();
        let mut comparisons = Vec::new();
        for (index, pts) in probe.timestamps.iter().enumerate() {
            let decoded = reader.frame(*pts, &|| Ok(())).unwrap();
            let pixels = decoded.rgba().unwrap();
            let far = &pixels[(10 * 32 + 16) * 4..][..4];
            let near = &pixels[(22 * 32 + 16) * 4..][..4];
            assert!(
                far[0] < 20 && far[1] < 20 && far[2] > 220,
                "{precision:?} {index}: far={far:?}"
            );
            assert!(
                near[0] > 30 + index as u8 * 40 && near[1] < 20 && near[2] < 20,
                "{precision:?} {index}: near={near:?}"
            );
            let difference: u64 = pixels
                .iter()
                .zip(&expected[index])
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            assert!(
                difference < 32 * 32 * 4 * 8,
                "{precision:?} frame={index}: difference={difference}"
            );
            comparisons.push(difference);
        }
        drop(reader);
        if let Some(directory) = std::env::var_os("MOTION_REAL_VIDEO_DEPTH_ARTIFACTS") {
            let directory = PathBuf::from(directory).join(format!("frozen-{precision:?}"));
            fs::create_dir_all(&directory).unwrap();
            fs::copy(&output, directory.join("frozen-temporal.mp4")).unwrap();
            fs::copy(
                queue.path().join("frozen.png"),
                directory.join("frozen.png"),
            )
            .unwrap();
            fs::copy(
                persisted.join("depth.snapshot"),
                directory.join("depth.snapshot"),
            )
            .unwrap();
            for (index, pixels) in expected.iter().enumerate() {
                image::save_buffer(
                    directory.join(format!("native-{index}.png")),
                    pixels,
                    32,
                    32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            fs::write(directory.join("comparison.json"), serde_json::to_vec_pretty(&serde_json::json!({
                "precision":format!("{precision:?}"), "decodedFrames":3, "sourcePts":pts,
                "absoluteDifferences":comparisons, "sessionAndOriginalGone":true, "restoredFromDisk":true,
                "depthBackend":"controlled actual decoded RGB", "durableFrozenVideoExport":true
            })).unwrap()).unwrap();
        }
        let cancelled =
            FrozenMovie::restore_persistent(&persisted, &ExportControl::default()).unwrap();
        let bytes = fs::read(persisted.join("depth.snapshot")).unwrap();
        let mut corrupted = bytes.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        fs::write(persisted.join("depth.snapshot"), corrupted).unwrap();
        assert!(
            FrozenMovie::restore_persistent(&persisted, &ExportControl::default())
                .err()
                .unwrap()
                .contains("integrity failure: depth.snapshot")
        );
        // Failure/cancellation must leave the existing output intact.
        let sentinel = queue.path().join("keep.mp4");
        fs::write(&sentinel, b"existing complete output").unwrap();
        let control = ExportControl::default();
        control.cancel();
        assert!(cancelled
            .export(&sentinel, &control, |_| {})
            .err()
            .unwrap()
            .contains("cancel"));
        assert_eq!(fs::read(sentinel).unwrap(), b"existing complete output");
    }
}
