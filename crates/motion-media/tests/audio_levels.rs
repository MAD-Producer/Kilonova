use motion_core::{
    AudioAsset, AudioClip, AudioLevels, Command, Content, Ease, Engine, Layer, Project,
};
use motion_media::AudioMixer;
use std::{fs, io::Write};

fn fixture(channels: u32) -> (tempfile::TempDir, Project) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("cache/audio-v1")).unwrap();
    let mut file = fs::File::create(root.path().join("cache/audio-v1/tone.pcm")).unwrap();
    for _ in 0..144000 {
        for channel in 0..channels {
            file.write_all(&(if channel == 0 { 0.025f32 } else { -0.025f32 }).to_le_bytes())
                .unwrap();
        }
    }
    let mut p = Project::new(64, 64, 24, 72).unwrap();
    p.version = 14;
    p.audio_assets.push(AudioAsset {
        id: 1,
        path: "assets/tone.wav".into(),
        bytes: 64,
        mime: "audio/wav".into(),
        track: 0,
        sample_rate: 48000,
        channels,
        sample_frames: 144000,
        duration_us: 3_000_000,
    });
    let mut layer = Layer::solid(1, "sound", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    p.layers.push(layer);
    (root, p)
}
#[test]
fn db_interpolates_before_conversion_and_each_output_sample_uses_its_own_gain() {
    let (root, mut p) = fixture(2);
    let mut levels = AudioLevels::default();
    levels.left.upsert(0, 0., Ease::Linear).unwrap();
    levels.left.upsert(24, -12., Ease::Linear).unwrap();
    levels.right.value = -12.;
    p.layers[0].audio_levels = Some(levels);
    let mut mixer = AudioMixer::new(p, root.path()).unwrap();
    let mut out = vec![0.; 9600];
    mixer.mix(24000, &mut out).unwrap();
    for frame in [0, 1, 1700, 4799] {
        let db = -12. * (24000 + frame) as f32 / 48000.;
        assert!((out[frame * 2] - 0.025 * 10f32.powf(db / 20.)).abs() < 1e-7);
        assert!((out[frame * 2 + 1] + 0.025 * 10f32.powf(-12. / 20.)).abs() < 1e-7);
    }
    assert!(
        (out[0] - 0.025 * 0.625).abs() > 0.001,
        "must not interpolate linear amplitudes"
    );
}
#[test]
fn mono_source_maps_independent_levels_to_both_output_channels() {
    let (root, mut p) = fixture(1);
    let mut levels = AudioLevels::default();
    levels.left.value = 24.;
    levels.right.value = -6.;
    p.layers[0].audio_levels = Some(levels);
    let mut mixer = AudioMixer::new(p, root.path()).unwrap();
    let mut out = vec![0.; 64];
    mixer.mix(2000, &mut out).unwrap();
    assert!((out[0] - 0.025 * 10f32.powf(24. / 20.)).abs() < 1e-7);
    assert!((out[1] - 0.025 * 10f32.powf(-6. / 20.)).abs() < 1e-7);
}
#[test]
fn seek_block_boundaries_splits_and_frozen_edit_snapshots_preserve_the_envelope() {
    let (root, mut p) = fixture(2);
    let mut levels = AudioLevels::default();
    levels.left.upsert(0, 0., Ease::Hold).unwrap();
    levels.left.upsert(24, -39., Ease::Linear).unwrap();
    levels.right = levels.left.clone();
    p.layers[0].audio_levels = Some(levels);
    let mut frozen = AudioMixer::new(p.clone(), root.path()).unwrap();
    let mut e = Engine::new(p).unwrap();
    e.apply(Command::SplitLayerClip {
        object: 1,
        frame: 12,
    })
    .unwrap();
    let mut split = AudioMixer::new(e.snapshot(), root.path()).unwrap();
    let mut a = vec![0.; 4800];
    let mut b = a.clone();
    for begin in [47950, 10000, 24000, 47950] {
        frozen.mix(begin, &mut a).unwrap();
        split.mix(begin, &mut b).unwrap();
        assert_eq!(a, b);
    }
    assert!(a[98] > 0.024);
    assert!(a[100] < 0.001, "hold key switches at exact output sample");
    e.apply(Command::AudioLevels {
        object: 1,
        action: motion_core::AudioLevelsAction::Set {
            channel: motion_core::AudioChannel::Left,
            frame: 0,
            value: -192.,
            animated: Some(false),
        },
    })
    .unwrap();
    let mut changed = AudioMixer::new(e.snapshot(), root.path()).unwrap();
    frozen.mix(10000, &mut a).unwrap();
    changed.mix(10000, &mut b).unwrap();
    assert_eq!(a[0], 0.025);
    assert!(b[0] < 1e-9);
    assert_eq!(a[1], b[1]);
}
