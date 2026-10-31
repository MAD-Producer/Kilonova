use motion_core::{
    masks::LayerMask,
    vector::{
        groups::{GroupItem, VectorGroup},
        FillRule, PathNode, VectorContent, VectorPath, VectorSource,
    },
    Command, Content, Ease, Engine, Keyframe, Layer, LayerTimeline, PathTarget, Project, Track,
};

fn fixture(grouped: bool, closed: bool) -> Engine {
    let mut p = Project::new(160, 120, 30, 120).unwrap();
    let path = VectorPath {
        tangents: Default::default(), id: 42,
        closed,
        nodes: (1..=4)
            .map(|id| PathNode {
                id,
                geometry: Track {
                    spatial_animation: None,
                    value: [id as f32, 3., -2., 1., 3., -4.],
                    axes: None,
                    keys: [-6, 11, 80]
                        .into_iter()
                        .map(|frame| Keyframe {
                            auto_bezier: false,
                            frame,
                            value: [id as f32 * 2., frame as f32 / 4., -3., 2., 4., -1.],
                            ease: Ease::InOut,
                            curve: None,
                            spatial: None,
                        })
                        .collect(),
                },
            })
            .collect(),
    };
    let vector = VectorContent {
        source: VectorSource::Paths {
            paths: vec![path.clone()],
        },
        fill: Some(Track::constant([0.3, 0.7, 0.1, 0.8])),
        fill_rule: FillRule::EvenOdd,
        stroke: None,
        trim: None,
    };
    let mut l = Layer::solid(7, "Source", [40., 30.], [70., 40., 0.], [1.; 4]);
    l.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 120,
        offset_frame: 17,
    });
    l.transform.rotation.set_animated(0, true).unwrap();
    l.transform.rotation.set_at(70, [0., 0., 48.]).unwrap();
    l.masks.push(LayerMask::new(3, path));
    l.content = Content::Vector {
        vector: if grouped {
            let mut wrapped = VectorGroup::wrap(vector, l.size);
            if let VectorSource::Group { group } = &mut wrapped.source {
                group.transform.position.value = [4., -2.];
                group.transform.anchor.value = [2., -1.];
                group.transform.scale.value = [-120., 80.];
                group.transform.rotation.value = 23.;
                group.transform.skew.value = 11.;
                group.transform.skew_axis.value = 8.;
                if let GroupItem::Geometry { position, .. } = &mut group.items[0] {
                    position.value = [3., 5.];
                }
            }
            wrapped
        } else {
            vector
        },
    };
    p.layers.push(l);
    Engine::new(p).unwrap()
}
fn command(grouped: bool) -> Command {
    Command::CopyPathAs {
        target: PathTarget::Vector {
            object: 7,
            item: grouped.then_some(2),
        },
        path: 42,
    }
}
fn close(a: &[f32; 6], b: &[f32; 6]) {
    for (a, b) in a.iter().zip(b) {
        assert!((a - b).abs() < 0.0001, "{a} != {b}");
    }
}
#[test]
fn copy_as_mask_and_shape_retains_source_keys_time_style_boundaries_storage_and_one_undo() {
    for closed in [false, true] {
        let mut e = fixture(false, closed);
        let before = e.project().clone();
        e.apply_batch(vec![command(false)]).unwrap();
        let after_mask = e.project().clone();
        assert_eq!(after_mask.layers[0].content, before.layers[0].content);
        assert_eq!(after_mask.layers[0].masks[0], before.layers[0].masks[0]);
        let mask = &after_mask.layers[0].masks[1];
        assert_eq!((mask.id, mask.path.id, mask.path.closed), (4, 4, closed));
        let Content::Vector { vector } = &before.layers[0].content else {
            panic!()
        };
        let VectorSource::Paths { paths } = &vector.source else {
            panic!()
        };
        for (old, new) in paths[0].nodes.iter().zip(&mask.path.nodes) {
            assert_eq!(old.id, new.id);
            assert_eq!(
                old.geometry
                    .keys
                    .iter()
                    .map(|k| (k.frame, k.ease, k.curve))
                    .collect::<Vec<_>>(),
                new.geometry
                    .keys
                    .iter()
                    .map(|k| (k.frame, k.ease, k.curve))
                    .collect::<Vec<_>>()
            );
            for frame in [-6., -3.5, 0., 11., 30.25, 80.] {
                let mut expected = old.geometry.sample(frame).unwrap();
                expected[0] += 20.;
                expected[1] += 15.;
                close(&expected, &new.geometry.sample(frame).unwrap());
            }
        }
        e.undo().unwrap();
        assert_eq!(*e.project(), before);
        e.redo().unwrap();
        assert_eq!(*e.project(), after_mask);
        e.apply_batch(vec![Command::CopyPathAs {
            target: PathTarget::Mask { object: 7, mask: 4 },
            path: 4,
        }])
        .unwrap();
        let copied = &e.project().layers[1];
        assert_eq!(e.project().layers[0], after_mask.layers[0]);
        assert_eq!(copied.timeline, before.layers[0].timeline);
        assert_eq!(copied.transform, before.layers[0].transform);
        let Content::Vector {
            vector: copied_vector,
        } = &copied.content
        else {
            panic!()
        };
        let VectorSource::Paths {
            paths: copied_paths,
        } = &copied_vector.source
        else {
            panic!()
        };
        assert_eq!(copied_paths[0].nodes, paths[0].nodes);
        assert_eq!(copied_paths[0].closed, closed);
        assert_eq!(copied_vector.fill, Some(Track::constant([1.; 4])));
        assert!(copied.effects.is_empty() && copied.masks.is_empty());
        let root = tempfile::tempdir().unwrap();
        motion_core::storage::save(root.path(), e.project()).unwrap();
        assert_eq!(
            motion_core::storage::load(root.path()).unwrap(),
            *e.project()
        );
        e.undo().unwrap();
        assert_eq!(*e.project(), after_mask);
    }
}
#[test]
fn group_basis_matches_rendered_geometry_at_fractional_times_without_baking() {
    let mut e = fixture(true, true);
    let before = e.project().clone();
    e.apply_batch(vec![command(true)]).unwrap();
    let Content::Vector { vector } = &before.layers[0].content else {
        panic!()
    };
    let mask = &e.project().layers[0].masks[1];
    for frame in [-6., -2.25, 0., 11., 25.5, 80.] {
        let sampled = vector.sample(frame, before.layers[0].size).unwrap();
        let batch = &sampled.batches.as_ref().unwrap()[0];
        let m = glam::Mat3::from_cols_array(&batch.transform);
        for (raw, n) in batch.vector.paths[0].nodes.iter().zip(&mask.path.nodes) {
            let p = m.transform_point2(glam::Vec2::new(raw[0], raw[1]));
            let a = m.transform_vector2(glam::Vec2::new(raw[2], raw[3]));
            let b = m.transform_vector2(glam::Vec2::new(raw[4], raw[5]));
            close(
                &[p.x + 20., p.y + 15., a.x, a.y, b.x, b.y],
                &n.geometry.sample(frame).unwrap(),
            );
        }
    }
    assert_eq!(mask.path.nodes[0].geometry.keys.len(), 3);
}
#[test]
fn unsupported_or_invalid_copies_preserve_all_state_and_original_error_context() {
    for kind in ["animated", "missing", "locked", "mask_limit", "range"] {
        let mut e = fixture(true, true);
        let mut p = e.project().clone();
        match kind {
            "animated" => {
                if let Content::Vector { vector } = &mut p.layers[0].content {
                    if let VectorSource::Group { group } = &mut vector.source {
                        group.transform.rotation.set_animated(0, true).unwrap();
                        group.transform.rotation.set_at(70, 42.).unwrap();
                    }
                }
            }
            "locked" => p.layers[0].locked = true,
            "mask_limit" => {
                for id in 4..=18 {
                    let mut m = p.layers[0].masks[0].clone();
                    m.id = id;
                    p.layers[0].masks.push(m);
                }
            }
            "range" => {
                if let Content::Vector { vector } = &mut p.layers[0].content {
                    if let VectorSource::Group { group } = &mut vector.source {
                        group.transform.position.value = [32768.; 2];
                    }
                }
            }
            _ => {}
        }
        e = Engine::new(p.clone()).unwrap();
        let c = if kind == "missing" {
            Command::CopyPathAs {
                target: PathTarget::Mask {
                    object: 7,
                    mask: 99,
                },
                path: 42,
            }
        } else {
            command(true)
        };
        let error = e.apply_batch(vec![c]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("operation=copy_as object=7 path=42"),
            "{error}"
        );
        assert!(std::error::Error::source(&error).is_some());
        assert_eq!(*e.project(), p, "{kind}");
        assert_eq!(e.revision(), 0);
        assert!(!e.can_undo());
    }
}

#[test]
fn mask_on_nonvector_source_copies_parent_and_spatial_timeline_but_refuses_transform_expression() {
    let mut p = fixture(false, true).project().clone();
    p.layers[0].content = Content::Solid {
        color: [0.1, 0.4, 0.8, 1.],
    };
    p.layers[0].three_d = true;
    p.layers
        .push(Layer::solid(99, "Parent", [20.; 2], [10., 4., 0.], [1.; 4]));
    p.layers[0].parent = Some(motion_core::ParentLink {
        object: Some(99),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    let command = Command::CopyPathAs {
        target: PathTarget::Mask { object: 7, mask: 3 },
        path: 42,
    };
    let encoded = serde_json::to_string(&command).unwrap();
    let parsed = motion_core::CommandRegistry::parse(&encoded).unwrap();
    let mut e = Engine::new(p.clone()).unwrap();
    e.apply_batch(parsed).unwrap();
    assert_eq!(e.project().layers[0], p.layers[0]);
    assert!(e.project().layers[1].three_d);
    assert_eq!(e.project().layers[1].transform, p.layers[0].transform);
    assert_eq!(e.project().layers[1].timeline, p.layers[0].timeline);
    assert_eq!(e.project().layers[1].parent, p.layers[0].parent);
    e.undo().unwrap();
    assert_eq!(*e.project(), p);
    p.expressions.push(motion_core::PropertyExpression {
        target: motion_core::ExpressionTarget::Property {
            object: 7,
            property: motion_core::Property::Position,
            axis: None,
        },
        source: "value".into(),
        enabled: true,
        seed: 0,
        profile: motion_core::EXPRESSION_PROFILE.into(),
    });
    let mut e = Engine::new(p.clone()).unwrap();
    let error = e.apply_batch(vec![command]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("transform expressions losslessly"),
        "{error}"
    );
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(*e.project(), p);
    assert!(!e.can_undo());
}
