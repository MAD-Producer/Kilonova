use motion_core::*;

fn project() -> Project {
    let mut project = Project::new(64, 64, 30, 180).unwrap();
    project.audio_assets.push(AudioAsset {
        id: 1,
        path: "assets/reference.wav".into(),
        mime: "audio/wav".into(),
        bytes: 64,
        track: 0,
        sample_rate: 48000,
        channels: 2,
        sample_frames: 192000,
        duration_us: 4_000_000,
    });
    let mut layer = Layer::solid(7, "Music", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    layer.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 120,
        offset_frame: 10,
    });
    project.layers.push(layer);
    project
}
fn video_project() -> Project {
    let mut project = project();
    project.audio_assets[0].path = "assets/clip.mp4".into();
    project.audio_assets[0].mime = "audio/mp4".into();
    project.audio_assets[0].sample_frames = 96_000;
    project.audio_assets[0].duration_us = 2_000_000;
    project.video_assets.push(VideoAsset {
        id: 2,
        path: "assets/clip.mp4".into(),
        bytes: 64,
        mime: "video/avc".into(),
        track: 1,
        width: 64,
        height: 64,
        rotation: 0,
        display_width: 64,
        display_height: 64,
        video_start_us: 0,
        video_end_us: 4_000_000,
        duration_us: 4_000_000,
        frame_count: 120,
        variable_frame_rate: false,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 1,
        audio_asset: Some(1),
    });
    let mut video = VideoClip::new(2);
    video.volume = 0.75;
    video.source_offset_us = 250_000;
    project.layers[0].content = Content::Video { video };
    project.layers[0].size = [64.; 2];
    project
}
#[test]
fn video_separation_keeps_the_picture_and_trims_a_shorter_audio_track_at_sample_time() {
    let mut engine = Engine::new(video_project()).unwrap();
    edit(
        &mut engine,
        AudioMarkersAction::ReplaceGenerated { frames: vec![40] },
    );
    let before = engine.project().clone();
    engine
        .apply(Command::SplitAudioChannels { object: 7 })
        .unwrap();
    let layers = &engine.project().layers;
    assert_eq!(layers.len(), 3);
    assert!(
        matches!(&layers[0].content, Content::Video { video } if video.muted && video.volume == 0.75)
    );
    assert_eq!(layers[0].timeline, before.layers[0].timeline);
    for layer in &layers[1..] {
        assert_eq!(layer.clip(180).offset_frame, 10);
        assert_eq!(
            layer.interval(180),
            ClipRange {
                in_frame: 10.,
                out_frame: 62.5
            }
        );
        assert!(
            matches!(&layer.content, Content::Audio { audio } if audio.volume == 0.75 && audio.source_offset_us == 250_000)
        );
    }
    let voices = engine.project().audio_voices().unwrap();
    assert!(layers[0].audio_markers.is_empty());
    assert_eq!(layers[1].audio_markers, before.layers[0].audio_markers);
    assert!(layers[2].audio_markers.is_empty());
    assert_eq!(voices.len(), 2);
    assert!(voices.iter().all(|v| v.end_sample == 100_000));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn video_audio_extraction_preserves_sound_and_picture_with_one_step_history_and_save() {
    for channels in [1, 2] {
        let mut project = video_project();
        project.audio_assets[0].channels = channels;
        let mut engine = Engine::new(project).unwrap();
        engine
            .apply(Command::AudioLevels {
                object: 7,
                action: AudioLevelsAction::Set {
                    channel: AudioChannel::Left,
                    frame: 10,
                    value: -6.,
                    animated: Some(true),
                },
            })
            .unwrap();
        edit(
            &mut engine,
            AudioMarkersAction::Add {
                frame: 40,
                label: "Cut".into(),
            },
        );
        let before = engine.project().clone();
        let before_voice = before.audio_voices().unwrap().remove(0);
        let command = Command::ExtractAudio { object: 7 };
        let wire = serde_json::to_string(&command).unwrap();
        engine.apply_batch(parse_commands(&wire).unwrap()).unwrap();
        let after = engine.project().clone();
        assert_eq!(after.layers.len(), 2);
        let mut video = after.layers[0].clone();
        if let Content::Video { video } = &mut video.content {
            assert!(video.muted);
            video.muted = false;
        }
        let mut expected_video = before.layers[0].clone();
        expected_video.audio_markers.clear();
        assert_eq!(video, expected_video);
        let extracted = &after.layers[1];
        assert_eq!(extracted.audio_levels, before.layers[0].audio_levels);
        assert_eq!(extracted.audio_markers, before.layers[0].audio_markers);
        assert_eq!(extracted.audio_source_channel, AudioSourceChannel::Stereo);
        assert_eq!(extracted.clip(180).offset_frame, 10);
        assert_eq!(
            extracted.interval(180),
            ClipRange {
                in_frame: 10.,
                out_frame: 62.5
            }
        );
        assert!(matches!(&extracted.content, Content::Audio { audio }
            if !audio.muted && audio.volume == 0.75 && audio.source_offset_us == 250_000));
        let voices = after.audio_voices().unwrap();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].asset, before_voice.asset);
        assert_eq!(voices[0].offset_sample, before_voice.offset_sample);
        assert_eq!(voices[0].source_offset_us, before_voice.source_offset_us);
        for at in [16_000, 40_000, 99_999] {
            assert_eq!(voices[0].gain_at(at), before_voice.gain_at(at));
        }
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        engine.redo().unwrap();
        assert_eq!(engine.project(), &after);
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("assets")).unwrap();
        std::fs::write(root.path().join("assets/clip.mp4"), [0u8; 64]).unwrap();
        storage::save(root.path(), &after).unwrap();
        assert_eq!(storage::load(root.path()).unwrap(), after);
    }
}

#[test]
fn unavailable_audio_extraction_is_atomic_and_muted_sources_stay_muted() {
    let mut muted = video_project();
    if let Content::Video { video } = &mut muted.layers[0].content {
        video.muted = true;
    }
    let mut engine = Engine::new(muted).unwrap();
    engine.apply(Command::ExtractAudio { object: 7 }).unwrap();
    assert!(matches!(&engine.project().layers[1].content, Content::Audio { audio } if audio.muted));
    let mut locked = video_project();
    locked.layers[0].locked = true;
    for project in [project(), locked] {
        let mut engine = Engine::new(project).unwrap();
        let before = engine.project().clone();
        assert!(engine.apply(Command::ExtractAudio { object: 7 }).is_err());
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
    }
    let mut missing = video_project();
    missing.video_assets[0].audio_asset = None;
    missing.audio_assets.clear();
    let mut empty = video_project();
    empty.layers[0].timeline = Some(LayerTimeline {
        in_frame: 90,
        out_frame: 120,
        offset_frame: 10,
    });
    for project in [missing, empty] {
        let mut engine = Engine::new(project).unwrap();
        let before = engine.project().clone();
        assert!(engine.apply(Command::ExtractAudio { object: 7 }).is_err());
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
    }
}
#[test]
fn reference_times_support_trimmed_long_sources_beyond_composition_frame_limits() {
    let mut project = project();
    project.fps = 120;
    project.frames = 36_000;
    project.audio_assets[0].sample_frames = 48_000 * 3600;
    project.audio_assets[0].duration_us = 3_600_000_000;
    project.layers[0].timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 36_000,
        offset_frame: -380_000,
    });
    let mut engine = Engine::new(project).unwrap();
    edit(
        &mut engine,
        AudioMarkersAction::Add {
            frame: 5000,
            label: String::new(),
        },
    );
    assert_eq!(engine.project().layers[0].audio_markers[0].frame, 385_000);
}
fn edit(engine: &mut Engine, action: AudioMarkersAction) {
    engine
        .apply(Command::AudioMarkers { object: 7, action })
        .unwrap();
}

#[test]
fn reanalysis_keeps_manual_and_corrected_markers_with_atomic_undo_and_save() {
    let mut engine = Engine::new(project()).unwrap();
    edit(
        &mut engine,
        AudioMarkersAction::Add {
            frame: 40,
            label: "Cut".into(),
        },
    );
    edit(
        &mut engine,
        AudioMarkersAction::ReplaceGenerated {
            frames: vec![20, 70, 20],
        },
    );
    assert_eq!(engine.project().layers[0].audio_markers.len(), 3);
    assert_eq!(engine.project().version, 18);
    let first = engine.project().layers[0]
        .audio_markers
        .iter()
        .find(|m| m.generated)
        .unwrap()
        .id;
    edit(
        &mut engine,
        AudioMarkersAction::Move {
            id: first,
            frame: 25,
        },
    );
    let before = engine.project().clone();
    edit(
        &mut engine,
        AudioMarkersAction::ReplaceGenerated {
            frames: vec![30, 90],
        },
    );
    let after = engine.project().clone();
    let markers = &after.layers[0].audio_markers;
    assert_eq!(markers.len(), 4);
    assert!(markers
        .iter()
        .any(|m| m.id == first && m.frame == 15 && m.generated && m.corrected));
    assert!(markers
        .iter()
        .any(|m| m.label == "Cut" && m.frame == 30 && !m.generated));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &after);
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("assets")).unwrap();
    std::fs::write(root.path().join("assets/reference.wav"), [0u8; 64]).unwrap();
    storage::save(root.path(), &after).unwrap();
    assert_eq!(storage::load(root.path()).unwrap(), after);
    edit(&mut engine, AudioMarkersAction::ClearGenerated);
    assert_eq!(engine.project().layers[0].audio_markers.len(), 1);
    assert_eq!(engine.project().layers[0].audio_markers[0].label, "Cut");
    engine.undo().unwrap();
    assert_eq!(engine.project(), &after);
}
#[test]
fn markers_follow_clip_moves_splits_and_frame_rate_changes() {
    let mut engine = Engine::new(project()).unwrap();
    edit(
        &mut engine,
        AudioMarkersAction::ReplaceGenerated {
            frames: vec![20, 70],
        },
    );
    engine
        .apply(Command::MoveLayerClip {
            object: 7,
            in_frame: 30,
        })
        .unwrap();
    let layer = &engine.project().layers[0];
    assert_eq!(
        layer
            .audio_markers
            .iter()
            .map(|m| m.frame)
            .collect::<Vec<_>>(),
        [10, 60]
    );
    assert_eq!(layer.clip(180).offset_frame, 30);
    engine
        .apply(Command::SplitLayerClip {
            object: 7,
            frame: 60,
        })
        .unwrap();
    assert_eq!(
        engine.project().layers[0].audio_markers,
        engine.project().layers[1].audio_markers
    );
    engine
        .apply(Command::Composition {
            action: CompositionAction::Settings {
                settings: CompositionSettings {
                    name: "Retime".into(),
                    width: 64,
                    height: 64,
                    fps: 60,
                    frames: 360,
                    timing: "preserve_seconds".into(),
                    shorten: "reject".into(),
                },
            },
        })
        .unwrap();
    for layer in &engine.project().layers {
        assert_eq!(
            layer
                .audio_markers
                .iter()
                .map(|m| m.frame)
                .collect::<Vec<_>>(),
            [20, 120]
        );
        assert_eq!(layer.clip(360).offset_frame, 60);
    }
}
#[test]
fn rejected_marker_batches_and_locked_layers_leave_project_and_history_unchanged() {
    let mut engine = Engine::new(project()).unwrap();
    edit(
        &mut engine,
        AudioMarkersAction::Add {
            frame: 20,
            label: String::new(),
        },
    );
    let before = engine.project().clone();
    for action in [
        AudioMarkersAction::ReplaceGenerated {
            frames: vec![30, 120],
        },
        AudioMarkersAction::Add {
            frame: 0,
            label: String::new(),
        },
        AudioMarkersAction::Move { id: 99, frame: 40 },
        AudioMarkersAction::Add {
            frame: 30,
            label: "invalid\nname".into(),
        },
    ] {
        assert!(engine
            .apply(Command::AudioMarkers { object: 7, action })
            .is_err());
        assert_eq!(engine.project(), &before);
    }
    engine
        .apply(Command::Flags {
            object: 7,
            visible: true,
            locked: true,
        })
        .unwrap();
    let locked = engine.project().clone();
    assert!(engine
        .apply(Command::AudioMarkers {
            object: 7,
            action: AudioMarkersAction::ClearGenerated
        })
        .is_err());
    assert!(engine
        .apply(Command::SplitAudioChannels { object: 7 })
        .is_err());
    assert_eq!(engine.project(), &locked);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}
#[test]
fn separated_tracks_preserve_timing_envelopes_and_exact_stereo_sum() {
    let mut project = project();
    if let Content::Audio { audio } = &mut project.layers[0].content {
        audio.volume = 0.75;
        audio.source_offset_us = 250_000;
    }
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(Command::AudioLevels {
            object: 7,
            action: AudioLevelsAction::Set {
                channel: AudioChannel::Right,
                frame: 10,
                value: -12.,
                animated: Some(true),
            },
        })
        .unwrap();
    edit(
        &mut engine,
        AudioMarkersAction::Add {
            frame: 40,
            label: "Cut".into(),
        },
    );
    let before = engine.project().clone();
    let voice = before.audio_voices().unwrap().remove(0);
    engine
        .apply(Command::SplitAudioChannels { object: 7 })
        .unwrap();
    let after = engine.project().clone();
    let voices = after.audio_voices().unwrap();
    assert_eq!(voices.len(), 2);
    assert_eq!(voices[0].source_channel, AudioSourceChannel::Left);
    assert_eq!(voices[1].source_channel, AudioSourceChannel::Right);
    let at = 40 * 48_000 / 30;
    let original = voice.gain_at(at);
    assert_eq!(voices[0].gain_at(at), [original[0], 0.]);
    assert_eq!(voices[1].gain_at(at), [0., original[1]]);
    for split in &voices {
        assert_eq!(split.offset_sample, voice.offset_sample);
        assert_eq!(split.source_offset_us, voice.source_offset_us);
    }
    assert_eq!(
        after.layers[0].audio_markers,
        before.layers[0].audio_markers
    );
    assert!(after.layers[1].audio_markers.is_empty());
    assert!(engine
        .apply(Command::SplitAudioChannels { object: 7 })
        .is_err());
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    let mut mono = before;
    mono.audio_assets[0].channels = 1;
    let mut mono = Engine::new(mono).unwrap();
    assert!(mono
        .apply(Command::SplitAudioChannels { object: 7 })
        .is_err());
}
