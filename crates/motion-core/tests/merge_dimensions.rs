use motion_core::{
    parse_commands, storage, Axis, CameraMode, Command, CommandRegistry, Curve, CurveShape,
    CurveSpace, Ease, Easing, Engine, ExpressionTarget, Layer, LayerTimeline, Project,
    ProjectExpressions, Property, PropertyExpression, Track, EXPRESSION_PROFILE,
};
use motion_render::Scene;
use serde_json::json;

fn project() -> Project {
    let mut p = Project::new(64, 64, 30, 120).unwrap();
    p.layers
        .push(Layer::solid(1, "Card", [20.; 2], [32., 32., 0.], [1.; 4]));
    p
}
fn merge(object: u64, property: Property) -> Command {
    Command::MergeDimensions { object, property }
}
fn animated(curve: Option<Curve>, ease: Ease) -> Track<[f32; 3]> {
    let mut t = Track::constant([4., -8., 720.]);
    t.upsert(-30, [20., -40., 720.], ease).unwrap();
    t.upsert(-5, [-40., 20., -1440.], ease).unwrap();
    t.upsert(45, [50., 60., 2160.], Ease::Linear).unwrap();
    t.set_curve(-30, Easing { ease, curve }).unwrap();
    t.set_curve(-5, Easing { ease, curve }).unwrap();
    t
}

#[test]
fn aligned_axes_round_trip_exactly_for_signed_times_holds_and_overshooting_curves() {
    let curves = [
        None,
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Quadratic {
                control: [0.4, 1.3],
                start: 0.,
                end: 1.,
            },
        }),
        Some(Curve {
            space: CurveSpace::Velocity,
            shape: CurveShape::Cubic {
                control1: [0.2, -1.5],
                control2: [0.8, 2.],
                start: 0.,
                end: 0.,
            },
        }),
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Elastic {
                oscillations: 3.,
                damping: 6.,
            },
        }),
    ];
    for curve in curves {
        for ease in [Ease::Linear, Ease::In, Ease::Out, Ease::InOut, Ease::Hold] {
            let original = animated(curve, ease);
            let mut t = original.clone();
            t.separate().unwrap();
            // Per-axis values can be edited independently without changing topology.
            t.axis_mut(Axis::Y).unwrap().keys[1].value = 123.;
            let separated = t.clone();
            t.merge_dimensions().unwrap();
            assert!(t.axes.is_none());
            assert_eq!(t.keys.len(), 3);
            assert_eq!(t.value, original.value);
            for i in -200..500 {
                let frame = f64::from(i) / 4.;
                assert_eq!(
                    t.sample(frame),
                    separated.sample(frame),
                    "{curve:?} {ease:?} {frame}"
                );
            }
            assert_eq!(t.sample(f64::NAN), separated.sample(f64::NAN));
            assert_eq!(
                t.sample(f64::NEG_INFINITY),
                separated.sample(f64::NEG_INFINITY)
            );
            assert_eq!(t.sample(f64::INFINITY), separated.sample(f64::INFINITY));
        }
    }
}

#[test]
fn static_axes_use_current_axis_values_and_already_coupled_vectors_are_noops() {
    let mut p = project();
    let transform = &mut p.layers[0].transform;
    for t in [
        &mut transform.position,
        &mut transform.rotation,
        &mut transform.scale,
    ] {
        t.separate().unwrap();
        t.axis_mut(Axis::X).unwrap().value = 345.;
        t.axis_mut(Axis::Y).unwrap().value = -720.;
    }
    let before = p.clone();
    let mut e = Engine::new(p).unwrap();
    for property in [Property::Position, Property::Rotation, Property::Scale] {
        e.apply(merge(1, property)).unwrap();
        let revision = e.revision();
        e.apply(merge(1, property)).unwrap();
        assert_eq!(e.revision(), revision);
    }
    let transform = &e.project().layers[0].transform;
    for (t, old) in [
        (&transform.position, &before.layers[0].transform.position),
        (&transform.rotation, &before.layers[0].transform.rotation),
        (&transform.scale, &before.layers[0].transform.scale),
    ] {
        assert!(t.axes.is_none() && t.keys.is_empty());
        assert_eq!(t.value, old.sample(12.5));
    }
    for _ in 0..3 {
        assert!(e.undo().unwrap());
    }
    assert_eq!(e.snapshot(), before);
    assert!(!e.can_undo());
}

#[test]
fn differing_key_counts_times_or_effective_interpolation_are_rejected_atomically() {
    for kind in 0..4 {
        let mut t = animated(None, Ease::InOut);
        t.separate().unwrap();
        let y = t.axis_mut(Axis::Y).unwrap();
        match kind {
            0 => {
                y.keys.remove(1);
            }
            1 => y.keys[1].frame += 1,
            2 => y.keys[0].ease = Ease::Hold,
            _ => y
                .set_curve(
                    -30,
                    Easing {
                        ease: Ease::Linear,
                        curve: Some(Curve {
                            space: CurveSpace::Progress,
                            shape: CurveShape::Elastic {
                                oscillations: 2.,
                                damping: 4.,
                            },
                        }),
                    },
                )
                .unwrap(),
        }
        let before = t.clone();
        let error = t.merge_dimensions().unwrap_err().to_string();
        assert!(
            error.contains(if kind < 2 {
                "matching key times"
            } else {
                "interpolation"
            }),
            "{error}"
        );
        assert_eq!(t, before);
        let mut p = project();
        p.layers[0].transform.rotation = t;
        let mut e = Engine::new(p).unwrap();
        e.apply(Command::Rename {
            object: 1,
            name: "redo retained".into(),
        })
        .unwrap();
        e.undo().unwrap();
        let before = e.snapshot();
        let revision = e.revision();
        // Even a successful merge earlier in this batch must roll back.
        let commands = parse_commands(
            r#"[
            {"op":"separate_dimensions","object":1,"property":"position"},
            {"op":"merge_dimensions","object":1,"property":"position"},
            {"op":"rename","object":1,"name":"must roll back"},
            {"op":"merge_dimensions","object":1,"property":"rotation"}
        ]"#,
        )
        .unwrap();
        assert!(CommandRegistry::run(&mut e, commands).is_err());
        assert_eq!(e.snapshot(), before);
        assert_eq!(e.revision(), revision);
        assert!(!e.can_undo());
        assert!(e.redo().unwrap());
        assert_eq!(e.project().layers[0].name, "redo retained");
    }
}

#[test]
fn custom_curves_override_ease_and_unused_final_key_timing_does_not_block_merge() {
    let curve = Curve {
        space: CurveSpace::Progress,
        shape: CurveShape::Elastic {
            oscillations: 2.,
            damping: 5.,
        },
    };
    let mut t = animated(Some(curve), Ease::Linear);
    t.separate().unwrap();
    let y = t.axis_mut(Axis::Y).unwrap();
    y.keys[0].ease = Ease::Hold;
    y.keys[1].ease = Ease::In;
    y.keys[2].ease = Ease::Out;
    y.keys[2].curve = Some(curve);
    let before = t.clone();
    t.merge_dimensions().unwrap();
    for i in -200..400 {
        assert_eq!(
            t.sample(f64::from(i) / 4.),
            before.sample(f64::from(i) / 4.)
        );
    }
}

#[test]
fn layer_offsets_scene_sampling_storage_and_package_keep_the_same_animation() {
    let mut p = project();
    p.version = 13;
    p.layers[0].timeline = Some(LayerTimeline {
        in_frame: -10,
        out_frame: 100,
        offset_frame: 20,
    });
    p.layers[0].transform.position = animated(None, Ease::Hold);
    p.layers[0].transform.rotation = animated(None, Ease::InOut);
    p.layers[0].transform.position.separate().unwrap();
    p.layers[0].transform.rotation.separate().unwrap();
    let mut e = Engine::new(p).unwrap();
    // Immutable pre-edit project is also the representation consumed by frozen jobs.
    let frozen = e.snapshot();
    let version = frozen.version;
    CommandRegistry::run(
        &mut e,
        parse_commands(
            r#"[
        {"op":"merge_dimensions","object":1,"property":"position"},
        {"op":"merge_dimensions","object":1,"property":"rotation"}
    ]"#,
        )
        .unwrap(),
    )
    .unwrap();
    let after = e.snapshot();
    assert_eq!(after.version, version);
    assert_eq!(after.layers[0].timeline, frozen.layers[0].timeline);
    let mut a = Scene::new(&frozen);
    let mut b = Scene::new(&after);
    for i in 0..400 {
        let frame = f64::from(i) / 4.;
        a.sample(&frozen, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        b.sample(&after, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        assert_eq!(a.world_matrix(1), b.world_matrix(1));
    }
    let folder = tempfile::tempdir().unwrap();
    storage::save(folder.path(), &after).unwrap();
    assert_eq!(storage::load(folder.path()).unwrap(), after);
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.path().join("project.json")).unwrap())
            .unwrap();
    let t = &json["layers"][0]["transform"]["position"];
    assert!(t.get("axes").is_none() && t["keys"].as_array().unwrap().len() == 3);
    let package = folder.path().join("merged.msproj");
    storage::export_package(folder.path(), &after, &package).unwrap();
    assert_eq!(
        storage::import_package(&package, &folder.path().join("import")).unwrap(),
        after
    );
    e.undo().unwrap();
    assert_eq!(e.snapshot(), frozen);
    assert!(!e.can_undo());
    e.redo().unwrap();
    assert_eq!(e.snapshot(), after);
}

#[test]
fn expressions_keep_targets_sources_seeds_key_metadata_and_fractional_samples() {
    for axis in [None, Some(Axis::X)] {
        let mut p = project();
        p.layers[0].transform.position = animated(None, Ease::InOut);
        p.layers[0].transform.position.separate().unwrap();
        p.layers[0].timeline = Some(LayerTimeline {
            in_frame: 0,
            out_frame: 120,
            offset_frame: 20,
        });
        p.expressions.push(PropertyExpression {
            target: ExpressionTarget::Property {
                object: 1,
                property: Property::Position,
                axis,
            },
            source: if axis.is_some() {
                "value + numKeys + key(1).value + valueAtTime(time - 0.025)"
            } else {
                "value + valueAtTime(time - 0.025) + [numKeys, 0, 0]"
            }
            .into(),
            enabled: true,
            seed: 93,
            profile: EXPRESSION_PROFILE.into(),
        });
        p.expressions.push(PropertyExpression {
            target: ExpressionTarget::Property {
                object: 1,
                property: Property::Rotation,
                axis: Some(Axis::Y),
            },
            source: "unfinished (".into(),
            enabled: false,
            seed: 123,
            profile: EXPRESSION_PROFILE.into(),
        });
        let mut e = Engine::new(p).unwrap();
        let before = e.snapshot();
        e.apply(merge(1, Property::Position)).unwrap();
        assert_eq!(e.project().expressions, before.expressions);
        for frame in [0., 7.25, 20.5, 30.125, 59.75, 99.5] {
            let a = before.evaluated_at(frame).unwrap();
            let b = e.project().evaluated_at(frame).unwrap();
            assert_eq!(
                a.layers[0].transform.position.sample(frame - 20.),
                b.layers[0].transform.position.sample(frame - 20.)
            );
        }
    }
}

#[test]
fn invalid_targets_locked_layers_and_camera_mode_are_checked_before_merging() {
    let mut p = project();
    p.layers[0].transform.position.separate().unwrap();
    p.layers[0].locked = true;
    let mut e = Engine::new(p).unwrap();
    for (object, property) in [
        (1, Property::Position),
        (99, Property::Position),
        (1, Property::Opacity),
        (0, Property::Target),
    ] {
        let before = e.snapshot();
        assert!(e.apply(merge(object, property)).is_err());
        assert_eq!(e.snapshot(), before);
        assert!(!e.can_undo());
    }
    let mut p = project();
    p.camera.created = true;
    let mut e = Engine::new(p).unwrap();
    assert!(e.apply(merge(1, Property::Opacity)).is_err());
    e.apply(Command::SeparateDimensions {
        object: 0,
        property: Property::Position,
    })
    .unwrap();
    let before = e.snapshot();
    e.apply(merge(0, Property::Position)).unwrap();
    e.undo().unwrap();
    assert_eq!(e.snapshot(), before);
    e.apply(Command::CameraMode {
        object: 0,
        mode: CameraMode::Orbit,
    })
    .unwrap();
    let before = e.snapshot();
    assert!(e.apply(merge(0, Property::Position)).is_err());
    assert_eq!(e.snapshot(), before);
    e.apply(Command::SeparateDimensions {
        object: 0,
        property: Property::Target,
    })
    .unwrap();
    e.apply(merge(0, Property::Target)).unwrap();
    assert!(e.project().camera.target.axes.is_none());
    assert!(Track::<f32>::constant(1.).merge_dimensions().is_err());
}

#[test]
fn composition_routing_and_gesture_cancel_do_not_touch_another_composition() {
    let mut e = Engine::new(project()).unwrap();
    CommandRegistry::run(&mut e, parse_commands(r#"{"op":"composition","action":{"kind":"precompose","objects":[1],"name":"child","range":"composition"}}"#).unwrap()).unwrap();
    let child = e.project().compositions[0].id.clone();
    let command = |op| {
        parse_commands(
            &json!({"op":op,"composition":child,"object":1,"property":"position"}).to_string(),
        )
        .unwrap()
    };
    CommandRegistry::run(&mut e, command("separate_dimensions")).unwrap();
    let before = e.snapshot();
    e.begin_gesture().unwrap();
    CommandRegistry::run(&mut e, command("merge_dimensions")).unwrap();
    assert_eq!(e.project().layers, before.layers);
    assert!(e.project().compositions[0].layers[0]
        .transform
        .position
        .axes
        .is_none());
    e.end_gesture(false).unwrap();
    assert_eq!(e.snapshot(), before);
    CommandRegistry::run(&mut e, command("merge_dimensions")).unwrap();
    let after = e.snapshot();
    e.undo().unwrap();
    assert_eq!(e.snapshot(), before);
    e.redo().unwrap();
    assert_eq!(e.snapshot(), after);
    assert!(parse_commands(
        r#"{"op":"merge_dimensions","object":1,"property":"position","axis":"x"}"#
    )
    .is_err());
}
