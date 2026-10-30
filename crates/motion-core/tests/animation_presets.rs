use motion_core::{
    AnimationPreset, AxisTracks, Command, Ease, EffectImageInput, EffectInstance, Engine, Keyframe,
    Layer, LayerTimeline, PresetProperty, Project, Property, Track,
};

fn key<T>(frame: i32, value: T) -> Keyframe<T> {
    Keyframe {
        auto_bezier: false,
        frame,
        value,
        ease: Ease::InOut,
        curve: None,
        spatial: None,
    }
}
fn source() -> Project {
    let mut p = Project::new(640, 360, 30, 180).unwrap();
    let mut l = Layer::solid(7, "Source", [200., 100.], [120., 90., 0.], [1.; 4]);
    l.transform.position.keys = vec![key(10, [20., 30., 0.]), key(40, [80., 90., 0.])];
    l.transform.anchor.value = [0.25, 0.75];
    p.layers.push(l);
    p
}
fn preset(p: &Project) -> AnimationPreset {
    AnimationPreset::capture(
        p,
        7,
        "移动与锚点".into(),
        "Motion".into(),
        &[Property::Anchor, Property::Position],
        &[],
    )
    .unwrap()
}
fn target(fps: u32) -> Engine {
    let mut p = Project::new(1920, 1080, fps, 600).unwrap();
    p.layers
        .push(Layer::solid(9, "Target", [400., 300.], [0.; 3], [0.; 4]));
    p.layers
        .push(Layer::solid(11, "Second", [100.; 2], [0.; 3], [0.; 4]));
    p.layers[0].timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 600,
        offset_frame: 7,
    });
    Engine::new(p).unwrap()
}
fn apply(preset: AnimationPreset, objects: Vec<u64>, at_frame: u32) -> Command {
    Command::ApplyAnimationPreset {
        preset: Box::new(preset),
        objects,
        at_frame,
    }
}
#[test]
fn pixel_coordinates_and_seconds_are_preserved_across_sizes_fps_and_local_offsets() {
    let recipe = preset(&source());
    let json = serde_json::to_vec(&recipe).unwrap();
    assert!(!String::from_utf8_lossy(&json).contains("Source"));
    let recipe = AnimationPreset::from_json(&json).unwrap();
    let mut engine = target(60);
    let before = engine.snapshot();
    engine.apply(apply(recipe, vec![9, 11], 60)).unwrap();
    let a = &engine.project().layers[0];
    assert_eq!(
        a.transform
            .position
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [53, 113]
    );
    assert_eq!(a.transform.position.keys[0].value, [20., 30., 0.]);
    assert_eq!(a.transform.anchor.value, [0.125, 0.25]); // still 50 x 75 px
    let b = &engine.project().layers[1];
    assert_eq!(b.transform.anchor.value, [0.5, 0.75]);
    assert_eq!(b.transform.position.keys[1].frame, 120);
    let after = engine.snapshot();
    assert_eq!(a.size, before.layers[0].size);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(!engine.undo().unwrap());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &after);
    let dir = tempfile::tempdir().unwrap();
    motion_core::storage::save(dir.path(), engine.project()).unwrap();
    assert_eq!(motion_core::storage::load(dir.path()).unwrap(), after);
}
#[test]
fn separated_axes_and_curve_shape_survive_retiming() {
    let mut p = source();
    let t = &mut p.layers[0].transform.position;
    t.keys.clear();
    t.axes = Some(Box::new(AxisTracks {
        x: Track {
            value: 10.,
            keys: vec![key(-10, 1.), key(20, 10.)],
            axes: None,
        },
        y: Track {
            value: 20.,
            keys: vec![key(5, 20.), key(35, 30.)],
            axes: None,
        },
        z: Track::constant(0.),
    }));
    let mut e = target(60);
    e.apply(apply(preset(&p), vec![9], 40)).unwrap();
    let axes = e.project().layers[0]
        .transform
        .position
        .axes
        .as_ref()
        .unwrap();
    assert_eq!(
        axes.x.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        [33, 93]
    );
    assert_eq!(
        axes.y.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        [63, 123]
    );
    assert_eq!(axes.x.keys[0].ease, Ease::InOut);
}
#[test]
fn failed_batch_keeps_every_target_history_and_revision_unchanged() {
    for objects in [vec![9, 999], vec![9, 11], vec![9, 9], vec![]] {
        let mut e = target(30);
        e.apply(Command::Flags {
            object: 11,
            visible: true,
            locked: true,
        })
        .unwrap();
        let before = e.snapshot();
        let revision = e.revision();
        assert!(e.apply(apply(preset(&source()), objects, 0)).is_err());
        assert_eq!(e.project(), &before);
        assert_eq!(e.revision(), revision);
        e.undo().unwrap();
        assert!(!e.project().layers[1].locked);
    }
}
#[test]
fn fps_collision_is_an_error_instead_of_discarding_keys() {
    let mut p = source();
    p.fps = 240;
    p.layers[0].transform.position.keys = vec![key(0, [0.; 3]), key(1, [1.; 3])];
    let mut e = target(24);
    let before = e.snapshot();
    let error = e.apply(apply(preset(&p), vec![9], 0)).unwrap_err();
    assert!(error.to_string().contains("merges distinct keys"));
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
}
fn with_effects() -> Project {
    let mut p = source();
    let package = motion_effects::builtin::package().unwrap();
    let def = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "curves")
        .unwrap();
    let mut effect = EffectInstance::new(
        40,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        def,
        p.layers[0].size,
    );
    let curve = effect
        .params
        .values_mut()
        .find_map(|p| p.curve.as_mut())
        .unwrap();
    curve.keys = vec![key(25, Default::default()), key(55, Default::default())];
    effect.enabled = false;
    p.layers[0].effects.push(effect);
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    p
}
#[test]
fn effect_order_disabled_state_color_keys_and_dependency_identity_survive() {
    let mut p = with_effects();
    let mut copy = p.layers[0].effects[0].clone();
    copy.id = 30;
    p.layers[0].effects.push(copy);
    let recipe =
        AnimationPreset::capture(&p, 7, "曲线".into(), "颜色".into(), &[], &[30, 40]).unwrap();
    assert_eq!(
        recipe.effects.iter().map(|e| e.id).collect::<Vec<_>>(),
        [40, 30]
    );
    assert_eq!(recipe.dependencies.len(), 1);
    let mut e = target(60);
    e.apply(apply(recipe, vec![9, 11], 100)).unwrap();
    let a = &e.project().layers[0];
    assert_eq!(a.effects.len(), 2);
    assert!(!a.effects[0].enabled);
    let curves = a.effects[0]
        .params
        .values()
        .find_map(|p| p.curve.as_ref())
        .unwrap();
    assert_eq!(
        curves.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        [93, 153]
    );
    assert_ne!(a.effects[0].id, e.project().layers[1].effects[0].id);
    assert_eq!(a.transform.position.value, [0.; 3]); // effect-only never replaces transform
}
#[test]
fn image_resources_are_exactly_declared_remapped_and_not_overwritten() {
    let mut p = with_effects();
    p.assets.push(motion_core::Asset {
        id: 77,
        path: "assets/source.png".into(),
        width: 32,
        height: 32,
    });
    p.layers[0].effects[0].image_input = Some(EffectImageInput::Asset { asset: 77 });
    let recipe = AnimationPreset::capture(&p, 7, "资源".into(), "测试".into(), &[], &[40]).unwrap();
    let mut e = target(30);
    e.apply(apply(recipe.clone(), vec![9], 0)).unwrap();
    assert_eq!(e.project().assets.len(), 1);
    assert_eq!(
        e.project().layers[0].effects[0].image_input,
        Some(EffectImageInput::Asset { asset: 1 })
    );
    e.apply(apply(recipe.clone(), vec![11], 0)).unwrap();
    assert_eq!(e.project().assets.len(), 1);
    let mut broken = recipe;
    broken.assets.clear();
    assert!(broken.validate().is_err());
    p.layers[0].effects[0].image_input = None;
    p.layers[0].effects[0].scene = Some(motion_effects::SceneSettings {
        sprite_asset: Some(77),
        ..Default::default()
    });
    let sprite =
        AnimationPreset::capture(&p, 7, "精灵资源".into(), "测试".into(), &[], &[40]).unwrap();
    let mut destination = target(30);
    destination.apply(apply(sprite, vec![9], 0)).unwrap();
    assert_eq!(
        destination.project().layers[0].effects[0]
            .scene
            .as_ref()
            .unwrap()
            .sprite_asset,
        Some(1)
    );
    p.layers[0].effects[0].scene.as_mut().unwrap().source_layer = Some(7);
    assert!(AnimationPreset::capture(&p, 7, "外部图层".into(), "测试".into(), &[], &[40]).is_err());
}
#[test]
fn imported_images_share_the_global_id_namespace_with_audio_video_and_models() {
    let mut p = with_effects();
    p.assets.push(motion_core::Asset {
        id: 77,
        path: "assets/source.png".into(),
        width: 32,
        height: 32,
    });
    p.layers[0].effects[0].image_input = Some(EffectImageInput::Asset { asset: 77 });
    let recipe = AnimationPreset::capture(&p, 7, "资源".into(), "测试".into(), &[], &[40]).unwrap();
    let mut destination = target(30).snapshot();
    destination.model_assets.push(motion_core::ModelAsset {
        id: 1,
        path: "assets/models/static.mscn".into(),
        bytes: 1,
        sha256: "a".repeat(64),
    });
    destination.audio_assets.push(motion_core::AudioAsset {
        id: 2,
        path: "assets/audio/source.wav".into(),
        mime: "audio/wav".into(),
        bytes: 44,
        track: 0,
        sample_rate: 48_000,
        channels: 1,
        sample_frames: 48_000,
        duration_us: 1_000_000,
    });
    destination.video_assets.push(motion_core::VideoAsset {
        id: 3,
        path: "assets/video/source.mp4".into(),
        bytes: 1,
        mime: "video/avc".into(),
        track: 0,
        width: 32,
        height: 32,
        rotation: 0,
        display_width: 32,
        display_height: 32,
        video_start_us: 0,
        video_end_us: 1_000_000,
        duration_us: 1_000_000,
        frame_count: 30,
        variable_frame_rate: false,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 1,
        audio_asset: None,
    });
    let mut engine = Engine::new(destination).unwrap();
    let before = engine.snapshot();
    engine.apply(apply(recipe, vec![9, 11], 0)).unwrap();
    assert_eq!(engine.project().assets[0].id, 4);
    for layer in &engine.project().layers {
        assert_eq!(
            layer.effects[0].image_input,
            Some(EffectImageInput::Asset { asset: 4 })
        );
    }
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}
#[test]
fn format_scope_dependency_and_expression_failures_are_explicit() {
    let mut recipe = preset(&source());
    recipe.version += 1;
    assert!(recipe.validate().is_err());
    recipe.version -= 1;
    recipe.properties.push(recipe.properties[0].clone());
    assert!(recipe.validate().is_err());
    assert!(AnimationPreset::from_json(br#"{"version":1}"#).is_err());
    let mut p = with_effects();
    p.layers[0].effects[0].image_input = Some(EffectImageInput::Layer {
        layer: 7,
        stage: motion_core::EffectImageStage::Source,
    });
    assert!(AnimationPreset::capture(&p, 7, "引用".into(), "测试".into(), &[], &[40]).is_err());
    assert!(AnimationPreset::capture(&source(), 7, "空".into(), "测试".into(), &[], &[]).is_err());
    let mut recipe =
        AnimationPreset::capture(&with_effects(), 7, "依赖".into(), "测试".into(), &[], &[40])
            .unwrap();
    recipe.dependencies.clear();
    assert!(recipe.validate().is_err());
}
#[test]
fn command_registration_and_direct_model_rollback_are_real() {
    let recipe = preset(&source());
    let cmd = apply(recipe.clone(), vec![9], 20);
    assert_eq!(
        motion_core::CommandRegistry::id(&cmd),
        "apply_animation_preset"
    );
    assert!(cmd.changes_resources() && cmd.composition_transaction());
    let parsed = motion_core::parse_commands(&serde_json::to_string(&cmd).unwrap()).unwrap();
    let mut e = target(30);
    e.apply_batch(parsed).unwrap();
    let mut p = e.snapshot();
    let before = p.clone();
    assert!(recipe.apply(&mut p, &[9, 999], 0).is_err());
    assert_eq!(p, before);
    assert!(matches!(recipe.properties[0], PresetProperty::Anchor(_)));
}

#[test]
fn preset_package_roundtrip_owns_its_resources_and_replaces_only_complete_files() {
    use motion_core::storage::{
        read_animation_preset, save_animation_preset, stage_animation_preset,
    };
    use std::sync::atomic::AtomicBool;
    let cancel = AtomicBool::new(false);
    let root = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let bytes = b"independent transfer fixture, not a decoded-image assertion";
    std::fs::write(root.path().join("assets/texture.png"), bytes).unwrap();
    let mut p = with_effects();
    p.assets.push(motion_core::Asset {
        id: 77,
        path: "assets/texture.png".into(),
        width: 32,
        height: 32,
    });
    p.layers[0].effects[0].image_input = Some(EffectImageInput::Asset { asset: 77 });
    let recipe =
        AnimationPreset::capture(&p, 7, "独立预设".into(), "资源".into(), &[], &[40]).unwrap();
    let package = root.path().join("recipe.msanim");
    save_animation_preset(root.path(), &recipe, &package, &cancel).unwrap();
    assert_eq!(read_animation_preset(&package).unwrap(), recipe);
    let staged = stage_animation_preset(&package, destination.path(), &cancel).unwrap();
    let imported = destination.path().join(&staged.preset.assets[0].path);
    assert_eq!(std::fs::read(&imported).unwrap(), bytes);
    drop(staged);
    assert!(!imported.exists());
    let staged = stage_animation_preset(&package, destination.path(), &cancel).unwrap();
    let imported = destination.path().join(&staged.preset.assets[0].path);
    staged.keep_resources();
    assert_eq!(std::fs::read(&imported).unwrap(), bytes);
    let original = std::fs::read(&package).unwrap();
    std::fs::remove_file(root.path().join("assets/texture.png")).unwrap();
    assert!(save_animation_preset(root.path(), &recipe, &package, &cancel).is_err());
    assert_eq!(std::fs::read(&package).unwrap(), original);
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|e| e
        .unwrap()
        .path()
        .extension()
        .is_some_and(|x| x == "tmp")));
    let cancelled = AtomicBool::new(true);
    assert!(stage_animation_preset(&package, destination.path(), &cancelled).is_err());
    assert_eq!(
        std::fs::read_dir(destination.path().join("assets"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn malformed_preset_archives_never_extract_undeclared_or_traversal_entries() {
    use std::{fs::File, io::Write, sync::atomic::AtomicBool};
    let root = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    for name in ["../escape", "assets/../escape", "assets/undeclared.png"] {
        let path = root.path().join("bad.msanim");
        let mut archive = zip::ZipWriter::new(File::create(&path).unwrap());
        archive
            .start_file(
                "animation-preset.json",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive
            .write_all(&serde_json::to_vec(&preset(&source())).unwrap())
            .unwrap();
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"outside").unwrap();
        archive.finish().unwrap();
        assert!(motion_core::storage::stage_animation_preset(
            &path,
            target.path(),
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(std::fs::read_dir(target.path()).unwrap().count(), 0);
    }
}
