use super::*;
use motion_core::{AudioAsset, AudioClip, Content, Layer};
use std::io::Write;

fn mixer(root: &Path) -> AudioMixer {
    let mut project = Project::new(32, 32, 240, 1).unwrap();
    let asset = AudioAsset {
        id: 1,
        path: "assets/ramp.wav".into(),
        mime: "audio/wav".into(),
        bytes: 1,
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 200,
        duration_us: 4166,
    };
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join(&asset.path), [0]).unwrap();
    let path = motion_media::cache_path(root, &asset).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut pcm = std::fs::File::create(path).unwrap();
    for sample in 0..200 {
        for value in [sample as f32 / 200., -(sample as f32) / 200.] {
            pcm.write_all(&value.to_le_bytes()).unwrap();
        }
    }
    drop(pcm);
    let mut layer = Layer::solid(1, "Ramp", [0.; 2], [0.; 3], [0.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    project.layers.push(layer);
    project.audio_assets.push(asset);
    AudioMixer::new(project, root).unwrap()
}

#[test]
fn looping_reuses_the_same_mixer_and_wraps_both_resampling_taps_at_the_selected_edge() {
    let root = tempfile::tempdir().unwrap();
    for rate in [44_100, 48_000, 96_000] {
        let mut producer = Producer::in_range(mixer(root.path()), 7, rate, 5..11, true).unwrap();
        for output in 0..1000u64 {
            let numerator = output * 48_000;
            let index = 7 + numerator / u64::from(rate);
            let a = 5 + (index - 5) % 6;
            let b = 5 + (index + 1 - 5) % 6;
            let fraction = (numerator % u64::from(rate)) as f32 / rate as f32;
            let expected = (a as f32 + (b as f32 - a as f32) * fraction) / 200.;
            let actual = producer.next().unwrap().unwrap();
            assert!(
                (actual[0] - expected).abs() < 1e-6,
                "rate={rate} output={output}: {actual:?} expected={expected}"
            );
            assert!((actual[1] + expected).abs() < 1e-6);
        }
    }
}

#[test]
fn non_looping_audio_stops_at_the_exclusive_edge_and_rejects_invalid_plans() {
    let root = tempfile::tempdir().unwrap();
    let mut producer = Producer::in_range(mixer(root.path()), 7, 48_000, 5..11, false).unwrap();
    for sample in 7..11 {
        assert_eq!(
            producer.next().unwrap().unwrap(),
            [sample as f32 / 200., -(sample as f32) / 200.]
        );
    }
    assert_eq!(producer.next().unwrap(), None);
    assert_eq!(producer.next().unwrap(), None);
    for (start, rate, range) in [
        (7, 48_000, 5..5),
        (7, 48_000, 5..201),
        (4, 48_000, 5..11),
        (11, 48_000, 5..11),
        (7, 0, 5..11),
    ] {
        assert!(Producer::in_range(mixer(root.path()), start, rate, range, true).is_err());
    }
}
