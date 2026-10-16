use motion_core::{
    vector::{groups::*, *},
    Command, Content, Ease, Easing, Engine, Layer, LayerTimeline, Project, Track,
};
use serde_json::Value;

fn edit(engine: &mut Engine, action: VectorAction) {
    engine.apply(Command::Vector { object: 7, action }).unwrap();
}
fn fixture() -> Engine {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    let mut l = Layer::solid(7, "Shapes", [100.; 2], [320., 180., 0.], [1.; 4]);
    l.content = Content::Vector {
        vector: VectorContent::shape(ShapeKind::Rectangle),
    };
    l.timeline = Some(LayerTimeline {
        in_frame: 12,
        out_frame: 120,
        offset_frame: 12,
    });
    p.layers.push(l);
    let mut e = Engine::new(p).unwrap();
    edit(&mut e, VectorAction::ConvertToGroup);
    let items = vec![
        GroupItem::Fill {
            id: 3,
            name: "Fill".into(),
            color: Track::constant([1.; 4]),
            fill_rule: FillRule::NonZero,
            composite: Composite::Above,
        },
        GroupItem::Stroke {
            id: 4,
            name: "Stroke".into(),
            stroke: Stroke {
                color: Track::constant([1.; 4]),
                width: Track::constant(2.),
                cap: LineCap::Butt,
                join: LineJoin::Miter,
                miter_limit: 4.,
                dashes: None,
            },
            composite: Composite::Above,
        },
        GroupItem::Trim {
            id: 5,
            name: "Trim".into(),
            trim: Default::default(),
        },
        GroupItem::Repeater {
            id: 6,
            name: "Repeater".into(),
            repeater: Default::default(),
        },
        GroupItem::Group {
            group: Box::new(VectorGroup {
                id: 7,
                name: "Nested".into(),
                transform: Default::default(),
                items: vec![GroupItem::Repeater {
                    id: 8,
                    name: "Nested repeat".into(),
                    repeater: Default::default(),
                }],
            }),
        },
    ];
    for item in items {
        edit(
            &mut e,
            VectorAction::EditGroupStructure {
                edit: StructureEdit::Insert { parent: 1, item },
            },
        );
    }
    e
}
fn root(e: &Engine) -> &VectorGroup {
    let Content::Vector { vector } = &e.project().layers[0].content else {
        panic!()
    };
    let VectorSource::Group { group } = &vector.source else {
        panic!()
    };
    group
}
fn track(e: &Engine, id: u64, key: &str) -> Value {
    let g = root(e);
    if id == 1 {
        return serde_json::to_value(&g.transform.position).unwrap();
    }
    let item = g.items.iter().find(|i| i.id() == id).unwrap();
    match item {
        GroupItem::Fill { color, .. } => serde_json::to_value(color).unwrap(),
        GroupItem::Stroke { stroke, .. } => {
            if key == "color" {
                serde_json::to_value(&stroke.color).unwrap()
            } else {
                serde_json::to_value(&stroke.width).unwrap()
            }
        }
        GroupItem::Trim { trim, .. } => serde_json::to_value(&trim.start).unwrap(),
        GroupItem::Repeater { repeater, .. } => serde_json::to_value(&repeater.copies).unwrap(),
        GroupItem::Geometry { size, .. } => serde_json::to_value(size).unwrap(),
        GroupItem::Group { group } => serde_json::to_value(&group.transform.scale).unwrap(),
    }
}
fn frames(t: &Value) -> Vec<i64> {
    t["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].as_i64().unwrap())
        .collect()
}
#[test]
fn group_keys_edit_only_target_tracks_with_offsets_history_and_storage_roundtrip() {
    for (item, key, a, b) in [
        (
            1,
            "position",
            ParameterValue::Vector([0., 0.]),
            ParameterValue::Vector([20., 30.]),
        ),
        (
            2,
            "size",
            ParameterValue::Vector([100.; 2]),
            ParameterValue::Vector([120.; 2]),
        ),
        (
            3,
            "color",
            ParameterValue::Color([1.; 4]),
            ParameterValue::Color([0.5, 0.2, 0.7, 0.8]),
        ),
        (
            4,
            "width",
            ParameterValue::Scalar(2.),
            ParameterValue::Scalar(10.),
        ),
        (
            5,
            "start",
            ParameterValue::Scalar(0.),
            ParameterValue::Scalar(30.),
        ),
        (
            6,
            "copies",
            ParameterValue::Scalar(3.),
            ParameterValue::Scalar(6.),
        ),
        (
            7,
            "scale",
            ParameterValue::Vector([100.; 2]),
            ParameterValue::Vector([120., 90.]),
        ),
    ] {
        let mut e = fixture();
        for (frame, value) in [(21, a), (60, b)] {
            edit(
                &mut e,
                VectorAction::SetGroupParameter {
                    item,
                    parameter: key.into(),
                    frame,
                    value,
                    animated: Some(true),
                },
            );
        }
        assert_eq!(frames(&track(&e, item, key)), [9, 48]);
        let pose = e.project().clone();
        let key_edit = |edit| VectorAction::EditGroupKey {
            item,
            parameter: key.into(),
            edit,
        };
        edit(
            &mut e,
            key_edit(KeyEdit::Curve {
                frame: 21,
                easing: Easing {
                    ease: Ease::InOut,
                    curve: None,
                },
            }),
        );
        let before_move = e.project().clone();
        edit(&mut e, key_edit(KeyEdit::Move { from: 21, to: 30 }));
        assert_eq!(frames(&track(&e, item, key)), [18, 48]);
        assert_eq!(track(&e, item, key)["keys"][0]["ease"], "in_out");
        e.undo().unwrap();
        assert_eq!(e.project(), &before_move);
        e.redo().unwrap();
        edit(&mut e, key_edit(KeyEdit::Copy { from: 30, to: 45 }));
        assert_eq!(frames(&track(&e, item, key)), [18, 33, 48]);
        edit(&mut e, key_edit(KeyEdit::Delete { frame: 30 }));
        assert_eq!(frames(&track(&e, item, key)), [33, 48]);
        let before_invalid = e.project().clone();
        for operation in [
            KeyEdit::Move { from: 45, to: 60 },
            KeyEdit::Copy { from: 45, to: 60 },
            KeyEdit::Delete { frame: 46 },
            KeyEdit::Move { from: 45, to: 120 },
            KeyEdit::Curve {
                frame: 60,
                easing: Easing {
                    ease: Ease::In,
                    curve: None,
                },
            },
        ] {
            assert!(e
                .apply(Command::Vector {
                    object: 7,
                    action: key_edit(operation)
                })
                .is_err());
            assert_eq!(e.project(), &before_invalid);
        }
        let folder = tempfile::tempdir().unwrap();
        motion_core::storage::save(folder.path(), e.project()).unwrap();
        assert_eq!(
            &motion_core::storage::load(folder.path()).unwrap(),
            e.project()
        );
        // Unrelated item data and layer data remain byte-for-byte unchanged.
        let mut compare = e.project().clone();
        compare.layers[0].content = pose.layers[0].content.clone();
        assert_eq!(compare, pose);
        for other in &root(&e).items {
            if other.id() != item {
                assert_eq!(
                    other,
                    root(&Engine::new(pose.clone()).unwrap())
                        .items
                        .iter()
                        .find(|i| i.id() == other.id())
                        .unwrap()
                );
            }
        }
    }
}
#[test]
fn structure_options_preserve_animation_and_reject_invalid_edits_atomically() {
    let mut e = fixture();
    edit(
        &mut e,
        VectorAction::SetGroupParameter {
            item: 4,
            parameter: "width".into(),
            frame: 21,
            value: ParameterValue::Scalar(6.),
            animated: Some(true),
        },
    );
    let before = e.project().clone();
    let animated = track(&e, 4, "width");
    for option in [
        OptionsEdit::Cap {
            value: LineCap::Round,
        },
        OptionsEdit::Join {
            value: LineJoin::Bevel,
        },
        OptionsEdit::Miter { value: 8. },
        OptionsEdit::Dashes { enabled: true },
        OptionsEdit::DashPairs { pairs: 3 },
    ] {
        edit(
            &mut e,
            VectorAction::EditGroupStructure {
                edit: StructureEdit::Options {
                    item: 4,
                    edit: option,
                },
            },
        );
        assert_eq!(track(&e, 4, "width"), animated);
    }
    let GroupItem::Stroke { stroke, .. } = &root(&e).items[2] else {
        panic!()
    };
    assert_eq!(stroke.dashes.as_ref().unwrap().pattern.len(), 6);
    let good = e.project().clone();
    for option in [
        OptionsEdit::Miter { value: 0. },
        OptionsEdit::DashPairs { pairs: 4 },
    ] {
        assert!(e
            .apply(Command::Vector {
                object: 7,
                action: VectorAction::EditGroupStructure {
                    edit: StructureEdit::Options {
                        item: 4,
                        edit: option
                    }
                }
            })
            .is_err());
        assert_eq!(e.project(), &good);
    }
    edit(
        &mut e,
        VectorAction::EditGroupStructure {
            edit: StructureEdit::Move { item: 6, delta: -1 },
        },
    );
    assert_eq!(root(&e).items[3].id(), 6);
    e.undo().unwrap();
    assert_eq!(e.project(), &good);
    edit(
        &mut e,
        VectorAction::EditGroupStructure {
            edit: StructureEdit::Remove { item: 7 },
        },
    );
    assert!(!root(&e).items.iter().any(|i| i.id() == 7));
    e.undo().unwrap();
    assert_eq!(e.project(), &good);
    for _ in 0..5 {
        e.undo().unwrap();
    }
    assert_eq!(e.project(), &before);
}
#[test]
fn existing_flat_shape_animation_and_style_survive_first_content_edit() {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    let mut l = Layer::solid(7, "Flat", [100.; 2], [0.; 3], [1.; 4]);
    let v = VectorContent::shape(ShapeKind::Star);
    l.content = Content::Vector { vector: v.clone() };
    p.layers.push(l);
    let mut e = Engine::new(p).unwrap();
    let before = e.project().clone();
    edit(
        &mut e,
        VectorAction::SetGroupParameter {
            item: 2,
            parameter: "shape:points".into(),
            frame: 20,
            value: ParameterValue::Scalar(7.),
            animated: Some(true),
        },
    );
    assert_eq!(e.project().version, before.version);
    let Content::Vector { vector } = &e.project().layers[0].content else {
        panic!()
    };
    assert_eq!(vector.fill, v.fill);
    assert_eq!(vector.stroke, v.stroke);
    let VectorSource::Shape { parameters, .. } = &vector.source else {
        panic!()
    };
    assert_eq!(parameters["points"].keys[0].ease, Ease::Hold);
    let good = e.project().clone();
    assert!(e
        .apply(Command::Vector {
            object: 7,
            action: VectorAction::EditGroupKey {
                item: 2,
                parameter: "shape:points".into(),
                edit: KeyEdit::Curve {
                    frame: 20,
                    easing: Easing {
                        ease: Ease::In,
                        curve: None
                    }
                }
            }
        })
        .is_err());
    assert_eq!(e.project(), &good);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
}
