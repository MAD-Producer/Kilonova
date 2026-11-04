use super::*;
use motion_core::FrameRate;

#[test]
fn cached_original_pixels_keep_exact_rate_metadata_and_fractional_local_clock_on_seeks() {
    let folder = tempfile::tempdir().unwrap();
    let mut project = image_project(folder.path());
    let mut reader = PreviewInputReader::new(Arc::new(PlatformStub));
    let mut retained = None;
    let mut sequence = 0;
    for (revision, [numerator, denominator]) in [
        [24, 1],
        [24000, 1001],
        [30000, 1001],
        [30000, 1003],
        [60000, 1001],
    ]
    .into_iter()
    .enumerate()
    {
        project.fps = FrameRate::new(numerator, denominator).unwrap();
        project.validate().unwrap();
        let before = project.clone();
        for frame in [15.125, 15.25, 50.5, 15.125] {
            sequence += 1;
            let revision = revision as u64;
            let info = reader
                .request(
                    &project,
                    folder.path(),
                    revision,
                    request(
                        sequence,
                        InputSource::Layer {
                            composition: "comp-main".into(),
                            object: 3,
                        },
                        frame,
                    ),
                )
                .unwrap();
            let local_frame = frame - 15.;
            assert_eq!(info.clock.composition_frame.to_bits(), frame.to_bits());
            assert_eq!(info.clock.local_frame.to_bits(), local_frame.to_bits());
            assert_eq!(info.clock.fps.ratio(), [numerator, denominator]);
            // Independent multiply/divide oracle; the source reader's clock must
            // not use the previous request's rate or quantize to an integer frame.
            let seconds = local_frame * f64::from(denominator) / f64::from(numerator);
            assert!((info.clock.seconds - seconds).abs() < 1e-12);
            assert_eq!(
                serde_json::to_value(&info).unwrap()["clock"]["fps"],
                serde_json::json!({"numerator":numerator,"denominator":denominator})
            );
            let packet = ready(&mut reader, sequence, revision);
            assert_eq!(packet.info.sequence, sequence);
            assert_eq!(packet.info.revision, revision);
            assert_eq!(
                packet.info.clock.local_frame.to_bits(),
                local_frame.to_bits()
            );
            assert_eq!(packet.info.clock.fps, project.fps);
            assert!((packet.info.clock.seconds - seconds).abs() < 1e-12);
            let InputPixels::Image(pixels) = packet.pixels else {
                panic!("actual original PNG required");
            };
            assert_eq!((pixels.width, pixels.height), (5, 8));
            assert_eq!(pixels.rgba[3], 128);
            assert!((181..195).contains(&pixels.rgba[0]));
            if let Some(retained) = &retained {
                assert!(Arc::ptr_eq(retained, &pixels));
            } else {
                retained = Some(pixels);
            }
            assert_eq!(project, before);
        }
    }
}
