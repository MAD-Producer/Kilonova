use motion_core::*;
use serde_json::json;

fn project() -> Project {
    let mut p = Project::new(64, 64, 24, 72).unwrap();
    p.audio_assets.push(AudioAsset {
        id: 1,
        path: "assets/tone.wav".into(),
        bytes: 64,
        mime: "audio/wav".into(),
        track: 0,
        sample_rate: 48000,
        channels: 2,
        sample_frames: 192000,
        duration_us: 4_000_000,
    });
    let mut layer = Layer::solid(1, "sound", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 72,
        offset_frame: 0,
    });
    p.layers.push(layer);
    p
}
fn edit(e: &mut Engine, action: serde_json::Value) {
    e.apply_batch(
        parse_commands(&json!({"op":"audio_levels","object":1,"action":action}).to_string())
            .unwrap(),
    )
    .unwrap();
}
#[test]
fn stereo_db_keys_keep_local_time_gesture_and_history() {
    let mut p = project();
    p.layers[0].timeline.as_mut().unwrap().offset_frame = 10;
    if let Content::Audio { audio } = &mut p.layers[0].content {
        audio.source_offset_us = 1_000_000;
    }
    let mut e = Engine::new(p).unwrap();
    let original = e.snapshot();
    edit(
        &mut e,
        json!({"kind":"set","channel":"left","frame":4,"value":-6,"animated":true}),
    );
    let first = e.snapshot();
    assert_eq!(first.version, 14);
    assert_eq!(
        first.layers[0].audio_levels.as_ref().unwrap().left.keys[0].frame,
        -6
    );
    e.begin_gesture().unwrap();
    edit(
        &mut e,
        json!({"kind":"set","channel":"left","frame":28,"value":-12}),
    );
    edit(
        &mut e,
        json!({"kind":"set","channel":"left","frame":28,"value":-18}),
    );
    e.end_gesture(true).unwrap();
    let final_state = e.snapshot();
    e.undo().unwrap();
    assert_eq!(e.snapshot(), first);
    e.redo().unwrap();
    assert_eq!(e.snapshot(), final_state);
    e.undo().unwrap();
    e.undo().unwrap();
    assert_eq!(e.snapshot(), original);
}
#[test]
fn invalid_ranges_locked_layers_and_non_audio_content_fail_atomically() {
    let mut e = Engine::new(project()).unwrap();
    let original = e.snapshot();
    let commands = json!([{"op":"audio_levels","object":1,"action":{"kind":"set","channel":"left","frame":0,"value":-6}},
        {"op":"audio_levels","object":1,"action":{"kind":"set","channel":"right","frame":0,"value":25}}]);
    assert!(e
        .apply_batch(parse_commands(&commands.to_string()).unwrap())
        .is_err());
    assert_eq!(e.snapshot(), original);
    let mut p = original.clone();
    p.layers[0].locked = true;
    let mut locked = Engine::new(p.clone()).unwrap();
    assert!(locked
        .apply(Command::AudioLevels {
            object: 1,
            action: AudioLevelsAction::Clear
        })
        .is_err());
    assert_eq!(locked.snapshot(), p);
    let mut p = original;
    p.layers[0].content = Content::Null;
    p.layers[0].size = [100.; 2];
    let mut null = Engine::new(p).unwrap();
    assert!(null
        .apply(Command::AudioLevels {
            object: 1,
            action: AudioLevelsAction::Clear
        })
        .is_err());
}
#[test]
fn nested_envelopes_use_each_owner_fps_and_fractional_sample_origin() {
    let mut p = project();
    p.version = 14;
    p.fps = 59;
    p.frames = 177;
    let mut child = p.clone();
    child.fps = 24;
    child.frames = 72;
    child.layers[0].timeline.as_mut().unwrap().offset_frame = -6;
    let mut levels = AudioLevels::default();
    levels.left.upsert(6, -12., Ease::Linear).unwrap();
    levels.left.upsert(30, 0., Ease::Linear).unwrap();
    child.layers[0].audio_levels = Some(levels);
    let c = Composition {
        id: "comp-child".into(),
        name: "child".into(),
        width: 64,
        height: 64,
        fps: 24,
        frames: 72,
        background: [0.; 4],
        camera: child.camera,
        layers: child.layers,
        expressions: vec![],
    };
    let mut reference = Layer::solid(2, "reference", [64.; 2], [32., 32., 0.], [1.; 4]);
    reference.content = Content::Composition {
        clip: CompositionClip::new("comp-child".into()),
    };
    reference.timeline = Some(LayerTimeline {
        in_frame: 1,
        out_frame: 177,
        offset_frame: 1,
    });
    reference.audio_levels = Some(AudioLevels {
        left: Track::constant(-6.),
        right: Track::constant(-12.),
    });
    p.layers = vec![reference];
    p.compositions = vec![c];
    p.validate().unwrap();
    let voices = p.audio_voices().unwrap();
    let v = &voices[0];
    let sample = 24_813;
    let child_local = (sample as f64 - 48_000. / 59.) * 24. / 48_000. + 6.;
    let expected_db = -12. + (child_local - 6.) / 24. * 12.;
    assert!((v.gain_at(sample)[0] - 10f32.powf((expected_db as f32 - 6.) / 20.)).abs() < 1e-6);
    assert!((v.gain_at(sample)[1] - 10f32.powf(-12. / 20.)).abs() < 1e-6);
}
#[test]
fn old_documents_remain_unchanged_and_bad_tracks_are_rejected() {
    let p = project();
    assert!(!serde_json::to_string(&p).unwrap().contains("audio_levels"));
    assert_eq!(
        serde_json::from_slice::<Project>(&serde_json::to_vec(&p).unwrap()).unwrap(),
        p
    );
    let mut p = p;
    p.layers[0].audio_levels = Some(AudioLevels::default());
    assert!(p.validate().is_err());
    p.version = 14;
    p.validate().unwrap();
    p.layers[0].audio_levels.as_mut().unwrap().right.value = f32::NAN;
    assert!(p.validate().is_err());
}

#[test]
fn precompose_fps_changes_and_package_roundtrip_keep_audio_envelopes() {
    let mut e=Engine::new(project()).unwrap();
    edit(&mut e,json!({"kind":"set","channel":"left","frame":0,"value":0,"animated":true}));
    edit(&mut e,json!({"kind":"set","channel":"left","frame":24,"value":-12}));
    let before=e.snapshot();
    e.apply(Command::Composition{action:CompositionAction::Precompose{objects:vec![1],name:"sound child".into(),range:"composition".into()}}).unwrap();
    let child=e.project().compositions[0].id.clone();
    for sample in [0,12345,24000,47999,60000] {
        assert_eq!(before.audio_voices().unwrap()[0].gain_at(sample),e.project().audio_voices().unwrap()[0].gain_at(sample));
    }
    e.apply(Command::InComposition{composition:child,command:Box::new(Command::Composition{action:CompositionAction::Settings{settings:CompositionSettings{
        name:"sound child".into(),width:64,height:64,fps:48,frames:144,timing:"preserve_seconds".into(),shorten:"reject".into()}}})}).unwrap();
    let retimed=e.snapshot();
    for sample in [0,12345,24000,47999,60000] {assert_eq!(before.audio_voices().unwrap()[0].gain_at(sample),retimed.audio_voices().unwrap()[0].gain_at(sample));}
    let root=tempfile::tempdir().unwrap();std::fs::create_dir(root.path().join("assets")).unwrap();
    std::fs::write(root.path().join("assets/tone.wav"),[0u8;64]).unwrap();
    motion_core::storage::save(root.path(),&retimed).unwrap();
    assert_eq!(motion_core::storage::load(root.path()).unwrap(),retimed);
    let package=root.path().join("project.msproj");motion_core::storage::export_package(root.path(),&retimed,&package).unwrap();
    assert_eq!(motion_core::storage::import_package(&package,&root.path().join("import")).unwrap(),retimed);
}
