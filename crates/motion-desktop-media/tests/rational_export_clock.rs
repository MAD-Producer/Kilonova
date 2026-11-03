#![cfg(feature = "ffmpeg")]
//! Native mux/demux proof; no GPU, wall clock, external command, or reference asset.
use ffmpeg_next::{format, media};
use motion_core::FrameRate;
use motion_desktop_media::encode::Mp4Encoder;

#[test]
fn fractional_rates_survive_native_mp4_timestamps_and_audio_boundaries() {
    let root = tempfile::tempdir().unwrap();
    for (numerator, denominator) in [(24, 1), (24000, 1001), (30000, 1001), (60000, 1001)] {
        let fps = FrameRate::new(numerator, denominator).unwrap();
        let path = root.path().join(format!("{numerator}-{denominator}.mp4"));
        let frames = 37u32;
        let expected_samples = u64::from(frames) * 48_000 * u64::from(denominator)
            / u64::from(numerator);
        let mut encoder = Mp4Encoder::new(&path, 32, 32, fps).unwrap();
        for index in 0..frames {
            let color = [index as u8 * 3, 96, 192, 255];
            encoder.video_frame(&color.repeat(32 * 32), index).unwrap();
        }
        let mut sample = 0;
        while sample < expected_samples {
            let count = encoder.audio_frame_size().min((expected_samples - sample) as usize);
            encoder.audio_frame(&vec![0.125; count * 2], sample).unwrap();
            sample += count as u64;
        }
        encoder.finish().unwrap();

        let mut input = format::input(&path).unwrap();
        let video = input.streams().best(media::Type::Video).unwrap();
        let rate = video.avg_frame_rate();
        assert_eq!(i64::from(rate.numerator()) * i64::from(denominator),
            i64::from(rate.denominator()) * i64::from(numerator), "rate={fps}");
        let video_index = video.index();
        let timebase = video.time_base();
        assert_eq!(video.frames(), i64::from(frames));
        let audio = input.streams().best(media::Type::Audio).unwrap();
        assert_eq!(audio.time_base().numerator(), 1);
        assert_eq!(audio.time_base().denominator(), 48_000);
        assert_eq!(audio.duration(), expected_samples as i64, "audio tail rate={fps}");

        let mut count = 0;
        let mut packets = Vec::new();
        for (stream, packet) in input.packets() {
            if stream.index() != video_index { continue; }
            let pts = packet.pts().unwrap();
            assert_eq!(packet.dts(), Some(pts), "no reordered pictures rate={fps}");
            // Compare rational seconds by cross multiplication, never f64.
            assert_eq!(i128::from(pts) * i128::from(timebase.numerator()) * i128::from(numerator),
                i128::from(count) * i128::from(denominator) * i128::from(timebase.denominator()),
                "PTS frame={count} rate={fps}");
            assert_eq!(i128::from(packet.duration()) * i128::from(timebase.numerator()) * i128::from(numerator),
                i128::from(denominator) * i128::from(timebase.denominator()),
                "duration frame={count} rate={fps}");
            packets.push(serde_json::json!({"pts":pts,"duration":packet.duration()}));
            count += 1;
        }
        assert_eq!(count, frames);
        if let Some(destination) = std::env::var_os("MOTION_RATE_CLOCK_EVIDENCE_DIR") {
            let destination = std::path::PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            std::fs::copy(&path, destination.join(path.file_name().unwrap())).unwrap();
            let evidence = serde_json::json!({"fps":fps,"videoFrames":frames,
                "audioSamples":expected_samples,"videoTimebase":[timebase.numerator(),timebase.denominator()],
                "packets":packets,"scope":"Native MP4 encoder and demux clock; no GPU or complete FrozenMovie proof"});
            std::fs::write(destination.join(format!("{numerator}-{denominator}.json")),
                serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        }
    }
}
