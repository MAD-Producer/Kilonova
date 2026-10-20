use super::*;
use motion_core::{Engine, Layer};

fn fixture() -> Engine {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    p.layers.push(Layer::solid(
        1,
        "Wide",
        [100., 40.],
        [180., 150., 31.],
        [1.; 4],
    ));
    p.layers.push(Layer::solid(
        2,
        "Tall",
        [30., 90.],
        [420., 220., 19.],
        [1.; 4],
    ));
    p.layers[0].transform.rotation.value[2] = 32.;
    p.layers[0].transform.anchor.value = [0.15, 0.8];
    p.layers[0].transform.scale.value = [160., 80., 100.];
    p.layers[1].transform.rotation.value[2] = -21.;
    Engine::new(p).unwrap()
}
fn sampled(p: &Project, frame: f64) -> Scene {
    let mut scene = Scene::new(p);
    scene
        .sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
#[test]
fn six_edges_use_evaluated_bounds_and_preserve_other_tracks_with_one_undo_and_save_reopen() {
    let registry = Registry::new_with_builtins().unwrap();
    for target in [Target::Composition, Target::Selection] {
        for edge in [
            Edge::Left,
            Edge::HorizontalCenter,
            Edge::Right,
            Edge::Top,
            Edge::VerticalCenter,
            Edge::Bottom,
        ] {
            for separated in [false, true] {
                let mut engine = fixture();
                if separated {
                    engine
                        .apply(Command::SeparateDimensions {
                            object: 1,
                            property: Property::Position,
                        })
                        .unwrap();
                }
                let before = engine.project().clone();
                let original = sampled(&before, 17.);
                let initial = [
                    bounds(&original, &registry, 1).unwrap(),
                    bounds(&original, &registry, 2).unwrap(),
                ];
                let union = std::array::from_fn(|i| {
                    std::array::from_fn(|axis| {
                        if i == 0 {
                            initial[0][i][axis].min(initial[1][i][axis])
                        } else {
                            initial[0][i][axis].max(initial[1][i][axis])
                        }
                    })
                });
                let expected = edge.coordinate(if target == Target::Composition {
                    [[0.; 2], [640., 360.]]
                } else {
                    union
                });
                let commands =
                    plan(&before, &original, &registry, 17., &[2, 1], edge, target).unwrap();
                engine.apply_batch(commands).unwrap();
                let after = sampled(engine.project(), 17.);
                for (index, object) in [1, 2].into_iter().enumerate() {
                    let b = bounds(&after, &registry, object).unwrap();
                    assert!(
                        (edge.coordinate(b) - expected).abs() < 0.002,
                        "{edge:?}/{target:?}: {b:?}"
                    );
                    let other = 1 - edge.axis();
                    assert!((b[0][other] - initial[index][0][other]).abs() < 0.002);
                    assert!((b[1][other] - initial[index][1][other]).abs() < 0.002);
                    let old = &before.layers[index];
                    let new = &engine.project().layers[index];
                    assert_eq!(old.transform.anchor, new.transform.anchor);
                    assert_eq!(old.transform.scale, new.transform.scale);
                    assert_eq!(old.transform.rotation, new.transform.rotation);
                    assert_eq!(old.transform.opacity, new.transform.opacity);
                    assert_eq!(
                        old.transform.position.sample(17.)[2],
                        new.transform.position.sample(17.)[2]
                    );
                }
                let folder = tempfile::tempdir().unwrap();
                motion_core::storage::save(folder.path(), engine.project()).unwrap();
                let reopened = motion_core::storage::load(folder.path()).unwrap();
                assert_eq!(&reopened, engine.project());
                let committed = engine.project().clone();
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
                engine.redo().unwrap();
                assert_eq!(engine.project(), &committed);
            }
        }
    }
}
#[test]
fn rotated_scaled_parent_and_selected_ancestor_are_compensated_independent_of_selection_order() {
    let registry = Registry::new_with_builtins().unwrap();
    for selected in [vec![1, 2], vec![2, 1], vec![2]] {
        let mut engine = fixture();
        engine
            .apply(Command::Parent {
                object: 2,
                parent: Some(1),
                frame: 0,
            })
            .unwrap();
        engine
            .apply(Command::SetVector {
                object: 1,
                property: Property::Rotation,
                frame: 0,
                value: [0., 0., 72.],
            })
            .unwrap();
        engine
            .apply(Command::SetVector {
                object: 1,
                property: Property::Scale,
                frame: 0,
                value: [140., 75., 100.],
            })
            .unwrap();
        engine
            .apply(Command::SeparateDimensions {
                object: 2,
                property: Property::Position,
            })
            .unwrap();
        let before = engine.project().clone();
        let original = sampled(&before, 9.);
        let commands = plan(
            &before,
            &original,
            &registry,
            9.,
            &selected,
            Edge::Left,
            Target::Composition,
        )
        .unwrap();
        engine.apply_batch(commands).unwrap();
        let after = sampled(engine.project(), 9.);
        for object in selected {
            let b = bounds(&after, &registry, object).unwrap();
            let old = bounds(&original, &registry, object).unwrap();
            assert!(b[0][0].abs() < 0.003, "{object}: {b:?}");
            assert!((b[0][1] - old[0][1]).abs() < 0.003);
        }
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }
}
#[test]
fn rejection_and_already_aligned_are_nonmutating() {
    let registry = Registry::new_with_builtins().unwrap();
    let mut engine = fixture();
    let before = engine.project().clone();
    let scene = sampled(&before, 4.);
    for ids in [vec![], vec![1, 1], vec![1, 999], vec![0]] {
        assert!(plan(
            &before,
            &scene,
            &registry,
            4.,
            &ids,
            Edge::Left,
            Target::Composition
        )
        .is_err());
        assert_eq!(engine.project(), &before);
    }
    assert!(plan(
        &before,
        &scene,
        &registry,
        4.,
        &[1],
        Edge::Left,
        Target::Selection
    )
    .is_err());
    for kind in 0..4 {
        let mut p = before.clone();
        match kind {
            0 => p.layers[1].locked = true,
            1 => p.layers[1].three_d = true,
            2 => p.layers[1].visible = false,
            _ => p.layers[1].content = Content::Null,
        }
        assert!(plan(
            &p,
            &sampled(&p, 4.),
            &registry,
            4.,
            &[1, 2],
            Edge::Left,
            Target::Composition
        )
        .is_err());
    }
    engine
        .apply(Command::SeparateDimensions {
            object: 1,
            property: Property::Position,
        })
        .unwrap();
    let p = engine.project().clone();
    let scene = sampled(&p, 4.);
    engine
        .apply_batch(
            plan(
                &p,
                &scene,
                &registry,
                4.,
                &[1],
                Edge::Left,
                Target::Composition,
            )
            .unwrap(),
        )
        .unwrap();
    let p = engine.project().clone();
    let scene = sampled(&p, 4.);
    assert!(plan(
        &p,
        &scene,
        &registry,
        4.,
        &[1],
        Edge::Left,
        Target::Composition
    )
    .unwrap()
    .is_empty());
}

#[test]
fn clip_offsets_write_the_existing_local_key_and_keep_unrelated_keys() {
    let registry = Registry::new_with_builtins().unwrap();
    for separated in [false, true] {
        let mut p = fixture().project().clone();
        p.layers[0].timeline = Some(motion_core::LayerTimeline {
            in_frame: 40,
            out_frame: 110,
            offset_frame: 40,
        });
        let mut engine = Engine::new(p).unwrap();
        if separated {
            engine
                .apply(Command::SeparateDimensions {
                    object: 1,
                    property: Property::Position,
                })
                .unwrap();
        }
        for axis in if separated {
            vec![Some(Axis::X), Some(Axis::Y)]
        } else {
            vec![None]
        } {
            engine
                .apply(Command::Animate {
                    object: 1,
                    property: Property::Position,
                    axis,
                    frame: 40,
                    enabled: true,
                })
                .unwrap();
        }
        engine
            .apply(Command::SetVector {
                object: 1,
                property: Property::Position,
                frame: 60,
                value: [260., 140., 31.],
            })
            .unwrap();
        let before = engine.project().clone();
        let scene = sampled(&before, 60.);
        engine
            .apply_batch(
                plan(
                    &before,
                    &scene,
                    &registry,
                    60.,
                    &[1],
                    Edge::Right,
                    Target::Composition,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(
            (bounds(&sampled(engine.project(), 60.), &registry, 1).unwrap()[1][0] - 640.).abs()
                < 0.002
        );
        assert_eq!(
            before.layers[0].timeline,
            engine.project().layers[0].timeline
        );
        assert_eq!(
            before.layers[0].transform.position.sample(0.),
            engine.project().layers[0].transform.position.sample(0.)
        );
        assert_eq!(
            before.layers[0]
                .transform
                .position
                .key_frames()
                .collect::<Vec<_>>(),
            engine.project().layers[0]
                .transform
                .position
                .key_frames()
                .collect::<Vec<_>>()
        );
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }
}

#[test]
fn expression_control_and_singular_parent_reject_the_entire_plan() {
    let registry = Registry::new_with_builtins().unwrap();
    let mut p = fixture().project().clone();
    p.expressions.push(motion_core::PropertyExpression {
        target: ExpressionTarget::Property {
            object: 2,
            property: Property::Position,
            axis: None,
        },
        source: "[100, 120, 0]".into(),
        enabled: true,
        seed: 0,
        profile: motion_core::EXPRESSION_PROFILE.into(),
    });
    assert!(plan(
        &p,
        &sampled(&p, 4.),
        &registry,
        4.,
        &[1, 2],
        Edge::Left,
        Target::Composition
    )
    .unwrap_err()
    .contains("position is controlled"));
    let mut engine = fixture();
    engine
        .apply(Command::Parent {
            object: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
    engine
        .apply(Command::SetVector {
            object: 1,
            property: Property::Scale,
            frame: 0,
            value: [0., 100., 100.],
        })
        .unwrap();
    let before = engine.project().clone();
    assert!(plan(
        &before,
        &sampled(&before, 4.),
        &registry,
        4.,
        &[2],
        Edge::Left,
        Target::Composition
    )
    .unwrap_err()
    .contains("zero-scale parent"));
    assert_eq!(engine.project(), &before);
}
