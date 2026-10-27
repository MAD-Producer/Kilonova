use motion_core::*;
use motion_render::Scene;

fn fixture() -> Project {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    for (id, color) in [(1, [1., 0., 0., 1.]), (2, [0., 0., 1., 1.])] {
        project
            .layers
            .push(Layer::solid(id, "Visual", [64.; 2], [32., 32., 0.], color));
    }
    let manifest = motion_effects::builtin::manifest();
    let definition = manifest
        .effects
        .iter()
        .find(|effect| effect.id == "tint")
        .unwrap();
    let effect = |id| {
        EffectInstance::new(
            id,
            &manifest.id,
            &manifest.version,
            &"0".repeat(64),
            definition,
            [64.; 2],
        )
    };
    project.layers[0].effects.push(effect(1));
    let mut disabled = effect(2);
    disabled.enabled = false;
    project.layers[0].effects.push(disabled);
    project.rebuild_plugin_dependencies();
    project
}

fn command(object: u64, switch: LayerSwitch, enabled: bool) -> Command {
    Command::SetLayerSwitch {
        object,
        switch,
        enabled,
    }
}

fn scene(project: &Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
fn solo_filters_output_but_retains_non_solo_matte_dependencies() {
    let mut project = fixture();
    project.layers[0].track_matte = Some(compositing::TrackMatte {
        source: 2,
        mode: compositing::MatteMode::Alpha,
        hide_source: false,
    });
    let mut engine = Engine::new(project).unwrap();
    engine.apply(command(1, LayerSwitch::Solo, true)).unwrap();
    let sampled = scene(engine.project());
    assert_eq!(
        sampled.layers.len(),
        2,
        "matte pixels must remain available"
    );
    assert!(
        sampled
            .layers
            .iter()
            .find(|l| l.id == 1)
            .unwrap()
            .composite_visible
    );
    assert!(
        !sampled
            .layers
            .iter()
            .find(|l| l.id == 2)
            .unwrap()
            .composite_visible
    );
    engine.apply(command(1, LayerSwitch::Solo, false)).unwrap();
    assert!(scene(engine.project())
        .layers
        .iter()
        .all(|l| l.composite_visible));
}

#[test]
fn bypass_preserves_individual_flags_and_one_undo_save_reopen_and_package_roundtrip() {
    let before = fixture();
    let mut engine = Engine::new(before.clone()).unwrap();
    engine
        .apply_batch(vec![
            command(1, LayerSwitch::Effects, false),
            command(1, LayerSwitch::Solo, true),
        ])
        .unwrap();
    let after = engine.snapshot();
    assert_eq!(after.layers[0].effects, before.layers[0].effects);
    assert!(scene(&after).effects.iter().all(|effect| !effect.enabled));
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &after);
    let root = tempfile::tempdir().unwrap();
    storage::save(root.path(), &after).unwrap();
    assert_eq!(storage::load(root.path()).unwrap(), after);
    let package = root.path().join("switches.msproj");
    // These effects intentionally have no installed package; ordinary save is
    // independent of registry availability. Package persistence uses plain layers.
    let mut plain = after.clone();
    plain.layers[0].effects.clear();
    plain.rebuild_plugin_dependencies();
    storage::export_package(root.path(), &plain, &package).unwrap();
    assert_eq!(
        storage::import_package(&package, &root.path().join("reopen")).unwrap(),
        plain
    );
    engine
        .apply(command(1, LayerSwitch::Effects, true))
        .unwrap();
    let sampled = scene(engine.project());
    assert!(
        sampled
            .effects
            .iter()
            .find(|e| e.instance == 1)
            .unwrap()
            .enabled
    );
    assert!(
        !sampled
            .effects
            .iter()
            .find(|e| e.instance == 2)
            .unwrap()
            .enabled
    );
}

#[test]
fn invalid_locked_or_unavailable_switch_rolls_back_the_whole_batch() {
    let mut project = fixture();
    project.layers[1].locked = true;
    project.layers.push(Layer::camera(3, "Camera", 64, 64));
    let mut engine = Engine::new(project.clone()).unwrap();
    let revision = engine.revision();
    for rejected in [
        command(2, LayerSwitch::Solo, true),
        command(999, LayerSwitch::Video, false),
        command(3, LayerSwitch::Solo, true),
        command(3, LayerSwitch::Effects, false),
        command(1, LayerSwitch::Audio, false),
    ] {
        assert!(engine
            .apply_batch(vec![command(1, LayerSwitch::Solo, true), rejected])
            .is_err());
        assert_eq!(engine.project(), &project);
        assert_eq!(engine.revision(), revision);
        assert!(!engine.can_undo());
    }
    for field in ["solo", "effects_enabled"] {
        let mut json = serde_json::to_value(&project).unwrap();
        json["layers"][0].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Project>(json)
            .unwrap_err()
            .to_string()
            .contains(field));
    }
}

#[test]
fn precomposition_preserves_solo_output_and_scoped_commands_do_not_leak() {
    let mut project = fixture();
    project.layers[0].solo = true;
    let mut engine = Engine::new(project).unwrap();
    engine.apply_batch(parse_commands(r#"{"op":"composition","action":{"kind":"precompose","objects":[1],"name":"Solo child"}}"#).unwrap()).unwrap();
    let snapshot = engine.snapshot();
    let reference = snapshot
        .layers
        .iter()
        .find(|l| matches!(l.content, Content::Composition { .. }))
        .unwrap();
    assert!(reference.solo);
    let child = snapshot.compositions[0].id.clone();
    assert_eq!(
        scene(&snapshot)
            .layers
            .iter()
            .filter(|l| l.composite_visible)
            .count(),
        1
    );
    engine
        .apply(Command::InComposition {
            composition: child.clone(),
            command: Box::new(command(1, LayerSwitch::Effects, false)),
        })
        .unwrap();
    assert_eq!(engine.project().composition_id, MAIN_COMPOSITION);
    assert!(engine.project().layers.iter().all(|l| l.effects_enabled));
    assert!(
        !engine
            .project()
            .compositions
            .iter()
            .find(|c| c.id == child)
            .unwrap()
            .layers[0]
            .effects_enabled
    );
}

#[test]
fn audio_and_video_are_independent_and_solo_is_composition_local() {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.audio_assets.push(AudioAsset {
        id: 10,
        path: "assets/video.mp4".into(),
        bytes: 4,
        mime: "audio/mp4".into(),
        track: 1,
        sample_rate: 48000,
        channels: 2,
        sample_frames: 192000,
        duration_us: 4_000_000,
    });
    project.video_assets.push(VideoAsset {
        id: 20,
        path: "assets/video.mp4".into(),
        bytes: 4,
        mime: "video/avc".into(),
        track: 0,
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
        color_range: 2,
        audio_asset: Some(10),
    });
    let mut audio = Layer::solid(1, "Audio", [0.; 2], [0.; 3], [0.; 4]);
    audio.content = Content::Audio {
        audio: AudioClip::new(10),
    };
    let mut video = Layer::solid(2, "Video", [64.; 2], [32., 32., 0.], [1.; 4]);
    video.content = Content::Video {
        video: VideoClip::new(20),
    };
    project.layers = vec![audio, video];
    let mut engine = Engine::new(project).unwrap();
    assert_eq!(engine.project().audio_voices().unwrap().len(), 2);
    engine.apply(command(2, LayerSwitch::Video, false)).unwrap();
    assert!(scene(engine.project()).layers.is_empty());
    assert_eq!(engine.project().audio_voices().unwrap().len(), 2);
    engine.apply(command(2, LayerSwitch::Audio, false)).unwrap();
    assert_eq!(engine.project().audio_voices().unwrap().len(), 1);
    engine.apply(command(2, LayerSwitch::Video, true)).unwrap();
    assert_eq!(scene(engine.project()).layers.len(), 1);
    assert_eq!(engine.project().audio_voices().unwrap().len(), 1);
    engine.apply(command(1, LayerSwitch::Solo, true)).unwrap();
    assert!(scene(engine.project()).layers.is_empty());
    assert_eq!(engine.project().audio_voices().unwrap().len(), 1);
    engine.apply(command(1, LayerSwitch::Audio, false)).unwrap();
    assert!(engine.project().audio_voices().unwrap().is_empty());
    assert!(engine.apply(command(1, LayerSwitch::Video, true)).is_err());
}
