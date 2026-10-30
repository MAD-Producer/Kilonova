use motion_core::{
    masks::{LayerMask, MaskAction},
    vector::{
        groups::{GroupItem, VectorGroup},
        FillRule, PathNode, VectorContent, VectorPath, VectorSource,
    },
    Command, Content, Ease, Engine, Keyframe, Layer, LayerTimeline, PathClipboard, PathDestination,
    PathSelection, PathTarget, Project, Track,
};
use std::collections::BTreeSet;

fn path(id: u64, first: u64, closed: bool) -> VectorPath {
    VectorPath {
        tangents: Default::default(), id,
        closed,
        nodes: (0..6)
            .map(|i| PathNode {
                id: first + i,
                geometry: Track::constant([i as f32 * 10., 5., -2., 3., 4., -5.]),
            })
            .collect(),
    }
}
fn vector(paths: Vec<VectorPath>) -> VectorContent {
    VectorContent {
        source: VectorSource::Paths { paths },
        fill: Some(Track::constant([0.2, 0.4, 0.6, 0.7])),
        fill_rule: FillRule::EvenOdd,
        stroke: None,
        trim: None,
    }
}
fn fixture() -> Engine {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    for (id, offset, p) in [(7, 10, path(5, 10, true)), (9, -7, path(99, 400, false))] {
        let mut layer = Layer::solid(id, "Geometry", [100., 80.], [0.; 3], [1.; 4]);
        layer.timeline = Some(LayerTimeline {
            in_frame: 0,
            out_frame: 110,
            offset_frame: offset,
        });
        layer.content = Content::Vector {
            vector: vector(vec![p]),
        };
        project.layers.push(layer);
    }
    let mut node = paths_mut(&mut project, 7)[0].nodes[0].clone();
    node.geometry.keys = vec![
        Keyframe {
            auto_bezier: false,
            frame: -5,
            value: [10., 20., -2., 3., 4., -5.],
            ease: Ease::InOut,
            curve: Some(motion_core::Curve {
                space: motion_core::CurveSpace::Progress,
                shape: motion_core::CurveShape::Cubic {
                    control1: [0.2, 0.1],
                    control2: [0.7, 0.9],
                    start: 0.,
                    end: 1.,
                },
            }),
            spatial: None,
        },
        Keyframe {
            auto_bezier: false,
            frame: 10,
            value: [30., 40., -6., 7., 8., -9.],
            ease: Ease::Out,
            curve: None,
            spatial: None,
        },
    ];
    paths_mut(&mut project, 7)[0].nodes[0] = node;
    project.layers[1]
        .masks
        .push(LayerMask::new(2, path(8, 100, true)));
    Engine::new(project).unwrap()
}
fn paths(project: &Project, object: u64) -> &[VectorPath] {
    let Content::Vector { vector } = &project
        .layers
        .iter()
        .find(|l| l.id == object)
        .unwrap()
        .content
    else {
        panic!()
    };
    let VectorSource::Paths { paths } = &vector.source else {
        panic!()
    };
    paths
}
fn paths_mut(project: &mut Project, object: u64) -> &mut Vec<VectorPath> {
    let Content::Vector { vector } = &mut project
        .layers
        .iter_mut()
        .find(|l| l.id == object)
        .unwrap()
        .content
    else {
        panic!()
    };
    let VectorSource::Paths { paths } = &mut vector.source else {
        panic!()
    };
    paths
}
fn selection(nodes: Option<Vec<u64>>) -> PathSelection {
    PathSelection {
        target: PathTarget::Vector {
            object: 7,
            item: None,
        },
        path: 5,
        nodes,
    }
}
fn copy(engine: &Engine, nodes: Option<Vec<u64>>) -> PathClipboard {
    PathClipboard::capture(engine.project(), &[selection(nodes)], 15).unwrap()
}
fn paste(clipboard: &PathClipboard, target: PathDestination, frame: Option<u32>) -> Command {
    Command::PastePaths {
        clipboard: Box::new(clipboard.clone()),
        target,
        at_frame: frame,
    }
}
fn destination() -> PathDestination {
    PathDestination::Vector {
        object: 9,
        item: None,
    }
}
fn rejected(engine: &mut Engine, command: Command, cause: &str) {
    let before = engine.snapshot();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    let error = engine.apply(command).unwrap_err();
    assert!(
        error.to_string().contains(cause),
        "expected {cause}: {error}"
    );
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.history_diagnostics(), history);
}

#[test]
fn paths_freeze_authored_keys_and_cross_project_offsets_save_and_undo_once() {
    let source = fixture();
    let clipboard = copy(&source, None);
    let frozen = clipboard.entries[0].path.clone();
    let mut target = source.snapshot();
    // The source document may disappear; no project or node aliases are retained.
    paths_mut(&mut target, 7)[0].nodes[0].geometry.value[0] = 999.;
    target.layers.retain(|layer| layer.id == 9);
    drop(source);
    let mut target = Engine::new(target).unwrap();
    let before = target.snapshot();
    target
        .apply(paste(&clipboard, destination(), Some(30)))
        .unwrap();
    let pasted = paths(target.project(), 9).last().unwrap();
    assert!(pasted.id > 99 && pasted.closed);
    assert!(pasted.nodes.iter().all(|node| node.id > 405));
    for (original, actual) in frozen.nodes.iter().zip(&pasted.nodes) {
        let mut expected = original.geometry.clone();
        for key in &mut expected.keys {
            key.frame += 32;
        }
        assert_eq!(actual.geometry, expected);
    }
    assert_eq!(
        pasted.nodes[0]
            .geometry
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [27, 42]
    );
    assert_eq!(clipboard.entries[0].path, frozen);
    let Content::Vector { vector } = &target.project().layers[0].content else {
        panic!()
    };
    assert_eq!(vector.fill_rule, FillRule::EvenOdd);
    assert_eq!(vector.fill.as_ref().unwrap().value, [0.2, 0.4, 0.6, 0.7]);
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), target.project()).unwrap();
    assert_eq!(
        motion_core::storage::load(directory.path()).unwrap(),
        *target.project()
    );
    let after = target.snapshot();
    assert!(target.undo().unwrap());
    assert_eq!(target.project(), &before);
    assert!(!target.undo().unwrap());
    assert!(target.redo().unwrap());
    assert_eq!(target.project(), &after);
}

#[test]
fn disconnected_node_selection_preserves_the_closing_seam_without_false_edges() {
    let mut engine = fixture();
    let before = engine.snapshot();
    let clipboard = copy(&engine, Some(vec![11, 15, 13, 10]));
    engine
        .apply(paste(&clipboard, destination(), None))
        .unwrap();
    let pasted = &paths(engine.project(), 9)[1..];
    assert_eq!(pasted.len(), 2);
    assert!(pasted.iter().all(|p| !p.closed));
    let originals = &paths(&before, 7)[0].nodes;
    for (fragment, indices) in pasted.iter().zip([vec![3], vec![5, 0, 1]]) {
        assert_eq!(fragment.nodes.len(), indices.len());
        for (actual, index) in fragment.nodes.iter().zip(indices) {
            let mut expected = originals[index].geometry.clone();
            for key in &mut expected.keys {
                key.frame += 17;
            }
            assert_eq!(actual.geometry, expected);
        }
    }
    let once = engine.snapshot();
    engine
        .apply(paste(&clipboard, destination(), None))
        .unwrap();
    let all = paths(engine.project(), 9);
    assert_eq!(all.len(), 5);
    assert_eq!(all.iter().map(|p| p.id).collect::<BTreeSet<_>>().len(), 5);
    let ids: Vec<_> = all
        .iter()
        .flat_map(|p| p.nodes.iter().map(|n| n.id))
        .collect();
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &once);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
}

#[test]
fn cut_uses_exact_source_identity_and_is_atomic_across_masks_and_vectors() {
    let mut engine = fixture();
    let before = engine.snapshot();
    let selections = [
        selection(Some(vec![10, 11, 13, 15])),
        PathSelection {
            target: PathTarget::Mask { object: 9, mask: 2 },
            path: 8,
            nodes: None,
        },
    ];
    let clipboard = PathClipboard::capture(engine.project(), &selections, 15).unwrap();
    engine
        .apply(Command::RemovePaths {
            clipboard: Box::new(clipboard.clone()),
        })
        .unwrap();
    let remaining = &paths(engine.project(), 7)[0];
    assert_eq!(
        remaining.nodes.iter().map(|n| n.id).collect::<Vec<_>>(),
        [12, 14]
    );
    assert!(!remaining.closed);
    assert!(engine.project().layers[1].masks[0].path.nodes.is_empty());
    assert_eq!(
        engine.project().layers[1].masks[0].opacity,
        before.layers[1].masks[0].opacity
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(!engine.undo().unwrap());
    let mut changed = before.layers[1].masks[0].path.clone();
    changed.nodes[0].geometry.value[0] = 123.;
    engine
        .apply(Command::Mask {
            object: 9,
            action: MaskAction::Path {
                mask: 2,
                path: changed,
            },
        })
        .unwrap();
    rejected(
        &mut engine,
        Command::RemovePaths {
            clipboard: Box::new(clipboard),
        },
        "source has changed",
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before, "failed cut added no undo step");
    let entire = copy(&engine, None);
    engine
        .apply(Command::RemovePaths {
            clipboard: Box::new(entire),
        })
        .unwrap();
    assert!(paths(engine.project(), 7).is_empty());
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
}

#[test]
fn mask_paste_preserves_settings_and_requires_separate_masks_for_fragments() {
    let mut engine = fixture();
    let clipboard = copy(&engine, None);
    let mut original = engine.project().layers[1].masks[0].clone();
    engine
        .apply(paste(
            &clipboard,
            PathDestination::Mask { object: 9, mask: 2 },
            None,
        ))
        .unwrap();
    let actual = &engine.project().layers[1].masks[0];
    assert_ne!(actual.path.id, original.path.id);
    assert_eq!(actual.path.nodes.len(), 6);
    original.path = actual.path.clone();
    assert_eq!(&original, actual);
    assert!(engine.undo().unwrap());
    let fragments = copy(&engine, Some(vec![10, 12, 14]));
    rejected(
        &mut engine,
        paste(
            &fragments,
            PathDestination::Mask { object: 9, mask: 2 },
            None,
        ),
        "exactly one connected path",
    );
    let before = engine.snapshot();
    engine
        .apply(paste(
            &fragments,
            PathDestination::NewMasks { object: 9 },
            Some(20),
        ))
        .unwrap();
    let masks = &engine.project().layers[1].masks;
    assert_eq!(masks.len(), 4);
    assert!(masks[1..]
        .iter()
        .all(|m| !m.path.closed && m.path.nodes.len() == 1 && m.id > 2));
    assert_eq!(masks[0], before.layers[1].masks[0]);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
}

fn grouped(id: u64, vector: VectorContent) -> VectorContent {
    VectorContent {
        source: VectorSource::Group {
            group: Box::new(VectorGroup {
                id,
                name: "Root".into(),
                transform: Default::default(),
                items: vec![GroupItem::Group {
                    group: Box::new(VectorGroup {
                        id: id + 1,
                        name: "Nested".into(),
                        transform: Default::default(),
                        items: vec![GroupItem::Geometry {
                            id: id + 2,
                            name: "Authored".into(),
                            vector: Box::new(vector),
                            size: Track::constant([100., 80.]),
                            position: Track::constant([20., 30.]),
                        }],
                    }),
                }],
            }),
        },
        fill: None,
        fill_rule: FillRule::NonZero,
        stroke: None,
        trim: None,
    }
}
#[test]
fn nested_geometry_and_mask_sources_remain_independent_from_group_transforms() {
    let mut project = fixture().snapshot();
    for (index, id) in [(0, 20), (1, 30)] {
        let Content::Vector { vector } = &project.layers[index].content else {
            panic!()
        };
        project.layers[index].content = Content::Vector {
            vector: grouped(id, vector.clone()),
        };
    }
    let mut engine = Engine::new(project).unwrap();
    let before = engine.snapshot();
    let clipboard = PathClipboard::capture(
        engine.project(),
        &[PathSelection {
            target: PathTarget::Vector {
                object: 7,
                item: Some(22),
            },
            path: 5,
            nodes: None,
        }],
        15,
    )
    .unwrap();
    rejected(
        &mut engine,
        paste(
            &clipboard,
            PathDestination::Vector {
                object: 9,
                item: Some(999),
            },
            None,
        ),
        "item missing",
    );
    engine
        .apply(paste(
            &clipboard,
            PathDestination::Vector {
                object: 9,
                item: Some(32),
            },
            None,
        ))
        .unwrap();
    let Content::Vector { vector } = &engine.project().layers[1].content else {
        panic!()
    };
    let VectorSource::Group { group } = &vector.source else {
        panic!()
    };
    let GroupItem::Group { group: nested } = &group.items[0] else {
        panic!()
    };
    let GroupItem::Geometry {
        vector: leaf,
        position,
        ..
    } = &nested.items[0]
    else {
        panic!()
    };
    let VectorSource::Paths { paths } = &leaf.source else {
        panic!()
    };
    assert_eq!(paths.len(), 2);
    assert_eq!(position.value, [20., 30.]);
    assert_eq!(nested.transform, Default::default());
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    let mask = PathClipboard::capture(
        engine.project(),
        &[PathSelection {
            target: PathTarget::Mask { object: 9, mask: 2 },
            path: 8,
            nodes: None,
        }],
        15,
    )
    .unwrap();
    engine
        .apply(paste(
            &mask,
            PathDestination::Vector {
                object: 7,
                item: Some(22),
            },
            None,
        ))
        .unwrap();
    assert_eq!(engine.project().layers[1], before.layers[1]);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
}

#[test]
fn invalid_source_payload_and_destination_fail_without_history_or_model_changes() {
    let mut engine = fixture();
    for nodes in [Some(vec![]), Some(vec![10, 10]), Some(vec![999])] {
        let error = PathClipboard::capture(engine.project(), &[selection(nodes)], 15).unwrap_err();
        assert!(error.to_string().contains("object=7 path=5"));
        assert!(std::error::Error::source(&error).is_some());
    }
    assert!(
        PathClipboard::capture(engine.project(), &[selection(None), selection(None)], 15).is_err()
    );
    let clipboard = copy(&engine, None);
    rejected(
        &mut engine,
        paste(&clipboard, destination(), Some(120)),
        "outside composition",
    );
    rejected(
        &mut engine,
        paste(&clipboard, PathDestination::NewMasks { object: 404 }, None),
        "does not exist",
    );
    let mut invalid = clipboard.clone();
    invalid.entries[0].path.nodes[0].geometry.value[0] = f32::NAN;
    rejected(
        &mut engine,
        paste(&invalid, destination(), None),
        "non-finite",
    );
    let mut invalid = clipboard.clone();
    invalid.entries[0].path.id = u64::MAX;
    rejected(
        &mut engine,
        paste(&invalid, destination(), None),
        "ID overflow",
    );
    let mut locked = engine.snapshot();
    locked.layers[1].locked = true;
    let mut locked = Engine::new(locked).unwrap();
    rejected(
        &mut locked,
        paste(&clipboard, destination(), None),
        "locked",
    );
    let mut wire = serde_json::to_value(&clipboard).unwrap();
    wire["ignored_payload"] = true.into();
    assert!(serde_json::from_value::<PathClipboard>(wire).is_err());
}

#[test]
fn command_registry_exposes_path_transactions_and_preserves_typed_roundtrip() {
    let mut engine = fixture();
    let clipboard = copy(&engine, None);
    let command = paste(&clipboard, destination(), None);
    let text = serde_json::to_string(&command).unwrap();
    let parsed = motion_core::CommandRegistry::parse(&text).unwrap();
    assert_eq!(parsed.len(), 1);
    let Command::InComposition {
        composition,
        command: inner,
    } = &parsed[0]
    else {
        panic!("parsed path transaction must preserve explicit composition routing");
    };
    assert_eq!(composition, &engine.project().composition_id);
    assert_eq!(
        serde_json::to_value(inner).unwrap(),
        serde_json::to_value(&command).unwrap()
    );
    for name in ["paste_paths", "remove_paths"] {
        assert!(motion_core::CommandRegistry::catalogue()["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"] == name));
    }
    engine.apply_batch(parsed).unwrap();
    assert!(engine.undo().unwrap());
    assert!(!engine.undo().unwrap());
}

#[test]
fn payload_geometry_mask_and_time_limits_fail_before_publishing_a_partial_edit() {
    let mut engine = fixture();
    let clipboard = copy(&engine, None);
    let mut many = clipboard.clone();
    for id in 6..69 {
        let mut entry = many.entries[0].clone();
        entry.path.id = id;
        many.entries.push(entry);
    }
    rejected(
        &mut engine,
        paste(&many, destination(), None),
        "too many vector paths",
    );
    many.entries.truncate(16);
    rejected(
        &mut engine,
        paste(&many, PathDestination::NewMasks { object: 9 }, None),
        "mask limit",
    );
    let mut overflow = clipboard.clone();
    overflow.entries[0].offset = i32::MAX;
    rejected(
        &mut engine,
        paste(&overflow, destination(), None),
        "local key time overflow",
    );
    let mut large = clipboard;
    large.entries[0].path.nodes = (1..=2048)
        .map(|id| PathNode {
            id,
            geometry: Track {
                spatial_animation: None,
                value: [100.12345; 6],
                axes: None,
                keys: (0..128)
                    .map(|frame| Keyframe {
                        auto_bezier: false,
                        frame,
                        value: [100.12345; 6],
                        ease: Ease::Linear,
                        curve: None,
                        spatial: None,
                    })
                    .collect(),
            },
        })
        .collect();
    assert!(
        motion_core::serialized_bytes(&large)
            > motion_model::path_clipboard::MAX_PATH_CLIPBOARD_BYTES
    );
    assert!(large.validate().unwrap_err().to_string().contains("16 MiB"));
    let mut project = engine.snapshot();
    paths_mut(&mut project, 7)[0] = large.entries[0].path.clone();
    let error = PathClipboard::capture(&project, &[selection(None)], 15).unwrap_err();
    assert!(error.to_string().contains("16 MiB"), "{error}");
    assert!(std::error::Error::source(&error).is_some());
    assert!(!engine.can_undo());
}
