use motion_core::{
    AudioAsset, AudioClip, AudioLevels, Command, Content, Ease, Engine, Layer, Project,
};
use motion_media::AudioMixer;
use std::{fs, io::Write};

#[test]
fn stereo_balance_applies_to_mono_and_stereo_without_rewriting_envelopes_or_frozen_mix() {
    for channels in [1, 2] {
        let (root, mut project) = fixture(channels);
        let mut levels = AudioLevels::default();
        levels.left.upsert(0, 0., Ease::Linear).unwrap();
        levels.left.upsert(24, -12., Ease::Linear).unwrap();
        project.layers[0].audio_levels = Some(levels);
        let mut frozen = AudioMixer::new(project.clone(), root.path()).unwrap();
        let mut engine = Engine::new(project).unwrap();
        let mut original = vec![0.; 256];
        frozen.mix(24_000, &mut original).unwrap();
        for balance in [-1., -0.5, 0., 0.5, 1.] {
            engine.apply(Command::AudioLevels { object: 1, action: motion_core::AudioLevelsAction::Balance { value: balance } }).unwrap();
            let mut mixer = AudioMixer::new(engine.snapshot(), root.path()).unwrap();
            let mut actual = vec![0.; 256];
            mixer.mix(24_000, &mut actual).unwrap();
            let gains = [1. - balance.max(0.), 1. + balance.min(0.)];
            for (before, after) in original.chunks_exact(2).zip(actual.chunks_exact(2)) {
                for ch in 0..2 { assert!((after[ch] - before[ch] * gains[ch]).abs() < 1e-7); }
            }
            let mut unchanged = vec![0.; 256];
            frozen.mix(24_000, &mut unchanged).unwrap();
            assert_eq!(unchanged, original);
        }
        engine.apply(Command::SetAudio { object: 1, volume: None, muted: Some(true) }).unwrap();
        let mut muted = AudioMixer::new(engine.snapshot(), root.path()).unwrap();
        let mut actual = vec![1.; 256];
        muted.mix(24_000, &mut actual).unwrap();
        assert_eq!(actual, vec![0.; 256]);
    }
}

#[test]
fn nested_reference_balance_multiplies_each_voice_without_changing_its_channel_keys() {
    let (root, mut project) = fixture(2);
    let mut levels = AudioLevels::default(); levels.balance = 0.5;
    project.layers[0].audio_levels = Some(levels);
    let mut engine = Engine::new(project).unwrap();
    engine.apply(Command::Composition { action: motion_core::CompositionAction::Precompose {
        objects:vec![1],name:"Audio child".into(),range:"composition".into(),
    }}).unwrap();
    let mut project = engine.snapshot();
    project.layers[0].audio_levels = Some(AudioLevels::default());
    project.layers[0].audio_levels.as_mut().unwrap().balance = -0.25;
    let mut mixer = AudioMixer::new(project, root.path()).unwrap();
    let mut out = vec![0.; 256]; mixer.mix(0, &mut out).unwrap();
    for frame in out.chunks_exact(2) {
        assert!((frame[0] - 0.025 * 0.5).abs() < 1e-7);
        assert!((frame[1] + 0.025 * 0.75).abs() < 1e-7);
    }
}

#[test]
fn channel_separation_keeps_pcm_sum_and_isolates_each_track_with_frozen_export() {
    let (root, mut project) = fixture(2);
    let mut levels = AudioLevels::default();
    levels.left.upsert(0, -3., Ease::Linear).unwrap();
    levels.left.upsert(24, -12., Ease::Linear).unwrap();
    levels.right.value = -6.;
    project.layers[0].audio_levels = Some(levels);
    let mut frozen = AudioMixer::new(project.clone(), root.path()).unwrap();
    let mut engine = Engine::new(project).unwrap();
    engine.apply(Command::SplitAudioChannels { object: 1 }).unwrap();
    let separated = engine.snapshot();
    let mut split = AudioMixer::new(separated.clone(), root.path()).unwrap();
    let mut left = separated.clone();
    left.layers.retain(|l| l.audio_source_channel == motion_core::AudioSourceChannel::Left);
    let mut right = separated;
    right.layers.retain(|l| l.audio_source_channel == motion_core::AudioSourceChannel::Right);
    let mut left = AudioMixer::new(left, root.path()).unwrap();
    let mut right = AudioMixer::new(right, root.path()).unwrap();
    for begin in [0, 24000, 48000, 96000] {
        let mut original = vec![0.; 2000];
        let mut combined = original.clone();
        let mut l = original.clone();
        let mut r = original.clone();
        frozen.mix(begin, &mut original).unwrap();
        split.mix(begin, &mut combined).unwrap();
        left.mix(begin, &mut l).unwrap();
        right.mix(begin, &mut r).unwrap();
        assert_eq!(original, combined);
        assert!(l.chunks_exact(2).all(|s| s[1] == 0.));
        assert!(r.chunks_exact(2).all(|s| s[0] == 0.));
        for i in 0..l.len() { assert_eq!(l[i] + r[i], original[i]); }
    }
}

fn fixture(channels: u32) -> (tempfile::TempDir, Project) {
    let root = tempfile::tempdir().unwrap();
    let cache=root.path().join(format!("cache/audio-v{}/tone",motion_media::PCM_VERSION));
    fs::create_dir_all(&cache).unwrap();
    let mut file = fs::File::create(cache.join("decoded.pcm")).unwrap();
    for _ in 0..144000 {
        for channel in 0..channels {
            file.write_all(&(if channel == 0 { 0.025f32 } else { -0.025f32 }).to_le_bytes())
                .unwrap();
        }
    }
    let mut p = Project::new(64, 64, 24, 72).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
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
