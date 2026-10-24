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
