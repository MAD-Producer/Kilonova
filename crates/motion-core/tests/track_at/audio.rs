use super::*;
use motion_core::{AudioAsset, AudioChannel, AudioClip, AudioLevels, Content};

fn project(reverse: bool, animated: bool) -> Project {
    let mut p = Project::new(64, 64, 30, 120).unwrap();
    p.audio_assets.push(AudioAsset {
        id: 1,
        path: "assets/tone.wav".into(),
        mime: "audio/wav".into(),
        bytes: 44 + 48000 * 4 * 4,
        track: 0,
        sample_rate: 48000,
        channels: 2,
        sample_frames: 48000 * 4,
        duration_us: 4_000_000,
    });
    let mut layer = Layer::solid(7, "Mapped audio", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    layer.timeline = Some(LayerTimeline {
        in_frame: 2,
        out_frame: 120,
        offset_frame: 2,
    });
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if reverse { 0. } else { -6. },
        )
        .unwrap(),
        remap: Some(
            motion_core::layer_time::TimeRemap::new(vec![motion_core::layer_time::TimeRemapKey {
                frame: -6.,
                source_frame: 43.,
                easing: Default::default(),
            }])
            .unwrap(),
        ),
    });
    if animated {
        let mut levels = AudioLevels {
            balance: 0.3,
            ..Default::default()
        };
        for (frame, value) in [(-6, -6.), (-3, -12.), (0, -18.)] {
            levels.left.upsert(frame, value, Ease::Linear).unwrap();
            levels
                .right
                .upsert(frame, value * 0.5, Ease::Linear)
                .unwrap();
        }
        layer.audio_levels = Some(levels);
    }
    p.layers.push(layer);
    p
}
fn write_audio(root: &std::path::Path) {
    let length = 48000u32 * 4 * 4;
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + length).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(48000u32.to_le_bytes());
    wav.extend(192000u32.to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(length.to_le_bytes());
    wav.resize(44 + length as usize, 0);
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/tone.wav"), wav).unwrap();
}
fn command(channel: AudioChannel, frame: f64, edit: TrackEdit) -> Command {
    Command::TrackAt {
        target: KeyframeTarget::Audio { object: 7, channel },
        frame,
        edit,
    }
}
fn set(value: f32) -> TrackEdit {
    TrackEdit::Set {
        value: TrackValue::Scalar(value),
    }
}

#[test]
fn audio_at_creates_an_absent_envelope_only_after_valid_preflight_and_never_rounds_key_time() {
    for reverse in [false, true] {
        for channel in [AudioChannel::Left, AudioChannel::Right] {
            let mut e = Engine::new(project(reverse, false)).unwrap();
            let before = e.snapshot();
            for (frame, edit) in [
                (8.75, TrackEdit::ToggleKey),
                (6.5, set(25.)),
                (6.5, set(f32::NAN)),
                (
                    6.5,
                    TrackEdit::Set {
                        value: TrackValue::Vec2([0.; 2]),
                    },
                ),
            ] {
                assert!(e.apply(command(channel, frame, edit)).is_err());
                assert_eq!(e.snapshot(), before);
                assert!(!e.undo().unwrap());
            }
            e.apply(command(channel, 8.75, set(-9.25))).unwrap();
            let mut expected = before.clone();
            let mut levels = AudioLevels::default();
            match channel {
                AudioChannel::Left => levels.left.value = -9.25,
                AudioChannel::Right => levels.right.value = -9.25,
            };
            expected.layers[0].audio_levels = Some(levels);
            assert_eq!(e.snapshot(), expected);
            assert!(e.undo().unwrap());
            assert_eq!(e.snapshot(), before);
            assert!(e.redo().unwrap());
            assert_eq!(e.snapshot(), expected);
            assert!(e.undo().unwrap());
            e.apply(command(channel, 6.5, TrackEdit::Animate { enabled: true }))
                .unwrap();
            let levels = e.project().layers[0].audio_levels.as_ref().unwrap();
            let (active, other) = match channel {
                AudioChannel::Left => (&levels.left, &levels.right),
                AudioChannel::Right => (&levels.right, &levels.left),
            };
            assert_eq!(active.keys.len(), 1);
            assert_eq!(active.keys[0].frame, -3);
            assert_eq!(active.keys[0].value, 0.);
            assert!(other.keys.is_empty());
        }
    }
}

#[test]
fn audio_at_changes_only_the_exact_local_channel_and_preserves_remap_balance_history_and_storage() {
    for reverse in [false, true] {
        let mut e = Engine::new(project(reverse, true)).unwrap();
        let before = e.snapshot();
        e.apply(command(AudioChannel::Left, 6.5, set(-17.25)))
            .unwrap();
        let mut expected = before.clone();
        expected.layers[0].audio_levels.as_mut().unwrap().left.keys[1].value = -17.25;
        assert_eq!(e.snapshot(), expected);
        assert!(e.undo().unwrap());
        assert_eq!(e.snapshot(), before);
        assert!(e.redo().unwrap());
        assert_eq!(e.snapshot(), expected);
        let folder = tempfile::tempdir().unwrap();
        write_audio(folder.path());
        motion_core::storage::save(folder.path(), e.project()).unwrap();
        assert_eq!(motion_core::storage::load(folder.path()).unwrap(), expected);
        let before = e.snapshot();
        assert!(e
            .apply(command(AudioChannel::Left, 8.75, set(-10.)))
            .unwrap_err()
            .to_string()
            .contains("continuous key addressing"));
        assert_eq!(e.snapshot(), before);
        e.apply(command(AudioChannel::Left, 6.5, TrackEdit::ToggleKey))
            .unwrap();
        let mut expected = before.clone();
        expected.layers[0]
            .audio_levels
            .as_mut()
            .unwrap()
            .left
            .keys
            .remove(1);
        assert_eq!(e.snapshot(), expected);
    }
}

#[test]
fn audio_at_disables_animation_at_the_exact_sample_and_refuses_wrong_or_locked_targets() {
    for reverse in [false, true] {
        let mut e = Engine::new(project(reverse, true)).unwrap();
        let before = e.snapshot();
        let layer = &before.layers[0];
        let local = layer
            .time_mapping
            .as_ref()
            .unwrap()
            .clock
            .key_edit_time(8.75, 2)
            .unwrap();
        let expected_value = layer
            .audio_levels
            .as_ref()
            .unwrap()
            .left
            .sample(local)
            .unwrap();
        e.apply(command(
            AudioChannel::Left,
            8.75,
            TrackEdit::Animate { enabled: false },
        ))
        .unwrap();
        let mut expected = before.clone();
        expected.layers[0].audio_levels.as_mut().unwrap().left =
            motion_core::Track::constant(expected_value);
        assert_eq!(e.snapshot(), expected);
        assert!(e.undo().unwrap());
        e.apply(Command::Flags {
            object: 7,
            visible: true,
            locked: true,
        })
        .unwrap();
        let locked = e.snapshot();
        assert!(e
            .apply(command(AudioChannel::Left, 6.5, set(-10.)))
            .is_err());
        assert_eq!(e.snapshot(), locked);
    }
    let mut p = project(false, false);
    p.layers[0].content = Content::Solid { color: [1.; 4] };
    p.layers[0].size = [64.; 2];
    p.layers[0].time_mapping.as_mut().unwrap().remap = None;
    let mut e = Engine::new(p).unwrap();
    let before = e.snapshot();
    assert!(e
        .apply(command(AudioChannel::Left, 6.5, set(-10.)))
        .is_err());
    assert_eq!(e.snapshot(), before);
}
