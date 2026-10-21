#![cfg(feature = "ffmpeg")]
use motion_desktop_media::av::{probe, Decoder};
use motion_host::platform::VideoDecoder;
use std::path::{Path, PathBuf};

#[path = "libav/native_retirement.rs"]
mod native_retirement;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../motion-media/tests/fixtures/video")
        .join(name)
}

#[test]
fn missing_video_keeps_original_ffmpeg_error_at_the_media_boundary() {
    let root = tempfile::tempdir().unwrap();
    let error = match probe(&root.path().join("missing.mp4"), None, None, &|| Ok(())) {
        Ok(_) => panic!("missing video was accepted"),
        Err(error) => error,
    };
    assert!(error.to_string().starts_with("cannot open video: "));
    let motion_media::Error::Backend { backend, cause } = error.primary() else {
        panic!("FFmpeg failure was flattened: {error:?}");
    };
    assert_eq!(*backend, "ffmpeg");
    let native = cause.downcast_ref::<ffmpeg_next::Error>().unwrap();
    assert!(matches!(native, ffmpeg_next::Error::Other { errno } if *errno == 2));
}

#[test]
fn leading_edits_and_ctts_have_exact_tail_intervals_without_repeating_the_last_frame() {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture("manifest.json")).unwrap()).unwrap();
    for name in ["video-delayed.mp4", "video-ctts.mp4"] {
        let probed = probe(&fixture(name), None, Some(u32::MAX), &|| Ok(())).unwrap();
        let expected_end = manifest[name]["video_end_us"].as_u64().unwrap();
        let expected_pts: Vec<u64> = manifest[name]["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| {
                (frame["best_effort_timestamp_time"]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
                    * 1_000_000.0)
                    .round() as u64
            })
            .collect();
        assert!(probed.asset.video_start_us > 0, "{name}");
        assert_eq!(probed.timestamps, expected_pts, "{name}");
        assert_eq!(probed.asset.frame_count, 72, "{name}");
        assert_eq!(probed.asset.video_end_us, expected_end, "{name}");
        assert_eq!(probed.asset.duration_us, expected_end, "{name}");
        let mut decoder =
            Decoder::open(&fixture(name), probed.asset.clone(), probed.timestamps).unwrap();
        let tail = decoder.frame(expected_end - 1, &|| Ok(())).unwrap();
        assert_eq!(tail.pts, *expected_pts.last().unwrap(), "{name}");
        assert_eq!(tail.end, expected_end, "{name}");
        assert!(
            decoder.frame(expected_end, &|| Ok(())).is_err(),
            "{name} repeated past end"
        );
        assert!(
            decoder
                .frame(probed.asset.video_start_us - 1, &|| Ok(()))
                .is_err(),
            "{name}"
        );
    }
}

#[test]
fn all_visible_frames_match_reference_pts_and_scrub_in_both_directions() {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture("manifest.json")).unwrap()).unwrap();
    for name in [
        "silent-24fps.mp4",
        "sound-24fps.mp4",
        "audio-delayed.mp4",
        "variable.mp4",
        "rotated-90.mp4",
    ] {
        let path = fixture(name);
        let probed = probe(&path, None, None, &|| Ok(())).unwrap();
        probed.asset.validate().unwrap();
        let expected: Vec<u64> = manifest[name]["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (f["best_effort_timestamp_time"]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
                    * 1_000_000.0)
                    .round() as u64
            })
            .collect();
        assert_eq!(
            probed.timestamps, expected,
            "{name} must include B/P frames"
        );
        assert_eq!(
            probed.first_rgba.len(),
            probed.asset.display_width as usize * probed.asset.display_height as usize * 4
        );
        assert_eq!(
            probed.asset.rotation,
            if name == "rotated-90.mp4" { 90 } else { 0 }
        );
        assert_eq!(probed.asset.variable_frame_rate, name == "variable.mp4");
        assert_eq!(
            probed.audio_track.is_some(),
            matches!(name, "sound-24fps.mp4" | "audio-delayed.mp4")
        );
        let mut decoder =
            Decoder::open(&path, probed.asset.clone(), probed.timestamps.clone()).unwrap();
        for index in (0..expected.len()).chain((0..expected.len()).rev()).chain([
            0,
            expected.len() - 1,
            expected.len() / 2,
        ]) {
            let frame = decoder.frame(expected[index], &|| Ok(())).unwrap();
            assert_eq!(frame.pts, expected[index], "{name} frame {index}");
            assert_eq!(
                frame.end,
                expected
                    .get(index + 1)
                    .copied()
                    .unwrap_or(probed.asset.video_end_us)
            );
            assert_eq!(frame.rgba().unwrap().len(), probed.first_rgba.len());
            assert!(matches!(
                frame.pixels,
                motion_host::video_frame::VideoPixels::Yuv(_)
            ));
        }
        assert!(decoder.seeks() > 0);
    }
}

#[test]
fn fragmented_compressed_and_fractional_short_tail_use_exact_sample_intervals() {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture("manifest.json")).unwrap()).unwrap();
    for name in [
        "video-fragmented.mp4",
        "video-tail-90k.mp4",
        "video-tail-cmov.mov",
    ] {
        let expected = &manifest[name];
        let expected_end = expected["video_end_us"].as_u64().unwrap();
        let probed = probe(&fixture(name), None, Some(u32::MAX), &|| Ok(())).unwrap();
        let pts: Vec<u64> = expected["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| {
                (frame["best_effort_timestamp_time"]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
                    * 1_000_000.0)
                    .round() as u64
            })
            .collect();
        assert_eq!(probed.timestamps, pts, "{name}");
        assert_eq!(probed.asset.video_end_us, expected_end, "{name}");
        assert_eq!(probed.asset.duration_us, expected_end, "{name}");
        if name != "video-fragmented.mp4" {
            assert!(probed.asset.variable_frame_rate, "{name}");
            assert_eq!(expected_end, 3_243_356);
            assert_eq!(probed.asset.video_start_us, 250_000);
        }
        let mut decoder = Decoder::open(&fixture(name), probed.asset, probed.timestamps).unwrap();
        let tail = decoder.frame(expected_end - 1, &|| Ok(())).unwrap();
        assert_eq!(tail.pts, *pts.last().unwrap(), "{name}");
        assert_eq!(tail.end, expected_end, "{name}");
        assert!(decoder.frame(expected_end, &|| Ok(())).is_err(), "{name}");
    }
}

#[test]
fn stream_selection_and_cancel_do_not_silently_fall_back() {
    let path = fixture("sound-24fps.mp4");
    assert!(probe(&path, Some(99), None, &|| Ok(())).is_err());
    assert!(probe(&path, None, Some(99), &|| Ok(())).is_err());
    let probed = probe(&path, None, Some(u32::MAX), &|| Ok(())).unwrap();
    assert_eq!(probed.audio_track, None);
    let mut decoder = Decoder::open(&path, probed.asset, probed.timestamps.clone()).unwrap();
    assert_eq!(
        decoder
            .frame(probed.timestamps[0], &|| Err(
                "video target superseded".into()
            ))
            .err()
            .unwrap().to_string(),
        "video target superseded"
    );
    assert_eq!(
        decoder.frame(probed.timestamps[0], &|| Ok(())).unwrap().pts,
        probed.timestamps[0]
    );
}

#[test]
fn advertised_containers_and_codecs_decode_but_ten_bit_is_rejected() {
    let folder =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../motion-media/tests/fixtures/formats");
    for (name, mime) in [
        ("avc.mov", "video/avc"),
        ("avc.mkv", "video/avc"),
        ("hevc.mp4", "video/hevc"),
        ("hevc.mkv", "video/hevc"),
        ("vp8.webm", "video/x-vnd.on2.vp8"),
        ("vp8-601.webm", "video/x-vnd.on2.vp8"),
        ("vp9.webm", "video/x-vnd.on2.vp9"),
        ("vp9-delayed.webm", "video/x-vnd.on2.vp9"),
    ] {
        let path = folder.join(name);
        let probed = probe(&path, None, Some(u32::MAX), &|| Ok(()))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(probed.asset.mime, mime);
        let mut decoder = Decoder::open(&path, probed.asset, probed.timestamps.clone()).unwrap();
        for time in [
            probed.timestamps[0],
            *probed.timestamps.last().unwrap(),
            probed.timestamps[probed.timestamps.len() / 2],
        ] {
            assert_eq!(
                decoder
                    .frame(time, &|| Ok(()))
                    .unwrap_or_else(|e| panic!("{name} at {time}: {e}"))
                    .pts,
                time
            );
        }
    }
    for name in ["reject-hevc10.mp4", "reject-vp9-10.webm"] {
        assert!(
            probe(&folder.join(name), None, Some(u32::MAX), &|| Ok(())).is_err(),
            "{name}"
        );
    }
}
