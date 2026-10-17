use motion_model::{
    Axis, Curve, CurveShape, CurveSpace, Ease, Easing, SpatialTangents, Track, Tween,
};

fn collision_preserves<T: Tween + std::fmt::Debug + PartialEq>(track: &Track<T>) {
    for (from, to) in [(0, 60), (60, 0)] {
        for copy in [false, true] {
            let mut candidate = track.clone();
            let error = if copy {
                candidate.copy_key(from, to)
            } else {
                candidate.move_key(from, to)
            }
            .unwrap_err();
            assert!(error.to_string().contains("occupied"), "{error}");
            assert_eq!(&candidate, track);
        }
    }
}

#[test]
fn occupied_scalar_and_vector_destinations_preserve_values_and_handles() {
    let mut scalar = Track::constant(0.0);
    scalar.upsert(0, 1.0, Ease::In).unwrap();
    scalar.upsert(60, 2.0, Ease::Hold).unwrap();
    scalar
        .set_curve(
            0,
            Easing {
                ease: Ease::Linear,
                curve: Some(Curve {
                    space: CurveSpace::Progress,
                    shape: CurveShape::Quadratic {
                        control: [0.3, 0.8],
                        start: 0.0,
                        end: 1.0,
                    },
                }),
            },
        )
        .unwrap();
    collision_preserves(&scalar);
    let mut vector = Track::constant([0.0; 3]);
    vector.upsert(0, [1.0; 3], Ease::In).unwrap();
    vector.upsert(60, [2.0; 3], Ease::Hold).unwrap();
    vector
        .set_spatial(
            0,
            Some(SpatialTangents {
                incoming: None,
                outgoing: Some([4.0, 5.0, 6.0]),
            }),
        )
        .unwrap();
    vector
        .set_spatial(
            60,
            Some(SpatialTangents {
                incoming: Some([-4.0, -5.0, -6.0]),
                outgoing: None,
            }),
        )
        .unwrap();
    collision_preserves(&vector);
}

#[test]
fn separated_whole_and_axis_collisions_do_not_mutate_any_axis() {
    let mut vector = Track::constant([0.0; 3]);
    vector.upsert(0, [1.0; 3], Ease::Linear).unwrap();
    vector.upsert(60, [2.0; 3], Ease::Out).unwrap();
    vector.separate().unwrap();
    collision_preserves(&vector);
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        collision_preserves(vector.axes.as_ref().unwrap().get(axis));
    }
    // A destination occupied only on the last axis must not commit edits to X/Y.
    let axes = vector.axes.as_mut().unwrap();
    axes.x.delete_key(60).unwrap();
    axes.y.delete_key(60).unwrap();
    for copy in [false, true] {
        let mut candidate = vector.clone();
        let result = if copy {
            candidate.copy_key(0, 60)
        } else {
            candidate.move_key(0, 60)
        };
        assert!(result.is_err());
        assert_eq!(candidate, vector);
    }
}

#[test]
fn same_frame_is_noop_but_missing_source_is_reported() {
    let mut track = Track::constant(0.0);
    track.upsert(0, 1.0, Ease::Out).unwrap();
    let before = track.clone();
    track.move_key(0, 0).unwrap();
    track.copy_key(0, 0).unwrap();
    assert_eq!(track, before);
    for copy in [false, true] {
        let error = if copy {
            track.copy_key(5, 5)
        } else {
            track.move_key(5, 5)
        }
        .unwrap_err();
        assert!(error.to_string().contains("not found"));
        assert_eq!(track, before);
    }
}

#[test]
fn vacant_moves_and_copies_remain_sorted_and_value_edits_can_replace() {
    let mut track = Track::constant(0.0);
    for frame in [0, 30, 60] {
        track.upsert(frame, frame as f32, Ease::In).unwrap();
    }
    track.move_key(0, 45).unwrap();
    track.move_key(60, -10).unwrap();
    track.copy_key(45, 50).unwrap();
    track.copy_key(-10, -20).unwrap();
    assert_eq!(
        track.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        [-20, -10, 30, 45, 50]
    );
    assert_eq!(
        track.keys.iter().map(|k| k.value).collect::<Vec<_>>(),
        [60.0, 60.0, 30.0, 0.0, 0.0]
    );
    track.set_at(30, 4.0).unwrap();
    track.upsert(30, 5.0, Ease::Hold).unwrap();
    assert_eq!(track.keys[2].value, 5.0);
    assert_eq!(track.keys[2].ease, Ease::Hold);
    track.validate_local().unwrap();
}

#[test]
fn effect_key_collisions_are_atomic_even_without_an_engine_transaction() {
    use motion_model::{effects, CurveObject, EffectAction, EffectInstance, Layer, LayerTimeline};
    let package = motion_effects::builtin::package().unwrap();
    for (effect, param) in [("posterize", "levels"), ("curves", "p0001")] {
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|d| d.id == effect)
            .unwrap();
        let mut layer = Layer::solid(1, "test", [64.0; 2], [32.0, 32.0, 0.0], [1.0; 4]);
        layer.timeline = Some(LayerTimeline {
            in_frame: 20,
            out_frame: 60,
            offset_frame: 20,
        });
        layer.effects.push(EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [64.0; 2],
        ));
        effects::apply(
            &mut layer,
            EffectAction::Animate {
                effect: 1,
                param: param.into(),
                frame: 5,
                enabled: true,
            },
            60,
        )
        .unwrap();
        let action = if effect == "curves" {
            let mut value = CurveObject::default();
            value.channels[1] = vec![[0.0, 0.0], [1.0, 0.0]];
            EffectAction::SetCurveObject {
                effect: 1,
                param: param.into(),
                frame: 25,
                value,
            }
        } else {
            EffectAction::Set {
                effect: 1,
                param: param.into(),
                frame: 25,
                value: [8.0, 0.0, 0.0, 0.0],
            }
        };
        effects::apply(&mut layer, action, 60).unwrap();
        let p = &layer.effects[0].params[param];
        let frames: Vec<_> = if let Some(c) = &p.curve {
            c.keys.iter().map(|k| k.frame).collect()
        } else {
            p.track.keys.iter().map(|k| k.frame).collect()
        };
        assert_eq!(frames, [-15, 5]);
        let before = layer.clone();
        for (from, to) in [(5, 25), (25, 5)] {
            for copy in [false, true] {
                let action = if copy {
                    EffectAction::CopyKey {
                        effect: 1,
                        param: param.into(),
                        from,
                        to,
                    }
                } else {
                    EffectAction::MoveKey {
                        effect: 1,
                        param: param.into(),
                        from,
                        to,
                    }
                };
                let error = effects::apply(&mut layer, action, 60).unwrap_err();
                assert!(error.to_string().contains("occupied"), "{error}");
                assert_eq!(layer, before);
            }
        }
        effects::apply(
            &mut layer,
            EffectAction::MoveKey {
                effect: 1,
                param: param.into(),
                from: 5,
                to: 5,
            },
            60,
        )
        .unwrap();
        effects::apply(
            &mut layer,
            EffectAction::CopyKey {
                effect: 1,
                param: param.into(),
                from: 25,
                to: 25,
            },
            60,
        )
        .unwrap();
        assert_eq!(layer, before);
        // Vacant insertion on either side, then move back across the source.
        effects::apply(
            &mut layer,
            EffectAction::CopyKey {
                effect: 1,
                param: param.into(),
                from: 25,
                to: 35,
            },
            60,
        )
        .unwrap();
        effects::apply(
            &mut layer,
            EffectAction::MoveKey {
                effect: 1,
                param: param.into(),
                from: 35,
                to: 0,
            },
            60,
        )
        .unwrap();
        let p = &layer.effects[0].params[param];
        if let Some(c) = &p.curve {
            assert_eq!(
                c.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
                [-20, -15, 5]
            );
            assert_eq!(c.keys[0].value, c.keys[2].value);
        } else {
            assert_eq!(
                p.track.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
                [-20, -15, 5]
            );
            assert_eq!(p.track.keys[0].value, p.track.keys[2].value);
        }
    }
}
