use motion_media::AudioMixer;
use motion_model::{AudioAsset, AudioClip, ClipRange, Content, Layer, Project};
#[test]
fn pcm_clip_boundaries_use_continuous_time_and_output_sample_ceil() {
    let root = tempfile::tempdir().unwrap();
    let mut p = Project::new(64, 64, 24, 24).unwrap();
    p.version = 15;
    let asset = AudioAsset {
        id: 1,
        path: "assets/tone.wav".into(),
        bytes: 64,
        mime: "audio/wav".into(),
        track: 0,
        sample_rate: 48000,
        channels: 1,
        sample_frames: 48000,
        duration_us: 1_000_000,
    };
    let pcm = motion_media::cache_path(root.path(), &asset).unwrap();
    std::fs::create_dir_all(pcm.parent().unwrap()).unwrap();
    std::fs::write(pcm, 0.25f32.to_le_bytes().repeat(48000)).unwrap();
    p.audio_assets.push(asset);
    let mut l = Layer::solid(1, "audio", [0.; 2], [0.; 3], [1.; 4]);
    l.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    l.set_interval(
        24,
        ClipRange {
            in_frame: 1. / 3.,
            out_frame: 5. / 3.,
        },
    )
    .unwrap();
    p.layers.push(l);
    let voices = p.audio_voices().unwrap();
    assert_eq!(voices[0].begin_sample, 667);
    assert_eq!(voices[0].end_sample, 3334);
    let mut mixer = AudioMixer::new(p, root.path()).unwrap();
    let mut samples = vec![0.; 4000 * 2];
    mixer.mix(0, &mut samples).unwrap();
    assert!(samples[..667 * 2].iter().all(|v| *v == 0.));
    assert!(samples[667 * 2..3334 * 2].iter().all(|v| *v == 0.25));
    assert!(samples[3334 * 2..].iter().all(|v| *v == 0.));
    let mut repeat = vec![0.; 100 * 2];
    mixer.mix(660, &mut repeat).unwrap();
    assert_eq!(repeat, samples[660 * 2..760 * 2]);
}
