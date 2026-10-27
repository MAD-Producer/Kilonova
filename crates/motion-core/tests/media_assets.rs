use motion_core::{media_assets::*, Asset, Command, Content, Engine, Layer, Project};

fn fixture() -> Project {
    let mut p = Project::new(64, 64, 30, 60).unwrap();
    p.assets.push(Asset {
        id: 8,
        path: "assets/original.png".into(),
        width: 4,
        height: 4,
    });
    let mut layer = Layer::solid(7, "image", [4.; 2], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 8 };
    p.layers.push(layer);
    p
}

#[test]
fn asset_replacement_preserves_layers_and_single_step_history() {
    let before = fixture();
    let mut engine = Engine::new(before.clone()).unwrap();
    let source = Asset {
        id: 8,
        path: "assets/new.png".into(),
        width: 8,
        height: 6,
    };
    engine
        .apply(Command::MediaAsset {
            action: AssetAction::Replace {
                source: MediaAsset::Image(source.clone()),
            },
        })
        .unwrap();
    assert_eq!(engine.project().layers, before.layers);
    assert_eq!(engine.project().assets, vec![source]);
    assert_eq!(engine.revision(), 1);
    let after = engine.snapshot();
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &after);
}

#[test]
fn referenced_or_stale_deletes_fail_without_losing_redo_or_project() {
    let mut engine = Engine::new(fixture()).unwrap();
    engine
        .apply(Command::Rename {
            object: 7,
            name: "renamed".into(),
        })
        .unwrap();
    engine.undo().unwrap();
    let before = engine.snapshot();
    let revision = engine.revision();
    for id in [8, 999] {
        let error = engine
            .apply(Command::MediaAsset {
                action: AssetAction::Remove {
                    target: AssetRef {
                        kind: AssetKind::Image,
                        id,
                    },
                },
            })
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(if id == 8 {
                "referenced"
            } else {
                "no longer exists"
            }),
            "{error}"
        );
        assert_eq!(engine.project(), &before);
        assert_eq!(engine.revision(), revision);
        assert!(engine.can_redo());
    }
    assert!(engine.redo().unwrap());
}

#[test]
fn orphan_delete_saves_reopens_and_undo_restores_owned_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    let path = dir.path().join("assets/original.png");
    std::fs::write(&path, b"owned source bytes retained for undo").unwrap();
    let mut before = fixture();
    before.layers.clear();
    motion_core::storage::save(dir.path(), &before).unwrap();
    let mut engine = Engine::new(before.clone()).unwrap();
    engine
        .apply_batch_saved(
            vec![Command::MediaAsset {
                action: AssetAction::Remove {
                    target: AssetRef {
                        kind: AssetKind::Image,
                        id: 8,
                    },
                },
            }],
            dir.path(),
        )
        .unwrap();
    assert!(engine.project().assets.is_empty());
    assert!(motion_core::storage::load(dir.path())
        .unwrap()
        .assets
        .is_empty());
    assert!(path.is_file());
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    motion_core::storage::save(dir.path(), engine.project()).unwrap();
    assert_eq!(motion_core::storage::load(dir.path()).unwrap(), before);
}

#[test]
fn missing_identity_invalid_metadata_and_gesture_are_transactional_failures() {
    let mut engine = Engine::new(fixture()).unwrap();
    let before = engine.snapshot();
    for (id, width, path) in [
        (999, 4, "assets/new.png"),
        (8, 0, "assets/new.png"),
        (8, 4, "../outside.png"),
    ] {
        let source = Asset {
            id,
            width,
            height: 4,
            path: path.into(),
        };
        assert!(engine
            .apply(Command::MediaAsset {
                action: AssetAction::Replace {
                    source: MediaAsset::Image(source)
                }
            })
            .is_err());
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
        assert_eq!(engine.revision(), 0);
    }
    engine.begin_gesture().unwrap();
    assert!(engine
        .apply(Command::MediaAsset {
            action: AssetAction::Remove {
                target: AssetRef {
                    kind: AssetKind::Image,
                    id: 8
                }
            }
        })
        .is_err());
    engine.end_gesture(false).unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn usages_include_unreachable_compositions_and_disabled_layers() {
    let mut project = fixture();
    let mut child = motion_core::Composition {
        id: "comp-child".into(),
        name: "child".into(),
        width: 64,
        height: 64,
        fps: 30,
        frames: 60,
        work_area: project.work_area,
        guides: Vec::new(),
        background: project.background,
        camera: project.camera.clone(),
        layers: project.layers.clone(),
        expressions: vec![],
    };
    child.layers[0].name = "unreachable image".into();
    child.layers[0].visible = false;
    project.compositions.push(child);
    let target = AssetRef {
        kind: AssetKind::Image,
        id: 8,
    };
    let uses = usages(&project, target);
    assert_eq!(uses.len(), 2);
    assert_eq!(uses[1].composition, "comp-child");
    assert_eq!(uses[1].object, 7);
    let mut engine = Engine::new(project.clone()).unwrap();
    assert!(engine
        .apply(Command::MediaAsset {
            action: AssetAction::Remove { target }
        })
        .is_err());
    assert_eq!(engine.project(), &project);
}

#[test]
fn effect_image_input_blocks_deletion_even_without_an_image_layer() {
    let mut project = fixture();
    project.layers[0].content = Content::Solid { color: [1.; 4] };
    project.layers[0].effects.push(motion_core::EffectInstance {
        id: 11,
        plugin: "test".into(),
        effect: "passthrough".into(),
        version: "1.0.0".into(),
        hash: "1".repeat(64),
        enabled: false,
        seed: 0,
        params: Default::default(),
        image_input: Some(motion_core::EffectImageInput::Asset { asset: 8 }),
        scene: None,
        node_graph: None,
        shader_data: None,
    });
    project.rebuild_plugin_dependencies();
    let target = AssetRef {
        kind: AssetKind::Image,
        id: 8,
    };
    assert_eq!(usages(&project, target)[0].role, "effect:11");
    let mut engine = Engine::new(project.clone()).unwrap();
    assert!(engine
        .apply(Command::MediaAsset {
            action: AssetAction::Remove { target }
        })
        .is_err());
    assert_eq!(engine.project(), &project);
    assert!(!engine.can_undo());
}
