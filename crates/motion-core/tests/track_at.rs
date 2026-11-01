use motion_core::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    parse_commands, Command, Ease, Engine, KeyframeTarget, Layer, LayerTimeline, Project, Property,
    TrackEdit, TrackValue,
};
use serde_json::{json, Value};
#[path = "track_at/audio.rs"]
mod audio;

fn fixture(direction: TimeDirection) -> Engine {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(7, "Exact authoring", [32.; 2], [16., 16., 0.], [1.; 4]);
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 60,
        offset_frame: 2,
    });
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            direction,
            0.,
            if direction == TimeDirection::Forward {
                0.
            } else {
                10.
            },
        )
        .unwrap(),
        remap: None,
    });
    for frame in [0, 3, 6, 9] {
        layer
            .transform
            .position
            .upsert(frame, [frame as f32, 16., 0.], Ease::Linear)
            .unwrap();
        layer
            .transform
            .opacity
            .upsert(frame, 0.2 + frame as f32 / 20., Ease::Linear)
            .unwrap();
    }
    project.layers.push(layer);
    Engine::new(project).unwrap()
}
fn target(property: Property) -> KeyframeTarget {
    KeyframeTarget::Property {
        object: 7,
        property,
        axis: None,
    }
}
fn apply(
    engine: &mut Engine,
    frame: f64,
    property: Property,
    edit: TrackEdit,
) -> motion_core::Result<()> {
    engine.apply(Command::TrackAt {
        target: target(property),
        frame,
        edit,
    })
}
fn roundtrip(project: &Project) -> Project {
    let bytes = serde_json::to_vec(project).unwrap();
    let restored: Project = serde_json::from_slice(&bytes).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, *project);
    restored
}

#[test]
fn fractional_composition_time_updates_exact_signed_local_keys_in_both_directions() {
    for direction in [TimeDirection::Forward, TimeDirection::Reverse] {
        let mut engine = fixture(direction);
        let clock = engine.project().layers[0]
            .time_mapping
            .as_ref()
            .unwrap()
            .clock;
        for local in [-1, 3, 6] {
            let frame = clock.composition_frame(f64::from(local), 2).unwrap();
            if frame < 0. {
                continue;
            }
            apply(
                &mut engine,
                frame,
                Property::Position,
                TrackEdit::Set {
                    value: TrackValue::Vec3([99., 4., 0.]),
                },
            )
            .unwrap();
            let track = &engine.project().layers[0].transform.position;
            assert_eq!(
                track
                    .keys
                    .iter()
                    .find(|key| key.frame == local)
                    .unwrap()
                    .value,
                [99., 4., 0.]
            );
            roundtrip(engine.project());
        }
    }
}

#[test]
fn fractional_local_keys_are_rejected_without_rounding_or_partial_batch_mutation() {
    let mut engine = fixture(TimeDirection::Forward);
    let before = engine.snapshot();
    let commands = vec![
        Command::TrackAt {
            target: target(Property::Opacity),
            frame: 6.5,
            edit: TrackEdit::Set {
                value: TrackValue::Scalar(0.9),
            },
        },
        Command::TrackAt {
            target: target(Property::Position),
            frame: 6.6,
            edit: TrackEdit::Set {
                value: TrackValue::Vec3([99.; 3]),
            },
        },
    ];
    let error = engine.apply_batch(commands).unwrap_err().to_string();
    assert!(error.contains("continuous key addressing"), "{error}");
    assert!(error.contains("track_at") && error.contains("7"), "{error}");
    assert_eq!(engine.snapshot(), before);
    assert!(!engine.can_undo());
    for edit in [TrackEdit::Animate { enabled: true }, TrackEdit::ToggleKey] {
        let mut project = before.clone();
        project.layers[0].transform.opacity.keys.clear();
        let mut static_engine = Engine::new(project.clone()).unwrap();
        assert!(apply(&mut static_engine, 6.6, Property::Opacity, edit).is_err());
        assert_eq!(static_engine.snapshot(), project);
    }
}

#[test]
fn static_values_accept_fractional_local_times_and_animation_disable_freezes_exact_sample() {
    let mut engine = fixture(TimeDirection::Forward);
    let expected = engine.project().layers[0]
        .transform
        .position
        .sample((6.6 - 2.) * 2. / 3.)
        .unwrap();
    apply(
        &mut engine,
        6.6,
        Property::Position,
        TrackEdit::Animate { enabled: false },
    )
    .unwrap();
    assert_eq!(
        engine.project().layers[0].transform.position.value,
        expected
    );
    assert!(engine.project().layers[0]
        .transform
        .position
        .keys
        .is_empty());
    apply(
        &mut engine,
        6.6,
        Property::Position,
        TrackEdit::Set {
            value: TrackValue::Vec3([8., 9., 10.]),
        },
    )
    .unwrap();
    assert_eq!(
        engine.project().layers[0].transform.position.value,
        [8., 9., 10.]
    );
    assert!(engine.undo().unwrap());
    assert_eq!(
        engine.project().layers[0].transform.position.value,
        expected
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project().layers[0].transform.position.keys.len(), 4);
    assert!(engine.redo().unwrap());
    roundtrip(engine.project());
}

#[test]
fn numeric_updates_share_one_gesture_and_cancel_preserves_the_original_project() {
    for commit in [false, true] {
        let mut engine = fixture(TimeDirection::Forward);
        let before = engine.snapshot();
        engine.begin_gesture().unwrap();
        for value in [0.4, 0.5, 0.6] {
            apply(
                &mut engine,
                6.5,
                Property::Opacity,
                TrackEdit::Set {
                    value: TrackValue::Scalar(value),
                },
            )
            .unwrap();
        }
        assert!(apply(&mut engine, 6.5, Property::Opacity, TrackEdit::ToggleKey).is_err());
        engine.end_gesture(commit).unwrap();
        if commit {
            let final_state = engine.snapshot();
            assert_ne!(final_state, before);
            assert!(engine.undo().unwrap());
            assert_eq!(engine.snapshot(), before);
            assert!(!engine.undo().unwrap());
            assert!(engine.redo().unwrap());
            assert_eq!(engine.snapshot(), final_state);
        } else {
            assert_eq!(engine.snapshot(), before);
            assert!(!engine.can_undo());
        }
    }
}

#[test]
fn exact_toggle_removes_and_restores_local_key_without_duplicate_projected_keys() {
    let mut engine = fixture(TimeDirection::Reverse);
    let clock = engine.project().layers[0]
        .time_mapping
        .as_ref()
        .unwrap()
        .clock;
    let frame = clock.composition_frame(3., 2).unwrap();
    apply(&mut engine, frame, Property::Position, TrackEdit::ToggleKey).unwrap();
    assert!(!engine.project().layers[0]
        .transform
        .position
        .keys
        .iter()
        .any(|key| key.frame == 3));
    apply(&mut engine, frame, Property::Position, TrackEdit::ToggleKey).unwrap();
    assert_eq!(
        engine.project().layers[0]
            .transform
            .position
            .keys
            .iter()
            .filter(|key| key.frame == 3)
            .count(),
        1
    );
    roundtrip(engine.project());
}

#[test]
fn separated_axes_require_an_explicit_axis_and_other_axes_remain_unchanged() {
    let mut project = fixture(TimeDirection::Forward).snapshot();
    project.layers[0].transform.position.separate().unwrap();
    let mut engine = Engine::new(project.clone()).unwrap();
    assert!(apply(
        &mut engine,
        6.5,
        Property::Position,
        TrackEdit::Set {
            value: TrackValue::Vec3([1.; 3])
        }
    )
    .is_err());
    assert_eq!(engine.snapshot(), project);
    engine
        .apply(Command::TrackAt {
            target: KeyframeTarget::Property {
                object: 7,
                property: Property::Position,
                axis: Some(motion_core::Axis::X),
            },
            frame: 6.5,
            edit: TrackEdit::Set {
                value: TrackValue::Scalar(99.),
            },
        })
        .unwrap();
    let axes = engine.project().layers[0]
        .transform
        .position
        .axes
        .as_ref()
        .unwrap();
    assert_eq!(axes.get(motion_core::Axis::X).sample(3.).unwrap(), 99.);
    assert_eq!(
        axes.get(motion_core::Axis::Y),
        project.layers[0]
            .transform
            .position
            .axes
            .as_ref()
            .unwrap()
            .get(motion_core::Axis::Y)
    );
}

#[test]
fn invalid_targets_values_times_and_locked_layers_leave_project_and_history_unchanged() {
    let mut engine = fixture(TimeDirection::Forward);
    let before = engine.snapshot();
    for (frame, value) in [
        (f64::NAN, TrackValue::Scalar(0.5)),
        (-0.1, TrackValue::Scalar(0.5)),
        (60., TrackValue::Scalar(0.5)),
        (6.5, TrackValue::Scalar(f32::NAN)),
        (6.5, TrackValue::Vec2([1.; 2])),
        (6.5, TrackValue::Scalar(2.)),
    ] {
        assert!(apply(
            &mut engine,
            frame,
            Property::Opacity,
            TrackEdit::Set { value }
        )
        .is_err());
        assert_eq!(engine.snapshot(), before);
        assert!(!engine.can_undo());
    }
    let mut locked = before;
    locked.layers[0].locked = true;
    let mut engine = Engine::new(locked.clone()).unwrap();
    assert!(apply(
        &mut engine,
        6.5,
        Property::Position,
        TrackEdit::Set {
            value: TrackValue::Vec3([4.; 3])
        }
    )
    .is_err());
    assert_eq!(engine.snapshot(), locked);
}

#[test]
fn continuous_spatial_keys_keep_stable_identity_and_accept_fractional_local_edits() {
    let mut project = fixture(TimeDirection::Forward).snapshot();
    project.layers[0]
        .transform
        .position
        .enable_roving(&[3, 6])
        .unwrap();
    let mut engine = Engine::new(project).unwrap();
    let before_ids: Vec<_> = engine.project().layers[0]
        .transform
        .position
        .spatial_animation
        .as_ref()
        .unwrap()
        .keys()
        .iter()
        .map(|key| key.id)
        .collect();
    apply(
        &mut engine,
        6.6,
        Property::Position,
        TrackEdit::Set {
            value: TrackValue::Vec3([3.1, 16., 0.]),
        },
    )
    .unwrap();
    let spatial = engine.project().layers[0]
        .transform
        .position
        .spatial_animation
        .as_ref()
        .unwrap();
    assert_eq!(spatial.keys().len(), before_ids.len() + 1);
    for id in before_ids {
        assert!(spatial.keys().iter().any(|key| key.id == id));
    }
    assert_eq!(
        spatial.sample_value((6.6 - 2.) * 2. / 3.).unwrap(),
        [3.1, 16., 0.]
    );
    roundtrip(engine.project());
}

fn effect_fixture(kind: motion_effects::ParamKind, animated: bool) -> (Engine, KeyframeTarget) {
    let package = motion_effects::builtin::package().unwrap();
    let (definition, parameter) = package
        .manifest
        .effects
        .iter()
        .find_map(|definition| {
            definition
                .params
                .iter()
                .find(|p| p.kind == kind && p.implemented && p.animatable)
                .map(|parameter| (definition, parameter))
        })
        .unwrap();
    let mut instance = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64.; 2],
    );
    let p = instance.params.get_mut(&parameter.id).unwrap();
    if animated {
        if let Some(curve) = &mut p.curve {
            for frame in [0, 3, 6] {
                curve.keys.push(motion_core::Keyframe {
                    frame,
                    value: curve.value.clone(),
                    ease: Ease::Linear,
                    curve: None,
                    spatial: None,
                    auto_bezier: false,
                });
            }
        } else {
            for frame in [0, 3, 6] {
                p.track
                    .upsert(
                        frame,
                        p.default,
                        if kind.discrete() {
                            Ease::Hold
                        } else {
                            Ease::Linear
                        },
                    )
                    .unwrap();
            }
        }
    }
    let mut project = fixture(TimeDirection::Forward).snapshot();
    project.layers[0].effects.push(instance);
    project.rebuild_plugin_dependencies();
    (
        Engine::new(project).unwrap(),
        KeyframeTarget::Effect {
            object: 7,
            effect: 1,
            param: parameter.id.clone(),
        },
    )
}

#[test]
fn effect_numeric_and_discrete_parameters_use_the_same_exact_authoring_protocol() {
    for kind in [
        motion_effects::ParamKind::Float,
        motion_effects::ParamKind::Bool,
        motion_effects::ParamKind::Enum,
    ] {
        let (mut engine, target) = effect_fixture(kind, true);
        let before = engine.snapshot();
        let KeyframeTarget::Effect { param, .. } = &target else {
            unreachable!()
        };
        let value = engine.project().layers[0].effects[0].params[param].default;
        engine
            .apply(Command::TrackAt {
                target: target.clone(),
                frame: 6.5,
                edit: TrackEdit::Set {
                    value: TrackValue::Vec4(value),
                },
            })
            .unwrap();
        let keys = &engine.project().layers[0].effects[0].params[param]
            .track
            .keys;
        assert!(keys.iter().any(|key| key.frame == 3));
        if kind.discrete() {
            assert!(keys
                .iter()
                .all(|key| key.ease == Ease::Hold && key.curve.is_none()));
        }
        assert!(engine
            .apply(Command::TrackAt {
                target: target.clone(),
                frame: 6.6,
                edit: TrackEdit::Set {
                    value: TrackValue::Vec4(value)
                }
            })
            .is_err());
        roundtrip(engine.project());
        assert_eq!(
            engine.project().layers[0].time_mapping,
            before.layers[0].time_mapping
        );
    }
}

#[test]
fn static_only_effects_accept_fractional_value_edits_but_never_gain_animation_or_edit_unimplemented_parameters(
) {
    let (engine, target) = effect_fixture(motion_effects::ParamKind::Float, false);
    let KeyframeTarget::Effect { param, .. } = &target else {
        unreachable!()
    };
    let mut project = engine.snapshot();
    let p = project.layers[0].effects[0].params.get_mut(param).unwrap();
    p.animatable = false;
    let value = p.default;
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(Command::TrackAt {
            target: target.clone(),
            frame: 6.6,
            edit: TrackEdit::Set {
                value: TrackValue::Vec4(value),
            },
        })
        .unwrap();
    let before = engine.snapshot();
    for edit in [TrackEdit::Animate { enabled: true }, TrackEdit::ToggleKey] {
        assert!(engine
            .apply(Command::TrackAt {
                target: target.clone(),
                frame: 6.5,
                edit
            })
            .unwrap_err()
            .to_string()
            .contains("not animatable"));
        assert_eq!(engine.snapshot(), before);
    }
    let mut project = before;
    project.layers[0].effects[0]
        .params
        .get_mut(param)
        .unwrap()
        .implemented = false;
    let mut engine = Engine::new(project.clone()).unwrap();
    assert!(engine
        .apply(Command::TrackAt {
            target,
            frame: 6.6,
            edit: TrackEdit::Set {
                value: TrackValue::Vec4(value)
            }
        })
        .unwrap_err()
        .to_string()
        .contains("not implemented"));
    assert_eq!(engine.snapshot(), project);
}

#[test]
fn color_curve_disable_freezes_the_exact_fractional_sample_and_edit_preserves_timing() {
    let (mut engine, target) = effect_fixture(motion_effects::ParamKind::Curve, true);
    let KeyframeTarget::Effect { param, .. } = &target else {
        unreachable!()
    };
    let mut project = engine.snapshot();
    let curve = project.layers[0].effects[0]
        .params
        .get_mut(param)
        .unwrap()
        .curve
        .as_mut()
        .unwrap();
    curve.keys[1].value.channels[0] = vec![[0., 0.], [0.5, 0.8], [1., 1.]];
    curve.keys[1].ease = Ease::In;
    let expected = curve
        .sampled_curve((6.6 - 2.) * 2. / 3.)
        .unwrap()
        .high_precision_lut();
    engine = Engine::new(project).unwrap();
    engine
        .apply(Command::TrackAt {
            target: target.clone(),
            frame: 6.6,
            edit: TrackEdit::Animate { enabled: false },
        })
        .unwrap();
    let curve = engine.project().layers[0].effects[0].params[param]
        .curve
        .as_ref()
        .unwrap();
    assert!(curve.keys.is_empty());
    assert_eq!(
        curve.sampled_curve(0.).unwrap().high_precision_lut(),
        expected
    );
    assert!(curve.value.channel_luts.iter().all(Option::is_none));
    assert!(engine.undo().unwrap());
    let value = engine.project().layers[0].effects[0].params[param]
        .curve
        .as_ref()
        .unwrap()
        .keys[1]
        .value
        .clone();
    engine
        .apply(Command::TrackAt {
            target: target.clone(),
            frame: 6.5,
            edit: TrackEdit::Set {
                value: TrackValue::ColorCurve(Box::new(value)),
            },
        })
        .unwrap();
    let curve = engine.project().layers[0].effects[0].params[param]
        .curve
        .as_ref()
        .unwrap();
    assert_eq!(curve.keys[1].ease, Ease::In);
    roundtrip(engine.project());
}

#[test]
fn protocol_requires_exact_time_and_typed_values_and_rejects_unknown_fields() {
    let target = json!({"kind":"property","object":7,"property":"opacity","axis":null});
    let command =
        json!({"op":"track_at","target":target,"frame":6.5,"edit":{"kind":"set","value":0.7}});
    let parsed = parse_commands(&command.to_string()).unwrap();
    let Command::InComposition { command: inner, .. } = &parsed[0] else {
        panic!("missing explicit composition route")
    };
    assert_eq!(motion_core::CommandRegistry::id(inner), "track_at");
    let mut engine = fixture(TimeDirection::Forward);
    engine.apply_batch(parsed).unwrap();
    assert_eq!(
        engine.project().layers[0]
            .transform
            .opacity
            .sample(3.)
            .unwrap(),
        0.7
    );
    for (path, value) in [("frame", Value::Null), ("extra", json!(true))] {
        let mut invalid = command.clone();
        invalid[path] = value;
        assert!(parse_commands(&invalid.to_string()).is_err());
    }
    let mut wrong = command;
    wrong["edit"]["value"] = json!([1., 2., 3., 4., 5.]);
    assert!(parse_commands(&wrong.to_string()).is_err());
}

#[test]
fn exact_integer_projection_survives_inversion_error_without_accepting_nearby_composition_times() {
    for (n, d) in [(3, 7), (7, 3), (11, 13), (13, 11)] {
        for direction in [TimeDirection::Forward, TimeDirection::Reverse] {
            let clock = LayerClock::new(
                TimeStretch::new(n, d).unwrap(),
                direction,
                0.,
                if direction == TimeDirection::Forward {
                    0.
                } else {
                    10.
                },
            )
            .unwrap();
            for local in [1, 3, 7] {
                let frame = clock.composition_frame(f64::from(local), 2).unwrap();
                assert_eq!(clock.integer_key_frame(frame, 2).unwrap(), local);
                assert_eq!(clock.key_edit_time(frame, 2).unwrap(), f64::from(local));
                for nearby in [
                    frame - 0.000000001,
                    frame + 0.000000001,
                    frame.next_down(),
                    frame.next_up(),
                ] {
                    assert!(clock.integer_key_frame(nearby, 2).is_err());
                    assert_eq!(
                        clock.key_edit_time(nearby, 2).unwrap(),
                        clock.local_frame(nearby, 2).unwrap()
                    );
                }
            }
            let mut project = fixture(direction).snapshot();
            project.layers[0].time_mapping.as_mut().unwrap().clock = clock;
            let mut engine = Engine::new(project).unwrap();
            let frame = clock.composition_frame(3., 2).unwrap();
            apply(
                &mut engine,
                frame,
                Property::Opacity,
                TrackEdit::Set {
                    value: TrackValue::Scalar(0.8),
                },
            )
            .unwrap();
            assert_eq!(
                engine.project().layers[0]
                    .transform
                    .opacity
                    .keys
                    .iter()
                    .find(|key| key.frame == 3)
                    .unwrap()
                    .value,
                0.8
            );
            let before = engine.snapshot();
            assert!(apply(
                &mut engine,
                frame + 0.000000001,
                Property::Opacity,
                TrackEdit::Set {
                    value: TrackValue::Scalar(0.9)
                }
            )
            .is_err());
            assert_eq!(engine.snapshot(), before);
        }
    }
}

#[test]
fn projected_continuous_spatial_key_updates_its_stable_id_instead_of_inserting_a_nearby_key() {
    let mut project = fixture(TimeDirection::Forward).snapshot();
    let track = &mut project.layers[0].transform.position;
    track.enable_roving(&[3, 6]).unwrap();
    let spatial = track.spatial_animation.as_mut().unwrap();
    let id = spatial.keys()[1].id;
    let local = spatial.prepared().effective_time(id).unwrap();
    let clock = LayerClock::new(
        TimeStretch::new(3, 7).unwrap(),
        TimeDirection::Forward,
        0.,
        0.,
    )
    .unwrap();
    let frame = clock.composition_frame(local, 2).unwrap();
    let count = spatial.keys().len();
    project.layers[0].time_mapping.as_mut().unwrap().clock = clock;
    let mut engine = Engine::new(project).unwrap();
    apply(
        &mut engine,
        frame,
        Property::Position,
        TrackEdit::Set {
            value: TrackValue::Vec3([88., 16., 0.]),
        },
    )
    .unwrap();
    let spatial = engine.project().layers[0]
        .transform
        .position
        .spatial_animation
        .as_ref()
        .unwrap();
    assert_eq!(spatial.keys().len(), count);
    assert_eq!(
        spatial
            .keys()
            .iter()
            .find(|key| key.id == id)
            .unwrap()
            .value,
        [88., 16., 0.]
    );
}
