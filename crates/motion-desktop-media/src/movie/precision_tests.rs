use super::*;
use motion_core::Command;
use motion_effects::image_contract::ChannelPrecision;
#[path = "../../../motion-render/tests/support/sdr_fixture.rs"]
mod fixture;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_mp4_uses_frozen_precision_exposure_and_linear_black_sdr_conversion() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        for exposure in [false, true] {
            let root = tempfile::tempdir().unwrap();
            // Align this color-policy oracle to H.264 macroblocks. Nonaligned
            // dimensions have their own encoder layout tests; padding artifacts
            // must not be mistaken for a working-to-output color transform.
            let (project, registry) = fixture::project_sized(precision, exposure, 32);
            let mut session = Session::new(project, root.path().into(), crate::platform()).unwrap();
            session.effects.set_registry(registry);
            let frozen = FrozenMovie::from_session(&session).unwrap();
            assert_eq!(frozen.project.working_precision, precision);
            session
                .engine
                .apply_batch(vec![Command::SetWorkingPrecision {
                    precision: ChannelPrecision::Unorm8,
                }])
                .unwrap();
            drop(session);
            let output = root.path().join("native-output.mp4");
            frozen
                .export(&output, &ExportControl::default(), |_| {})
                .unwrap();
            let (first, last) = super::tests::decoded_pixels(&output);
            if let Some(directory) = std::env::var_os("MOTION_PRECISION_OUTPUT_ARTIFACTS") {
                let directory = PathBuf::from(directory);
                fs::create_dir_all(&directory).unwrap();
                let name = format!("{precision:?}-exposure{exposure}");
                fs::copy(&output, directory.join(format!("{name}.mp4"))).unwrap();
                for (label, pixels) in [("first", &first), ("last", &last)] {
                    image::save_buffer(
                        directory.join(format!("{name}-{label}.png")),
                        pixels,
                        32,
                        32,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
            // The first centered shutter has two of four taps before the clip;
            // the last frame has full coverage. Decode verifies actual MP4 RGB,
            // frame duration, codec color metadata and both endpoint frames.
            for (image, expected) in [
                (
                    &first,
                    if exposure {
                        [99u8, 0, 71]
                    } else {
                        [137u8, 0, 99]
                    },
                ),
                (&last, [137u8, 0, 99]),
            ] {
                assert_eq!(image.len(), 32 * 32 * 4);
                for pixel in image.chunks_exact(4) {
                    for (actual, expected) in pixel[..3].iter().zip(expected) {
                        assert!(
                            actual.abs_diff(expected) <= 10,
                            "{precision:?} exposure={exposure} pixel={pixel:?} expected={expected}"
                        );
                    }
                    assert_eq!(pixel[3], 255);
                }
            }
        }
    }
}

#[path = "../../../motion-render/tests/support/dense_output_fixture.rs"]
mod dense_fixture;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn dense_authored_curve_reaches_actual_frozen_mp4_frames_after_live_removal_and_close() {
    let mut report = Vec::new();
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        for exposure in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let (project, registry) = dense_fixture::project(precision, exposure);
            let mut session = Session::new(project, root.path().into(), crate::platform()).unwrap();
            session.effects.set_registry(registry);
            let frozen = FrozenMovie::from_session(&session).unwrap();
            assert_eq!(frozen.project.working_precision, precision);
            let revision = session.engine.revision();
            session
                .engine
                .apply_batch(vec![
                    Command::SetWorkingPrecision {
                        precision: ChannelPrecision::Unorm8,
                    },
                    Command::Effect {
                        object: 1,
                        action: motion_core::EffectAction::Remove { effect: 1 },
                    },
                ])
                .unwrap();
            assert!(session.engine.project().layers[0].effects.is_empty());
            drop(session);
            let output = root.path().join("dense.mp4");
            frozen
                .export(&output, &ExportControl::default(), |_| {})
                .unwrap();
            let probe = crate::av::probe(&output, None, None, &|| Ok(())).unwrap();
            assert_eq!(probe.asset.frame_count, 6);
            assert_eq!(probe.asset.video_end_us, 200_000);
            assert_eq!(probe.asset.color_standard, 1);
            assert_eq!(probe.asset.color_range, 2);
            let timestamps = probe.timestamps.clone();
            let mut decoder =
                crate::av::Decoder::open(&output, probe.asset, probe.timestamps).unwrap();
            let mut frames = Vec::new();
            let mut comparisons = Vec::new();
            for (index, pts) in timestamps.into_iter().enumerate() {
                let pixels = decoder
                    .frame(pts, &|| Ok(()))
                    .unwrap()
                    .rgba()
                    .unwrap()
                    .into_owned();
                let expected = dense_movie_color(precision, index as u32, exposure);
                assert_eq!(pixels.len(), 32 * 32 * 4);
                for pixel in pixels.chunks_exact(4) {
                    for c in 0..3 {
                        assert!(
                            pixel[c].abs_diff(expected[c]) <= 10,
                            "{precision:?} exposure={exposure} frame={index} actual={pixel:?} expected={expected:?}"
                        );
                    }
                    assert_eq!(pixel[3], 255);
                }
                comparisons.push(
                    serde_json::json!({"frame":index,"pts":pts,"expected":expected,
                    "actualCenter":&pixels[(16*32+16)*4..(16*32+16)*4+4]}),
                );
                frames.push(pixels);
            }
            assert_eq!(frames.len(), 6);
            report.push(serde_json::json!({"precision":format!("{precision:?}"),"exposure":exposure,
                "frames":comparisons,"frozenRevision":revision,"liveEffectRemovedBeforeExport":true}));
            if let Some(directory) = std::env::var_os("MOTION_PRECISION_OUTPUT_ARTIFACTS") {
                let directory = PathBuf::from(directory);
                fs::create_dir_all(&directory).unwrap();
                let name = format!("{precision:?}-dense-exposure{exposure}");
                fs::copy(&output, directory.join(format!("{name}.mp4"))).unwrap();
                for (index, pixels) in frames.iter().enumerate() {
                    image::save_buffer(
                        directory.join(format!("{name}-frame{index}.png")),
                        pixels,
                        32,
                        32,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        }
    }
    if let Some(directory) = std::env::var_os("MOTION_PRECISION_OUTPUT_ARTIFACTS") {
        fs::write(
            PathBuf::from(directory).join("dense-mp4-report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
}

fn dense_movie_color(mode: ChannelPrecision, frame: u32, exposure: bool) -> [u8; 4] {
    let count = if exposure { 4 } else { 1 };
    let mut hits = 0;
    let mut sum = 0.;
    for i in 0..count {
        let time = f64::from(frame)
            + if exposure {
                -0.25 + (f64::from(i) + 0.5) * 0.125
            } else {
                0.
            };
        if (0. ..6.).contains(&time) {
            hits += 1;
            sum += time.min(4.);
        }
    }
    assert!(hits > 0);
    dense_fixture::color(
        mode,
        sum / f64::from(hits),
        f64::from(hits) / f64::from(count),
    )
}
