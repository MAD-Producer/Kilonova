use super::*;

#[cfg(target_os = "windows")]
mod native;

fn state(kind: &str) -> Value {
    serde_json::json!({"composition":"comp-child","revision":41,
        "capabilities":{"layer_switches":{"supported":true}},
        "project":{"layers":[{"id":7,"locked":false,"solo":false,"effects_enabled":true,
            "content":{"kind":kind,"audio":{"muted":false},"video":{"asset":1,"muted":false},"clip":{"muted":false}}}],
            "video_assets":[{"id":1,"audio_asset":2}]}})
}
fn columns(width: f32) -> ColumnRects {
    ColumnRects::with_output_switches(
        &PaintList::default().styles(),
        0.,
        width,
        Default::default(),
        Default::default(),
        true,
        true,
    )
}
fn click(state: &Value, column: usize, locale: Locale, mut input: Input) -> Vec<Action> {
    let plan = columns(640.);
    let rect = plan.playback[column].unwrap().rect(0., 24.);
    input.mouse = [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ];
    input.pressed = Some((MouseButton::Left, input.mouse));
    let mut list = PaintList::default();
    let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
    let mut out = vec![];
    row(
        &mut list,
        &mut atlas,
        &plan,
        &state["project"]["layers"][0],
        state,
        0.,
        24.,
        &input,
        &mut out,
    );
    out
}

#[test]
fn bilingual_switches_capture_the_same_revision_composition_and_single_typed_edit() {
    for locale in [Locale::Zh, Locale::En] {
        for (index, switch, enabled) in [
            (0, LayerSwitch::Solo, true),
            (1, LayerSwitch::Effects, false),
            (2, LayerSwitch::Audio, false),
        ] {
            let out = click(&state("video"), index, locale, Input::default());
            let [Action::EditChecked {
                command:
                    Command::InComposition {
                        composition,
                        command,
                    },
                revision,
            }] = out.as_slice()
            else {
                panic!("unexpected actions: {out:?}")
            };
            assert_eq!(*revision, 41);
            assert_eq!(composition, "comp-child");
            assert!(
                matches!(**command, Command::SetLayerSwitch {object:7,switch:s,enabled:v} if s==switch && v==enabled)
            );
        }
    }
}

#[test]
fn observation_camera_does_not_disable_coordinate_independent_output_switches() {
    let mut state = state("video");
    state["observing"] = true.into();
    for index in 0..3 {
        assert_eq!(click(&state, index, Locale::Zh, Input::default()).len(), 1);
    }
}

#[test]
fn unavailable_types_locked_layers_and_cancelled_inputs_emit_no_edits() {
    for kind in ["camera", "null"] {
        for index in 0..3 {
            assert!(click(&state(kind), index, Locale::Zh, Input::default()).is_empty());
        }
    }
    assert!(click(&state("audio"), 1, Locale::En, Input::default()).is_empty());
    assert!(click(&state("solid"), 2, Locale::En, Input::default()).is_empty());
    let mut silent_video = state("video");
    silent_video["project"]["video_assets"][0]["audio_asset"] = Value::Null;
    assert!(click(&silent_video, 2, Locale::Zh, Input::default()).is_empty());
    for index in 0..3 {
        let mut locked = state("video");
        locked["project"]["layers"][0]["locked"] = true.into();
        assert!(click(&locked, index, Locale::Zh, Input::default()).is_empty());
        for input in [
            Input {
                focus_lost: true,
                ..Default::default()
            },
            Input {
                composing: true,
                ..Default::default()
            },
            Input {
                ime_consumed: true,
                ..Default::default()
            },
        ] {
            assert!(click(&state("video"), index, Locale::En, input).is_empty());
        }
    }
}

#[test]
fn sampling_capability_loss_does_not_reassign_old_hit_regions_to_output_switches() {
    let styles = PaintList::default().styles();
    for width in [320., 468., 640., 900.] {
        let present = ColumnRects::with_output_switches(
            &styles,
            0.,
            width,
            Default::default(),
            Default::default(),
            true,
            true,
        );
        let absent = ColumnRects::with_output_switches(
            &styles,
            0.,
            width,
            Default::default(),
            Default::default(),
            false,
            true,
        );
        assert_eq!(present.playback, absent.playback);
        assert_eq!(present.name, absent.name);
        assert!(absent.sampling.iter().all(Option::is_none));
        assert!(present.sampling.iter().flatten().all(|old| absent
            .playback
            .iter()
            .flatten()
            .all(|new| old.max_x() <= new.x || new.max_x() <= old.x)));
    }
}

#[test]
fn switch_slots_keep_the_existing_hit_size_without_overlapping_columns() {
    for width in [320., 468., 640., 900.] {
        let plan = columns(width);
        let mut slots: Vec<_> = plan
            .playback
            .iter()
            .chain(plan.sampling.iter())
            .filter_map(|slot| *slot)
            .collect();
        slots.extend(
            [
                plan.shy,
                plan.three_d,
                plan.matte,
                plan.blend,
                plan.link,
                plan.parent,
            ]
            .into_iter()
            .flatten(),
        );
        slots.sort_by(|a, b| a.x.total_cmp(&b.x));
        assert_eq!(plan.playback.iter().flatten().count(), 3);
        assert!(plan.name.width >= 64.);
        assert!(slots
            .iter()
            .all(|slot| slot.x >= plan.name.max_x() && slot.max_x() <= width));
        assert!(slots.windows(2).all(|pair| pair[0].max_x() <= pair[1].x));
        let expected = PaintList::default().styles().metric("icon.slot", 20.);
        assert!(plan
            .playback
            .iter()
            .flatten()
            .all(|slot| slot.width == expected));
    }
}

#[test]
fn actual_pcm_keeps_audio_independent_of_video_and_respects_solo_and_mute() {
    use motion_core::{
        AudioAsset, AudioClip, Content, Engine, Layer, Project, VideoAsset, VideoClip,
    };
    let root = tempfile::tempdir().unwrap();
    let mut project = Project::new(64, 64, 30, 30).unwrap();
    let asset = AudioAsset {
        id: 10,
        path: "assets/reference.mp4".into(),
        mime: "audio/mp4".into(),
        bytes: 4,
        track: 1,
        sample_rate: 48000,
        channels: 2,
        sample_frames: 48000,
        duration_us: 1_000_000,
    };
    let path = motion_media::cache_path(root.path(), &asset).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Deterministic decoded-source fixture, not a fake successful mixer: the real
    // mixer opens its PCM cache, plans voices, resamples and writes the block.
    let pcm: Vec<u8> = (0..96000).flat_map(|_| 0.25f32.to_le_bytes()).collect();
    std::fs::write(path, pcm).unwrap();
    project.audio_assets.push(asset);
    project.video_assets.push(VideoAsset {
        id: 20,
        path: "assets/reference.mp4".into(),
        bytes: 4,
        mime: "video/avc".into(),
        track: 0,
        width: 64,
        height: 64,
        rotation: 0,
        display_width: 64,
        display_height: 64,
        video_start_us: 0,
        video_end_us: 1_000_000,
        duration_us: 1_000_000,
        frame_count: 30,
        variable_frame_rate: false,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 2,
        audio_asset: Some(10),
    });
    let mut video = Layer::solid(1, "Video with audio", [64.; 2], [32., 32., 0.], [1.; 4]);
    video.content = Content::Video {
        video: VideoClip::new(20),
    };
    let mut audio = Layer::solid(2, "Audio", [0.; 2], [0.; 3], [0.; 4]);
    audio.content = Content::Audio {
        audio: AudioClip::new(10),
    };
    project.layers = vec![video, audio];
    let mut engine = Engine::new(project).unwrap();
    let check = |engine: &Engine, expected: f32| {
        let mut out = [f32::NAN; 512];
        let mut mixer = motion_media::AudioMixer::new(engine.snapshot(), root.path()).unwrap();
        assert_eq!(mixer.mix(4800, &mut out).unwrap(), 256);
        assert!(
            out.iter().all(|value| (*value - expected).abs() < 0.00001),
            "PCM expected {expected}: {:?}",
            &out[..8]
        );
    };
    check(&engine, 0.5);
    for (object, switch, enabled, expected) in [
        (1, LayerSwitch::Video, false, 0.5),
        (1, LayerSwitch::Audio, false, 0.25),
        (1, LayerSwitch::Video, true, 0.25),
        (1, LayerSwitch::Solo, true, 0.),
        (1, LayerSwitch::Audio, true, 0.25),
        (1, LayerSwitch::Solo, false, 0.5),
    ] {
        engine
            .apply(Command::SetLayerSwitch {
                object,
                switch,
                enabled,
            })
            .unwrap();
        check(&engine, expected);
    }
    assert!(engine.undo().unwrap());
    check(&engine, 0.25);
}
