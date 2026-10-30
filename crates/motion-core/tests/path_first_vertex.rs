use motion_core::{
    masks::{LayerMask, MaskAction},
    vector::{
        groups::{GroupItem, GroupTransform, VectorGroup},
        FillRule, PathNode, VectorAction, VectorContent, VectorPath, VectorSource,
    },
    Command, Content, Ease, Engine, Keyframe, Layer, LayerTimeline, Project, Track,
};

fn path() -> VectorPath {
    VectorPath {
        tangents: Default::default(), id: 42,
        closed: true,
        nodes: [11, 21, 31, 41]
            .into_iter()
            .enumerate()
            .map(|(i, id)| {
                let mut geometry = Track::constant([i as f32 * 5., 10., -3., 2., 4., -1.]);
                geometry.keys = [-8, 3, 90]
                    .into_iter()
                    .map(|frame| Keyframe {
                        frame,
                        value: [i as f32 * 7., frame as f32 * 0.25, -2., 3., 5., -4.],
                        ease: Ease::InOut,
                        curve: None,
                        spatial: None,
                    })
                    .collect();
                PathNode { id, geometry }
            })
            .collect(),
    }
}
fn vector() -> VectorContent {
    VectorContent {
        source: VectorSource::Paths {
            paths: vec![path()],
        },
        fill: Some(Track::constant([0.2, 0.5, 0.9, 0.6])),
        fill_rule: FillRule::EvenOdd,
        stroke: None,
        trim: None,
    }
}
fn fixture() -> Engine {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    for id in [7, 8, 9] {
        let mut layer = Layer::solid(id, "Authored animation", [32.; 2], [12., 7., 0.], [1.; 4]);
        layer.timeline = Some(LayerTimeline {
            in_frame: 0,
            out_frame: 120,
            offset_frame: 13,
        });
        if id == 9 {
            let mut mask = LayerMask::new(6, path());
            mask.opacity.value = 63.;
            mask.expansion.value = 4.;
            layer.masks.push(mask);
        } else if id == 8 {
            let mut grouped = VectorGroup::wrap(vector(), [32.; 2]);
            let VectorSource::Group { group } = &mut grouped.source else {
                panic!()
            };
            group.transform.rotation.set_animated(-8, true).unwrap();
            group.transform.rotation.set_at(90, 27.).unwrap();
            let child = group.clone();
            *group = Box::new(VectorGroup {
                id: 50,
                name: "Parent".into(),
                transform: GroupTransform::default(),
                items: vec![GroupItem::Group { group: child }],
            });
            layer.content = Content::Vector { vector: grouped };
        } else {
            layer.content = Content::Vector { vector: vector() };
        }
        project.layers.push(layer);
    }
    Engine::new(project).unwrap()
}
fn command(object: u64, path: u64, node: u64) -> Command {
    if object == 9 {
        Command::Mask {
            object,
            action: MaskAction::SetFirstVertex {
                mask: 6,
                path,
                node,
            },
        }
    } else {
        Command::Vector {
            object,
            action: VectorAction::SetFirstVertex {
                item: (object == 8).then_some(2),
                path,
                node,
            },
        }
    }
}
fn target(project: &mut Project, object: u64) -> &mut VectorPath {
    let layer = project.layers.iter_mut().find(|l| l.id == object).unwrap();
    if object == 9 {
        return &mut layer.masks[0].path;
    }
    let Content::Vector { vector } = &mut layer.content else {
        panic!()
    };
    let vector = if object == 8 {
        let VectorSource::Group { group } = &mut vector.source else {
            panic!()
        };
        let GroupItem::Group { group } = &mut group.items[0] else {
            panic!()
        };
        let GroupItem::Geometry { vector, .. } = &mut group.items[0] else {
            panic!()
        };
        vector.as_mut()
    } else {
        vector
    };
    let VectorSource::Paths { paths } = &mut vector.source else {
        panic!()
    };
    &mut paths[0]
}

#[test]
fn first_vertex_preserves_every_track_handle_id_and_other_field_and_undoes_once() {
    for object in [7, 8, 9] {
        let mut engine = fixture();
        let original = engine.project().clone();
        let mut expected = original.clone();
        target(&mut expected, object).nodes.rotate_left(2);
        engine.apply(command(object, 42, 31)).unwrap();
        assert_eq!(*engine.project(), expected);
        let mut actual = engine.project().clone();
        let nodes = &target(&mut actual, object).nodes;
        assert_eq!(
            nodes.iter().map(|n| n.id).collect::<Vec<_>>(),
            [31, 41, 11, 21]
        );
        for frame in [-8., 0., 2.5, 13., 37.25, 90., 119.] {
            for node in nodes {
                let original_node = path().nodes.into_iter().find(|n| n.id == node.id).unwrap();
                assert_eq!(
                    node.geometry.sample(frame).unwrap(),
                    original_node.geometry.sample(frame).unwrap()
                );
            }
        }
        let root = tempfile::tempdir().unwrap();
        motion_core::storage::save(root.path(), engine.project()).unwrap();
        assert_eq!(motion_core::storage::load(root.path()).unwrap(), expected);
        assert!(engine.undo().unwrap());
        assert_eq!(*engine.project(), original);
        assert!(engine.redo().unwrap());
        assert_eq!(*engine.project(), expected);
    }
}

fn rejected(engine: &mut Engine, command: Command, cause: &str) {
    use std::error::Error as _;
    let before = engine.project().clone();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    let error = engine.apply(command).unwrap_err();
    let text = error.to_string();
    assert!(
        text.contains("operation=first_vertex") && text.contains(cause),
        "{text}"
    );
    assert!(error.source().is_some(), "retain the original cause");
    assert_eq!(*engine.project(), before);
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.history_diagnostics(), history);
}

#[test]
fn invalid_first_vertex_rejects_missing_open_locked_and_parametric_targets_atomically() {
    let mut engine = fixture();
    for object in [7, 8, 9] {
        rejected(&mut engine, command(object, 42, 404), "node=Some(404)");
        rejected(&mut engine, command(object, 404, 11), "path=404");
    }
    rejected(
        &mut engine,
        Command::Vector {
            object: 8,
            action: VectorAction::SetFirstVertex {
                item: Some(404),
                path: 42,
                node: 11,
            },
        },
        "item=404",
    );
    rejected(
        &mut engine,
        Command::Vector {
            object: 404,
            action: VectorAction::SetFirstVertex {
                item: None,
                path: 42,
                node: 11,
            },
        },
        "object=404",
    );
    let mut original = engine.project().clone();
    for object in [7, 8, 9] {
        let mut open = original.clone();
        target(&mut open, object).closed = false;
        let mut engine = Engine::new(open).unwrap();
        rejected(
            &mut engine,
            command(object, 42, 31),
            "requires a closed path",
        );
        let mut locked = original.clone();
        locked
            .layers
            .iter_mut()
            .find(|l| l.id == object)
            .unwrap()
            .locked = true;
        let mut engine = Engine::new(locked).unwrap();
        rejected(&mut engine, command(object, 42, 31), "locked");
    }
    original.layers[0].content = Content::Vector {
        vector: VectorContent::shape(motion_core::vector::ShapeKind::Rectangle),
    };
    let mut engine = Engine::new(original).unwrap();
    rejected(&mut engine, command(7, 42, 31), "requires an authored path");
}

#[test]
fn first_vertex_command_roundtrips_through_existing_vector_and_mask_registry_entries() {
    for object in [7, 8, 9] {
        let mut engine = fixture();
        let command = command(object, 42, 31);
        let json = serde_json::to_string(&command).unwrap();
        let parsed = motion_core::CommandRegistry::parse(&json).unwrap();
        let Command::InComposition {
            composition,
            command: inner,
        } = &parsed[0]
        else {
            panic!("missing composition scope")
        };
        assert_eq!(composition, &engine.project().composition_id);
        assert_eq!(
            serde_json::to_value(inner).unwrap(),
            serde_json::to_value(command).unwrap()
        );
        engine.apply_batch(parsed).unwrap();
        let mut actual = engine.project().clone();
        assert_eq!(target(&mut actual, object).nodes[0].id, 31);
    }
}
