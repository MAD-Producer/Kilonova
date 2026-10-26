use super::*;
use motion_core::{
    Curve, CurveShape, CurveSpace, Easing,
    timing::{self, TimingInterpolation, TimingSettings, TimingSide, VelocityUnits},
};
#[path = "timing/continuity.rs"]
mod continuity;

fn settings(speed: f64) -> TimingSettings {
    TimingSettings {
        interpolation: TimingInterpolation::Preserve,
        units: VelocityUnits::PropertyPerSecond,
        incoming: None,
        outgoing: Some(TimingSide {
            speed: Some(speed),
            influence: Some(40.),
        }),
        match_speed: false,
    }
}
fn command(selections: Vec<KeyframeSelection>, settings: TimingSettings) -> Command {
    Command::KeyframeTiming {
        selections,
        settings,
    }
}
fn selected() -> Vec<KeyframeSelection> {
    vec![selection(property(7, Property::Position, None), &[10])]
}
fn easing<T>(key: &Keyframe<T>) -> Easing {
    Easing {
        ease: key.ease,
        curve: key.curve,
    }
}

#[test]
fn physical_endpoint_speed_uses_real_spatial_derivative_and_preserves_the_other_handle() {
    let mut engine = fixture();
    let before = engine.snapshot();
    // d(position)/d(progress) at the start is 3*[5,3,0], not the endpoint chord.
    let baseline = 15f64.hypot(9.) * 3.;
    engine
        .apply(command(selected(), settings(baseline * 0.5)))
        .unwrap();
    let mut expected = before.clone();
    expected.layers[0].transform.position.keys[0].ease = Ease::Linear;
    expected.layers[0].transform.position.keys[0].curve = Some(Curve {
        space: CurveSpace::Progress,
        shape: CurveShape::Cubic {
            control1: [0.4, 0.2],
            control2: [0.7, 0.9],
            start: 0.,
            end: 1.,
        },
    });
    assert_eq!(engine.project(), &expected);
    let profile = timing::profile(
        engine.project(),
        &selected(),
        VelocityUnits::PropertyPerSecond,
    )
    .unwrap();
    assert_eq!(profile.outgoing.speed, Some(baseline * 0.5));
    assert_eq!(profile.outgoing.influence, Some(40.));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
    engine.redo().unwrap();
    assert_eq!(engine.project(), &expected);
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), engine.project()).unwrap();
    assert_eq!(motion_core::storage::load(root.path()).unwrap(), expected);
}

#[test]
fn average_multiple_and_property_units_produce_the_same_authored_curve() {
    let mut absolute = fixture();
    let mut relative = fixture();
    let average = 90f64.hypot(20.) * 3.;
    absolute
        .apply(command(selected(), settings(average * 0.75)))
        .unwrap();
    let mut config = settings(0.75);
    config.units = VelocityUnits::AverageMultiple;
    relative.apply(command(selected(), config)).unwrap();
    assert_eq!(absolute.project(), relative.project());
    let profile = timing::profile(
        relative.project(),
        &selected(),
        VelocityUnits::AverageMultiple,
    )
    .unwrap();
    assert!((profile.outgoing.speed.unwrap() - 0.75).abs() < 1e-12);
    assert_eq!(profile.units.into_iter().collect::<Vec<_>>(), ["x average"]);
}

fn scalar_fixture() -> Engine {
    let mut project = fixture().snapshot();
    project.layers[0].transform.opacity.keys = [(3, 0.), (33, 0.5), (63, 0.75)]
        .into_iter()
        .map(|(frame, value)| Keyframe {
            frame,
            value,
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        })
        .collect();
    Engine::new(project).unwrap()
}
#[test]
fn selected_key_matches_real_rates_across_different_intervals() {
    let mut engine = scalar_fixture();
    let before = engine.snapshot();
    let target = property(7, Property::Opacity, None);
    let profile = timing::profile(
        engine.project(),
        &[selection(target.clone(), &[40])],
        VelocityUnits::PropertyPerSecond,
    )
    .unwrap();
    assert_eq!(profile.incoming.speed, Some(50.));
    assert_eq!(profile.outgoing.speed, Some(25.));
    let side = Some(TimingSide {
        speed: Some(10.),
        influence: Some(25.),
    });
    let config = TimingSettings {
        interpolation: TimingInterpolation::Bezier,
        units: VelocityUnits::PropertyPerSecond,
        incoming: side,
        outgoing: side,
        match_speed: true,
    };
    engine
        .apply(command(vec![selection(target.clone(), &[40])], config))
        .unwrap();
    let keys = &engine.project().layers[0].transform.opacity.keys;
    // The stored incoming handle is 1 - 0.2 * 0.25. Endpoint evaluation
    // subtracts it from 1 again, so compare the analytic derivative with a
    // numerical roundoff bound; authored project data remains exact below.
    assert!((easing(&keys[0]).sample(1.).velocity - 0.2).abs() < 1e-12);
    assert!((easing(&keys[1]).sample(0.).velocity - 0.4).abs() < 1e-12);
    let profile = timing::profile(
        engine.project(),
        &[selection(target, &[40])],
        VelocityUnits::PropertyPerSecond,
    )
    .unwrap();
    assert!((profile.incoming.speed.unwrap() - 10.).abs() < 1e-12);
    assert!((profile.outgoing.speed.unwrap() - 10.).abs() < 1e-12);
    for (old, new) in before.layers[0].transform.opacity.keys.iter().zip(keys) {
        assert_eq!(
            (old.frame, old.value, &old.spatial),
            (new.frame, new.value, &new.spatial)
        );
    }
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
}

#[test]
fn anchor_units_use_source_pixels_and_signed_separated_axes_keep_local_offsets() {
    let mut project = fixture().snapshot();
    project.layers[0].size = [200., 100.];
    project.layers[0].transform.anchor.keys = [(3, [0., 0.]), (33, [0.3, 0.8])]
        .into_iter()
        .map(|(frame, value)| Keyframe {
            frame,
            value,
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        })
        .collect();
    let targets = vec![selection(property(7, Property::Anchor, None), &[10])];
    let p = timing::profile(&project, &targets, VelocityUnits::PropertyPerSecond).unwrap();
    let expected = (f64::from(0.3f32) * 200.).hypot(f64::from(0.8f32) * 100.);
    assert_eq!(p.outgoing.speed, Some(expected));
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(command(targets, settings(expected * 0.5)))
        .unwrap();
    assert_eq!(
        easing(&engine.project().layers[0].transform.anchor.keys[0])
            .sample(0.)
            .velocity,
        0.5
    );
    let mut project = scalar_fixture().snapshot();
    project.layers[0].timeline.as_mut().unwrap().offset_frame = 20;
    for key in &mut project.layers[0].transform.position.keys {
        key.spatial = None;
    }
    project.layers[0].transform.position.separate().unwrap();
    let x = project.layers[0].transform.position.axes.as_mut().unwrap();
    x.x.keys[0].frame = -10;
    x.x.keys[0].value = 100.;
    x.x.keys[0].curve = None;
    x.x.keys[0].ease = Ease::Linear;
    x.x.keys[1].frame = 20;
    x.x.keys[1].value = 10.;
    let target = selection(property(7, Property::Position, Some(Axis::X)), &[10]);
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(command(vec![target.clone()], settings(-45.)))
        .unwrap();
    let profile = timing::profile(
        engine.project(),
        &[target],
        VelocityUnits::PropertyPerSecond,
    )
    .unwrap();
    assert_eq!(profile.outgoing.speed, Some(-45.));
    assert_eq!(
        engine.project().layers[0]
            .transform
            .position
            .axes
            .as_ref()
            .unwrap()
            .x
            .keys[0]
            .frame,
        -10
    );
}

#[test]
fn timing_rejections_preserve_all_channels_revision_and_history_including_late_failures() {
    let mut engine = fixture();
    for (config, cause) in [
        (settings(f64::NAN), "speed must be finite"),
        (settings(f64::INFINITY), "speed must be finite"),
        (
            TimingSettings {
                outgoing: Some(TimingSide {
                    speed: None,
                    influence: Some(0.),
                }),
                ..settings(1.)
            },
            "influence must be greater than zero",
        ),
        (
            TimingSettings {
                outgoing: Some(TimingSide {
                    speed: None,
                    influence: Some(101.),
                }),
                ..settings(1.)
            },
            "influence must be greater than zero",
        ),
        (
            TimingSettings {
                interpolation: TimingInterpolation::Hold,
                ..settings(1.)
            },
            "linear and hold interpolation cannot set",
        ),
    ] {
        rejected(&mut engine, command(selected(), config), cause);
    }
    let valid = selected()[0].clone();
    for (bad, cause) in [
        (
            selection(property(999, Property::Position, None), &[10]),
            "999",
        ),
        (
            selection(property(7, Property::Opacity, None), &[11]),
            "does not exist",
        ),
        (valid.clone(), "duplicate keyframe channel"),
    ] {
        rejected(
            &mut engine,
            command(vec![valid.clone(), bad], settings(1.)),
            cause,
        );
    }
    let mut project = engine.snapshot();
    project.layers[1].locked = true;
    let mut engine = Engine::new(project).unwrap();
    rejected(
        &mut engine,
        command(
            vec![
                valid,
                selection(property(9, Property::Position, None), &[43]),
            ],
            settings(1.),
        ),
        "locked",
    );
    rejected(
        &mut engine,
        command(
            vec![selection(property(7, Property::Opacity, None), &[15])],
            settings(0.),
        ),
        "no incident interval",
    );
}

#[test]
fn velocity_space_conversion_requires_explicit_choice_and_never_changes_values_or_spatial_tangents()
{
    let mut project = fixture().snapshot();
    project.layers[0].transform.position.keys[0].curve = Some(Curve {
        space: CurveSpace::Velocity,
        shape: CurveShape::Cubic {
            control1: [0.2, 0.8],
            control2: [0.8, 0.4],
            start: 0.2,
            end: 0.6,
        },
    });
    let mut engine = Engine::new(project).unwrap();
    let before = engine.snapshot();
    let profile = timing::profile(&before, &selected(), VelocityUnits::PropertyPerSecond).unwrap();
    assert!(!profile.progress_editable);
    assert!(profile.curve_spaces.contains("velocity"));
    rejected(
        &mut engine,
        command(selected(), settings(1.)),
        "choose Bezier explicitly",
    );
    let mut config = settings(1.);
    config.interpolation = TimingInterpolation::Bezier;
    engine.apply(command(selected(), config)).unwrap();
    assert_eq!(
        engine.project().layers[0].transform.position.keys[0]
            .curve
            .unwrap()
            .space,
        CurveSpace::Progress
    );
    let mut actual = engine.snapshot();
    actual.layers[0].transform.position.keys[0].curve =
        before.layers[0].transform.position.keys[0].curve;
    actual.layers[0].transform.position.keys[0].ease =
        before.layers[0].transform.position.keys[0].ease;
    assert_eq!(actual, before);
}

#[test]
fn timing_command_is_registered_and_can_edit_an_inactive_composition_atomically() {
    assert!(
        motion_core::CommandRegistry::catalogue()["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"] == "keyframe_timing")
    );
    let mut engine = fixture();
    engine
        .apply(Command::Composition {
            action: motion_core::CompositionAction::Create {
                settings: motion_core::CompositionSettings {
                    name: "Child".into(),
                    width: 640,
                    height: 360,
                    fps: 30,
                    frames: 120,
                    timing: "preserve_seconds".into(),
                    shorten: "reject".into(),
                },
            },
        })
        .unwrap();
    let child = engine.project().compositions[0].id.clone();
    for layer in fixture().project().layers.clone() {
        engine
            .apply(Command::InComposition {
                composition: child.clone(),
                command: Box::new(Command::Add { layer }),
            })
            .unwrap();
    }
    let before = engine.snapshot();
    let command = Command::InComposition {
        composition: child,
        command: Box::new(command(selected(), settings(1.))),
    };
    let parsed =
        motion_core::CommandRegistry::parse(&serde_json::to_string(&command).unwrap()).unwrap();
    motion_core::CommandRegistry::run(&mut engine, parsed).unwrap();
    let changed = engine.snapshot();
    assert_eq!(changed.layers, before.layers);
    assert_ne!(changed.compositions, before.compositions);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn adjacent_selected_keys_merge_both_handles_of_the_shared_interval_before_writing() {
    let mut engine = scalar_fixture();
    let before = engine.snapshot();
    let side = Some(TimingSide {
        speed: Some(15.),
        influence: Some(20.),
    });
    engine
        .apply(command(
            vec![selection(property(7, Property::Opacity, None), &[10, 40])],
            TimingSettings {
                interpolation: TimingInterpolation::Bezier,
                units: VelocityUnits::PropertyPerSecond,
                incoming: side,
                outgoing: side,
                match_speed: true,
            },
        ))
        .unwrap();
    let keys = &engine.project().layers[0].transform.opacity.keys;
    let Some(Curve {
        shape: CurveShape::Cubic {
            control1, control2, ..
        },
        ..
    }) = keys[0].curve
    else {
        panic!("shared interval not configured")
    };
    assert_eq!(control1, [0.2, 0.3 * 0.2]);
    assert_eq!(control2, [0.8, 1. - 0.3 * 0.2]);
    assert!(keys[1].curve.is_some());
    assert_eq!(keys[2], before.layers[0].transform.opacity.keys[2]);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
}

#[test]
fn grouped_and_ungrouped_vector_channels_keep_every_value_when_changing_influence() {
    for grouped in [false, true] {
        let (mut engine, targets) = vector_fixture(grouped);
        let selected = targets
            .iter()
            .cloned()
            .map(|t| selection(t, &[10]))
            .collect::<Vec<_>>();
        let frozen = KeyframeClipboard::capture(engine.project(), &selected).unwrap();
        engine.apply(paste(frozen, targets, Some(20))).unwrap();
        let before = engine.snapshot();
        let mut engine = Engine::new(before.clone()).unwrap();
        let profile =
            timing::profile(&before, &selected, VelocityUnits::PropertyPerSecond).unwrap();
        assert_eq!(profile.outgoing.intervals, selected.len());
        assert!(profile.numeric && profile.temporal);
        engine
            .apply(command(
                selected.clone(),
                TimingSettings {
                    interpolation: TimingInterpolation::Bezier,
                    units: VelocityUnits::PropertyPerSecond,
                    incoming: None,
                    outgoing: Some(TimingSide {
                        speed: None,
                        influence: Some(35.),
                    }),
                    match_speed: false,
                },
            ))
            .unwrap();
        let old = KeyframeClipboard::capture(&before, &selected).unwrap();
        let new = KeyframeClipboard::capture(engine.project(), &selected).unwrap();
        let mut old = serde_json::to_value(old).unwrap();
        let new = serde_json::to_value(new).unwrap();
        for (a, b) in old["tracks"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(new["tracks"].as_array().unwrap())
        {
            for (a, b) in a["data"]["keys"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .zip(b["data"]["keys"].as_array().unwrap())
            {
                a["ease"] = b["ease"].clone();
                a["curve"] = b["curve"].clone();
            }
        }
        assert_eq!(old, new, "only temporal metadata may change");
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
    }
}

#[test]
fn color_curve_keys_allow_relative_progress_rates_but_reject_physical_speed_without_history() {
    let (mut engine, target) = effects_fixture("curves");
    let selected = vec![selection(target.clone(), &[10])];
    let frozen = KeyframeClipboard::capture(engine.project(), &selected).unwrap();
    engine.apply(paste(frozen, vec![target], Some(20))).unwrap();
    let mut engine = Engine::new(engine.snapshot()).unwrap();
    let before = engine.snapshot();
    let mut config = settings(0.5);
    config.interpolation = TimingInterpolation::Bezier;
    rejected(
        &mut engine,
        command(selected.clone(), config),
        "property-unit speed is unavailable",
    );
    config.units = VelocityUnits::AverageMultiple;
    engine.apply(command(selected.clone(), config)).unwrap();
    let profile =
        timing::profile(engine.project(), &selected, VelocityUnits::AverageMultiple).unwrap();
    assert_eq!(profile.outgoing.speed, Some(0.5));
    assert!(!profile.numeric && profile.temporal);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
}

#[test]
fn unrepresentable_speeds_and_stationary_endpoints_fail_without_clamping_or_partial_edits() {
    let mut engine = scalar_fixture();
    let targets = vec![selection(property(7, Property::Opacity, None), &[10])];
    rejected(
        &mut engine,
        command(targets.clone(), settings(1e10)),
        "Bezier control is outside its numeric range",
    );
    let mut project = engine.snapshot();
    project.layers[0].transform.opacity.keys[1].value =
        project.layers[0].transform.opacity.keys[0].value;
    let mut engine = Engine::new(project).unwrap();
    rejected(
        &mut engine,
        command(targets, settings(1.)),
        "stationary keyframe endpoint cannot have nonzero speed",
    );
}

#[test]
fn preserve_with_an_unchanged_side_keeps_that_entire_velocity_interval_exact() {
    let mut project = scalar_fixture().snapshot();
    project.layers[0].transform.opacity.keys[1].curve = Some(Curve {
        space: CurveSpace::Velocity,
        shape: CurveShape::Cubic {
            control1: [0.2, 0.8],
            control2: [0.8, 0.4],
            start: 0.2,
            end: 0.6,
        },
    });
    let mut engine = Engine::new(project).unwrap();
    let before = engine.snapshot();
    let selected = vec![selection(property(7, Property::Opacity, None), &[40])];
    let profile = timing::profile(&before, &selected, VelocityUnits::PropertyPerSecond).unwrap();
    assert!(profile.incoming.progress_editable && !profile.outgoing.progress_editable);
    engine
        .apply(command(
            selected,
            TimingSettings {
                interpolation: TimingInterpolation::Preserve,
                units: VelocityUnits::PropertyPerSecond,
                incoming: Some(TimingSide {
                    speed: Some(20.),
                    influence: None,
                }),
                outgoing: Some(Default::default()),
                match_speed: false,
            },
        ))
        .unwrap();
    assert_eq!(
        engine.project().layers[0].transform.opacity.keys[1..],
        before.layers[0].transform.opacity.keys[1..]
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
}

#[test]
fn vector_physical_speed_is_a_nonnegative_magnitude_and_preserves_existing_reverse_direction() {
    let mut project = fixture().snapshot();
    let Some(Curve {
        shape: CurveShape::Cubic { control1, .. },
        ..
    }) = &mut project.layers[0].transform.position.keys[0].curve
    else {
        panic!()
    };
    control1[1] = -0.1;
    let mut engine = Engine::new(project).unwrap();
    let baseline = 15f64.hypot(9.) * 3.;
    let p = timing::profile(
        engine.project(),
        &selected(),
        VelocityUnits::PropertyPerSecond,
    )
    .unwrap();
    assert_eq!(p.outgoing.speed, Some(baseline * 0.5));
    rejected(
        &mut engine,
        command(selected(), settings(-1.)),
        "vector speed must be nonnegative",
    );
    engine
        .apply(command(selected(), settings(baseline * 0.25)))
        .unwrap();
    let Some(Curve {
        shape: CurveShape::Cubic { control1, .. },
        ..
    }) = engine.project().layers[0].transform.position.keys[0].curve
    else {
        panic!()
    };
    assert_eq!(control1, [0.4, -0.1]);
}
