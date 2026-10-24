use motion_core::{
    Asset, Command, CompositionAction, CompositionClip, CompositionSettings, Content, Engine,
    Layer, LayerClipboard, LayerTimeline, Project, Property, Track,
};

#[path = "layer_clipboard/resources.rs"]
mod resources;

fn source() -> Engine {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let parent = Layer::solid(1, "Controller", [80.; 2], [100., 50., 0.], [1.; 4]);
    let mut child = Layer::solid(9, "Animated", [80.; 2], [120., 90., 0.], [1.; 4]);
    child.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 90,
        offset_frame: 7,
    });
    child.transform.position.set_animated(0, true).unwrap();
    child
        .transform
        .position
        .set_at(70, [250., 120., 0.])
        .unwrap();
    project.layers = vec![parent, child];
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(Command::Parent {
            object: 9,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
    engine
}

fn paste(
    clipboard: LayerClipboard,
    at_frame: Option<u32>,
    keep_external_references: bool,
) -> Command {
    Command::PasteLayers {
        clipboard: Box::new(clipboard),
        at_frame,
        keep_external_references,
    }
}

#[test]
fn pasted_layers_have_fresh_ids_internal_links_and_independent_local_keys_and_history() {
    let source = source();
    let clipboard = LayerClipboard::capture(source.project(), vec![9, 1]).unwrap();
    let mut project = Project::new(640, 360, 60, 240).unwrap();
    project
        .layers
        .push(Layer::solid(90, "Existing", [80.; 2], [0.; 3], [1.; 4]));
    let mut engine = Engine::new(project).unwrap();
    let before = engine.snapshot();
    let result = engine
        .apply_batch(vec![paste(clipboard.clone(), Some(30), false)])
        .unwrap();
    assert_eq!(
        serde_json::to_value(&result).unwrap()[0]["objects"],
        serde_json::json!([92, 91])
    );
    let child = &engine.project().layers[2];
    assert_eq!(child.id, 92);
    assert_eq!(child.parent.as_ref().unwrap().object, Some(91));
    assert_eq!(
        child.clip(240),
        LayerTimeline {
            in_frame: 40,
            out_frame: 120,
            offset_frame: 37
        }
    );
    assert_eq!(
        child.transform.position,
        source.project().layers[1].transform.position
    );
    assert_eq!(clipboard.source.layers, source.project().layers);
    let once = engine.snapshot();
    engine.apply(paste(clipboard.clone(), None, false)).unwrap();
    assert_eq!(
        engine.project().layers[4].parent.as_ref().unwrap().object,
        Some(93)
    );
    assert_eq!(engine.project().layers[4].clip(240).offset_frame, 7);
    engine
        .apply(Command::SetVector {
            object: 92,
            property: Property::Position,
            frame: 37,
            value: [42.; 3],
        })
        .unwrap();
    assert_ne!(
        engine.project().layers[2].transform.position,
        engine.project().layers[4].transform.position
    );
    engine.undo().unwrap();
    engine.undo().unwrap();
    assert_eq!(engine.project(), &once);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &once);
}

#[test]
fn external_parent_is_retained_only_when_explicitly_scoped_and_still_present() {
    let mut engine = source();
    let clipboard = LayerClipboard::capture(engine.project(), vec![9]).unwrap();
    let before = engine.snapshot();
    let revision = engine.revision();
    let error = engine
        .apply(paste(clipboard.clone(), None, false))
        .unwrap_err();
    assert!(error.to_string().contains("dependency=1"));
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.revision(), revision);
    engine.apply(paste(clipboard.clone(), None, true)).unwrap();
    assert_eq!(
        engine
            .project()
            .layers
            .last()
            .unwrap()
            .parent
            .as_ref()
            .unwrap()
            .object,
        Some(1)
    );
    let mut empty = Engine::new(Project::new(640, 360, 30, 120).unwrap()).unwrap();
    assert!(empty
        .apply(paste(clipboard, None, true))
        .unwrap_err()
        .to_string()
        .contains("external reference missing"));
    assert!(empty.project().layers.is_empty() && !empty.can_undo());
}

#[test]
fn nested_composition_import_remaps_resources_and_preserves_save_reopen() {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let result = project
        .edit_composition(CompositionAction::Create {
            settings: CompositionSettings {
                name: "Nested".into(),
                width: 640,
                height: 360,
                fps: 24,
                frames: 96,
                timing: "preserve_frames".into(),
                shorten: "reject".into(),
            },
        })
        .unwrap();
    let child_id = result["composition"].as_str().unwrap().to_owned();
    project.assets = vec![
        Asset {
            id: 10,
            path: "assets/used.png".into(),
            width: 80,
            height: 80,
        },
        Asset {
            id: 11,
            path: "assets/unused.png".into(),
            width: 80,
            height: 80,
        },
    ];
    let mut image = Layer::solid(1, "Image", [80.; 2], [0.; 3], [1.; 4]);
    image.content = Content::Image { asset: 10 };
    project.compositions[0].layers.push(image.clone());
    let mut layer = image;
    layer.id = 3;
    layer.content = Content::Composition {
        clip: CompositionClip::new(child_id.clone()),
    };
    project.layers.push(layer);
    let clipboard = LayerClipboard::capture(&project, vec![3]).unwrap();
    let view = clipboard.resolve().unwrap();
    assert_eq!(view.assets.iter().map(|a| a.id).collect::<Vec<_>>(), [10]);
    assert_eq!(view.compositions.len(), 1);
    let mut target = Project::new(640, 360, 30, 120).unwrap();
    target.assets.push(Asset {
        id: 10,
        path: "assets/existing.png".into(),
        width: 80,
        height: 80,
    });
    let mut engine = Engine::new(target).unwrap();
    let before = engine.snapshot();
    engine.apply(paste(clipboard, None, false)).unwrap();
    let pasted = engine.snapshot();
    assert_eq!(pasted.assets.len(), 2);
    assert_eq!(pasted.compositions.len(), 1);
    assert_ne!(pasted.compositions[0].id, child_id);
    assert_eq!(
        pasted.compositions[0].layers[0].content,
        Content::Image { asset: 11 }
    );
    let root = tempfile::tempdir().unwrap();
    // Reopening validates resource ownership on disk, so the destination
    // fixture must contain both the existing and imported image files.
    std::fs::create_dir_all(root.path().join("assets")).unwrap();
    let image = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../motion-render/tests/fixtures/image-resources-adam7.png"),
    )
    .unwrap();
    for name in ["existing.png", "used.png"] {
        std::fs::write(root.path().join("assets").join(name), &image).unwrap();
    }
    motion_core::storage::save(root.path(), &pasted).unwrap();
    assert_eq!(motion_core::storage::load(root.path()).unwrap(), pasted);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn cut_is_one_transaction_detaches_remaining_children_and_rejects_locked_selection() {
    let mut engine = source();
    let mut remaining = Layer::solid(20, "Remaining", [80.; 2], [200., 50., 0.], [1.; 4]);
    remaining.transform.opacity = Track::constant(0.5);
    engine.apply(Command::Add { layer: remaining }).unwrap();
    engine
        .apply(Command::Parent {
            object: 20,
            parent: Some(9),
            frame: 21,
        })
        .unwrap();
    let before = engine.snapshot();
    let prefix = motion_model::hierarchy::prefix(&before, 20, 21.).unwrap();
    engine
        .apply(Command::RemoveLayers {
            objects: vec![1, 9],
            frame: 21,
        })
        .unwrap();
    assert_eq!(
        engine
            .project()
            .layers
            .iter()
            .map(|l| l.id)
            .collect::<Vec<_>>(),
        [20]
    );
    assert_eq!(
        engine.project().layers[0]
            .parent
            .as_ref()
            .and_then(|p| p.object),
        None
    );
    assert!(motion_model::hierarchy::prefix(engine.project(), 20, 21.)
        .unwrap()
        .abs_diff_eq(prefix, 1e-5));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    engine
        .apply(Command::Flags {
            object: 9,
            visible: true,
            locked: true,
        })
        .unwrap();
    let locked = engine.snapshot();
    let revision = engine.revision();
    assert!(engine
        .apply(Command::RemoveLayers {
            objects: vec![1, 9],
            frame: 21
        })
        .is_err());
    assert_eq!(engine.project(), &locked);
    assert_eq!(engine.revision(), revision);
}

#[test]
fn invalid_capture_and_expression_failure_leave_the_destination_unchanged() {
    let mut project = source().snapshot();
    for ids in [vec![], vec![9, 9], vec![0], vec![42]] {
        assert!(LayerClipboard::capture(&project, ids).is_err());
    }
    project.expressions.push(motion_core::PropertyExpression {
        target: motion_core::ExpressionTarget::Property {
            object: 9,
            property: Property::Opacity,
            axis: None,
        },
        source: "(() => { throw new Error('clipboard-expression-failure'); })()".into(),
        enabled: true,
        seed: 7,
        profile: motion_core::EXPRESSION_PROFILE.into(),
    });
    let clipboard = LayerClipboard::capture(&project, vec![1, 9]).unwrap();
    let mut target = Engine::new(Project::new(640, 360, 30, 120).unwrap()).unwrap();
    let before = target.snapshot();
    assert!(target
        .apply(paste(clipboard, None, false))
        .unwrap_err()
        .to_string()
        .contains("clipboard-expression-failure"));
    assert_eq!(target.project(), &before);
    assert_eq!(target.revision(), 0);
    assert!(!target.can_undo());
}

#[test]
fn fractional_edges_local_tracks_flags_and_expression_targets_survive_integer_frame_translation() {
    let mut project = source().snapshot();
    let child = &mut project.layers[1];
    child.parent = None;
    child.shy = true;
    child.locked = true;
    child.three_d = true;
    child
        .set_interval(
            120,
            motion_core::ClipRange {
                in_frame: 10.25,
                out_frame: 89.75,
            },
        )
        .unwrap();
    project.expressions.push(motion_core::PropertyExpression {
        target: motion_core::ExpressionTarget::Property {
            object: 9,
            property: Property::Opacity,
            axis: None,
        },
        source: "value * 0.5".into(),
        enabled: true,
        seed: 17,
        profile: motion_core::EXPRESSION_PROFILE.into(),
    });
    let clipboard = LayerClipboard::capture(&project, vec![9]).unwrap();
    let mut target = Engine::new(Project::new(640, 360, 60, 240).unwrap()).unwrap();
    target.apply(paste(clipboard, Some(30), false)).unwrap();
    let layer = &target.project().layers[0];
    let mut expected = project.layers[1].clone();
    expected.id = 1;
    expected.locked = false;
    expected.timeline = Some(LayerTimeline {
        in_frame: 30,
        out_frame: 110,
        offset_frame: 27,
    });
    expected.subframe_range = Some(motion_core::ClipRange {
        in_frame: 30.25,
        out_frame: 109.75,
    });
    assert_eq!(
        layer, &expected,
        "all non-time layer payloads must remain identical"
    );
    let mut expression = project.expressions[0].clone();
    expression.target.set_object(1);
    assert_eq!(target.project().expressions, [expression]);
    assert!(!layer.active(30., 240));
    assert!(layer.active(30.25, 240));
    assert!(!layer.active(109.75, 240));
}

#[test]
fn inactive_destination_composition_keeps_root_unchanged_and_undoes_in_one_step() {
    let clipboard = LayerClipboard::capture(source().project(), vec![1, 9]).unwrap();
    let mut target = Project::new(640, 360, 30, 120).unwrap();
    target
        .layers
        .push(Layer::solid(40, "Root", [80.; 2], [0.; 3], [1.; 4]));
    let result = target
        .edit_composition(CompositionAction::Create {
            settings: CompositionSettings {
                name: "Destination".into(),
                width: 640,
                height: 360,
                fps: 60,
                frames: 240,
                timing: "preserve_frames".into(),
                shorten: "reject".into(),
            },
        })
        .unwrap();
    let id = result["composition"].as_str().unwrap().to_owned();
    let mut engine = Engine::new(target.clone()).unwrap();
    engine
        .apply(Command::InComposition {
            composition: id.clone(),
            command: Box::new(paste(clipboard, None, false)),
        })
        .unwrap();
    assert_eq!(engine.project().composition_id, target.composition_id);
    assert_eq!(engine.project().layers, target.layers);
    let child = engine
        .project()
        .compositions
        .iter()
        .find(|c| c.id == id)
        .unwrap();
    assert_eq!(child.layers.len(), 2);
    assert_eq!(
        child.layers[1].parent.as_ref().unwrap().object,
        Some(child.layers[0].id)
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &target);
    assert!(!engine.can_undo());
}
