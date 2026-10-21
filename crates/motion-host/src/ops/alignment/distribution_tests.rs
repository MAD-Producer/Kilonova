use super::*;
use motion_core::{Engine, Layer};

fn fixture(separated: bool, parented: bool) -> Engine {
    let mut project = Project::new(900, 600, 30, 160).unwrap();
    for (index, (size, position, angle)) in [
        ([80., 30.], [100., 90., 11.], 27.),
        ([25., 100.], [230., 170., 23.], -19.),
        ([120., 65.], [440., 310., 37.], 16.),
        ([60., 45.], [760., 490., 45.], -31.),
    ]
    .into_iter()
    .enumerate()
    {
        let mut layer = Layer::solid(index as u64 + 1, "Distribution", size, position, [1.; 4]);
        layer.transform.anchor.value = [0.15, 0.8];
        layer.transform.rotation.value[2] = angle;
        project.layers.push(layer);
    }
    let mut engine = Engine::new(project).unwrap();
    if parented {
        engine
            .apply(Command::Parent {
                object: 2,
                parent: Some(1),
                frame: 0,
            })
            .unwrap();
        // A movable middle layer parents a fixed endpoint: its inherited
        // displacement must be cancelled even when the child is edited first.
        engine
            .apply(Command::Parent {
                object: 4,
                parent: Some(2),
                frame: 0,
            })
            .unwrap();
        engine
            .apply(Command::SetVector {
                object: 1,
                property: Property::Scale,
                frame: 0,
                value: [130., 75., 100.],
            })
            .unwrap();
    }
    if separated {
        for object in 1..=4 {
            engine
                .apply(Command::SeparateDimensions {
                    object,
                    property: Property::Position,
                })
                .unwrap();
        }
    }
    engine
}
fn sampled(project: &Project, frame: f64) -> Scene {
    let mut scene = Scene::new(project);
    scene
        .sample(project, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn ordered(scene: &Scene, registry: &Registry, kind: Distribution) -> Vec<u64> {
    let axis = kind.axis();
    let coordinate = |id| {
        let b = bounds(scene, registry, id).unwrap();
        if kind.gaps() {
            b[0][axis]
        } else {
            (b[0][axis] + b[1][axis]) * 0.5
        }
    };
    let mut ids = vec![1, 2, 3, 4];
    ids.sort_by(|a, b| coordinate(*a).total_cmp(&coordinate(*b)).then(a.cmp(b)));
    ids
}
fn assert_geometry(
    before: &Project,
    after: &Project,
    frame: f64,
    kind: Distribution,
    registry: &Registry,
) {
    let initial = sampled(before, frame);
    let final_scene = sampled(after, frame);
    let ids = ordered(&initial, registry, kind);
    let axis = kind.axis();
    let mut intervals = vec![];
    for pair in ids.windows(2) {
        let a = bounds(&final_scene, registry, pair[0]).unwrap();
        let b = bounds(&final_scene, registry, pair[1]).unwrap();
        intervals.push(if kind.gaps() {
            b[0][axis] - a[1][axis]
        } else {
            (b[0][axis] + b[1][axis] - a[0][axis] - a[1][axis]) * 0.5
        });
    }
    assert!(
        intervals.iter().all(|d| (d - intervals[0]).abs() < 0.004),
        "{kind:?}: {intervals:?}"
    );
    for id in 1..=4 {
        let a = bounds(&initial, registry, id).unwrap();
        let b = bounds(&final_scene, registry, id).unwrap();
        for edge in 0..2 {
            assert!((a[edge][1 - axis] - b[edge][1 - axis]).abs() < 0.004);
            if [ids[0], ids[3]].contains(&id) {
                assert!((a[edge][axis] - b[edge][axis]).abs() < 0.004);
            }
        }
        let old = &before.layers[id as usize - 1];
        let new = &after.layers[id as usize - 1];
        assert_eq!(old.transform.anchor, new.transform.anchor);
        assert_eq!(old.transform.rotation, new.transform.rotation);
        assert_eq!(old.transform.scale, new.transform.scale);
        assert_eq!(old.transform.opacity, new.transform.opacity);
        assert_eq!(
            old.transform.position.sample(old.local_frame(frame))[2],
            new.transform.position.sample(new.local_frame(frame))[2]
        );
    }
}

#[test]
fn four_distributions_keep_endpoints_other_axis_and_tracks_with_parent_order_independence_and_atomic_history(
) {
    let registry = Registry::new_with_builtins().unwrap();
    for kind in [
        Distribution::HorizontalCenters,
        Distribution::VerticalCenters,
        Distribution::HorizontalGaps,
        Distribution::VerticalGaps,
    ] {
        for split in [false, true] {
            for parented in [false, true] {
                let mut expected = None;
                for selection in [vec![1, 2, 3, 4], vec![4, 3, 2, 1]] {
                    let mut engine = fixture(split, parented);
                    let before = engine.project().clone();
                    let commands = distribution_plan(
                        &before,
                        &sampled(&before, 17.),
                        &registry,
                        17.,
                        &selection,
                        kind,
                    )
                    .unwrap();
                    engine.apply_batch(commands).unwrap();
                    assert_geometry(&before, engine.project(), 17., kind, &registry);
                    let after = engine.project().clone();
                    if let Some(expected) = &expected {
                        assert_eq!(&after, expected);
                    } else {
                        expected = Some(after.clone());
                    }
                    let root = tempfile::tempdir().unwrap();
                    motion_core::storage::save(root.path(), &after).unwrap();
                    assert_eq!(motion_core::storage::load(root.path()).unwrap(), after);
                    engine.undo().unwrap();
                    assert_eq!(engine.project(), &before);
                    engine.redo().unwrap();
                    assert_eq!(engine.project(), &after);
                    // Repeating an already distributed arrangement does not add history.
                    assert!(distribution_plan(
                        &after,
                        &sampled(&after, 17.),
                        &registry,
                        17.,
                        &selection,
                        kind
                    )
                    .unwrap()
                    .is_empty());
                }
            }
        }
    }
}

#[test]
fn insufficient_duplicate_missing_locked_and_expression_selections_reject_whole_batch() {
    let registry = Registry::new_with_builtins().unwrap();
    let engine = fixture(false, false);
    let before = engine.project().clone();
    for ids in [vec![], vec![1], vec![1, 2], vec![1, 2, 2], vec![1, 2, 999]] {
        assert!(distribution_plan(
            &before,
            &sampled(&before, 3.),
            &registry,
            3.,
            &ids,
            Distribution::HorizontalGaps
        )
        .is_err());
        assert_eq!(engine.project(), &before);
    }
    for invalid in 0..3 {
        let mut p = before.clone();
        match invalid {
            0 => p.layers[2].locked = true,
            1 => p.layers[2].three_d = true,
            _ => p.expressions.push(motion_core::PropertyExpression {
                target: ExpressionTarget::Property {
                    object: 3,
                    property: Property::Position,
                    axis: None,
                },
                source: "[100, 120, 0]".into(),
                enabled: true,
                seed: 0,
                profile: motion_core::EXPRESSION_PROFILE.into(),
            }),
        }
        assert!(distribution_plan(
            &p,
            &sampled(&p, 3.),
            &registry,
            3.,
            &[1, 2, 3, 4],
            Distribution::VerticalCenters
        )
        .is_err());
    }
}

#[test]
fn negative_gaps_are_equal_overlap_and_ties_use_object_id_without_selection_order_dependence() {
    let registry = Registry::new_with_builtins().unwrap();
    let mut p = fixture(false, false).project().clone();
    for (i, l) in p.layers.iter_mut().enumerate() {
        l.transform.position.value = [[100., 130., 190., 270.][i], 200., 0.];
        l.transform.rotation.value = [0.; 3];
        l.transform.anchor.value = [0.; 2];
    }
    let mut engine = Engine::new(p).unwrap();
    let before = engine.project().clone();
    let commands = distribution_plan(
        &before,
        &sampled(&before, 0.),
        &registry,
        0.,
        &[4, 1, 3, 2],
        Distribution::HorizontalGaps,
    )
    .unwrap();
    engine.apply_batch(commands).unwrap();
    assert_geometry(
        &before,
        engine.project(),
        0.,
        Distribution::HorizontalGaps,
        &registry,
    );
    let scene = sampled(engine.project(), 0.);
    assert!(
        bounds(&scene, &registry, 2).unwrap()[0][0] < bounds(&scene, &registry, 1).unwrap()[1][0]
    );
    assert!(distribution_plan(
        engine.project(),
        &scene,
        &registry,
        0.,
        &[4, 1, 3, 2],
        Distribution::HorizontalGaps
    )
    .unwrap()
    .is_empty());
    let mut impossible = before.clone();
    for (index, layer) in impossible.layers.iter_mut().enumerate() {
        layer.transform.position.value[0] = 100. + index as f32 * 10.;
    }
    let saved = impossible.clone();
    assert!(distribution_plan(
        &impossible,
        &sampled(&impossible, 0.),
        &registry,
        0.,
        &[4, 1, 3, 2],
        Distribution::HorizontalGaps
    )
    .unwrap_err()
    .to_string()
    .contains("reversed edge order"));
    assert_eq!(impossible, saved);
    let mut tied = fixture(false, false).project().clone();
    for layer in &mut tied.layers[..2] {
        layer.transform.anchor.value = [0.5; 2];
        layer.transform.rotation.value = [0.; 3];
        layer.transform.position.value[1] = 200.;
    }
    let mut a = Engine::new(tied.clone()).unwrap();
    let mut b = Engine::new(tied).unwrap();
    let original = a.project().clone();
    let scene = sampled(&original, 0.);
    assert_eq!(
        &ordered(&scene, &registry, Distribution::VerticalCenters)[..2],
        &[1, 2]
    );
    for (engine, ids) in [(&mut a, vec![1, 2, 3, 4]), (&mut b, vec![4, 3, 2, 1])] {
        engine
            .apply_batch(
                distribution_plan(
                    &original,
                    &scene,
                    &registry,
                    0.,
                    &ids,
                    Distribution::VerticalCenters,
                )
                .unwrap(),
            )
            .unwrap();
    }
    assert_eq!(a.project(), b.project());
}

#[test]
fn animated_offset_clips_update_only_the_current_local_key_and_keep_other_frames() {
    let registry = Registry::new_with_builtins().unwrap();
    for split in [false, true] {
        for kind in [
            Distribution::HorizontalCenters,
            Distribution::HorizontalGaps,
            Distribution::VerticalCenters,
            Distribution::VerticalGaps,
        ] {
            let mut p = fixture(false, false).project().clone();
            for layer in &mut p.layers {
                layer.timeline = Some(motion_core::LayerTimeline {
                    in_frame: 40,
                    out_frame: 140,
                    offset_frame: 40,
                });
            }
            let mut engine = Engine::new(p).unwrap();
            for object in 1..=4 {
                if split {
                    engine
                        .apply(Command::SeparateDimensions {
                            object,
                            property: Property::Position,
                        })
                        .unwrap();
                }
                for axis in if split {
                    vec![Some(Axis::X), Some(Axis::Y)]
                } else {
                    vec![None]
                } {
                    engine
                        .apply(Command::Animate {
                            object,
                            property: Property::Position,
                            axis,
                            frame: 40,
                            enabled: true,
                        })
                        .unwrap();
                }
                let mut value = engine.project().layers[object as usize - 1]
                    .transform
                    .position
                    .sample(0.);
                value[0] += object as f32 * 19.;
                value[1] += object as f32 * 9.;
                engine
                    .apply(Command::SetVector {
                        object,
                        property: Property::Position,
                        frame: 80,
                        value,
                    })
                    .unwrap();
            }
            let before = engine.project().clone();
            let commands = distribution_plan(
                &before,
                &sampled(&before, 80.),
                &registry,
                80.,
                &[4, 1, 3, 2],
                kind,
            )
            .unwrap();
            engine.apply_batch(commands).unwrap();
            assert_geometry(&before, engine.project(), 80., kind, &registry);
            for (old, new) in before.layers.iter().zip(&engine.project().layers) {
                assert_eq!(old.timeline, new.timeline);
                assert_eq!(
                    old.transform.position.sample(0.),
                    new.transform.position.sample(0.)
                );
                assert_eq!(
                    old.transform.position.key_frames().collect::<Vec<_>>(),
                    new.transform.position.key_frames().collect::<Vec<_>>()
                );
            }
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
        }
    }
}
