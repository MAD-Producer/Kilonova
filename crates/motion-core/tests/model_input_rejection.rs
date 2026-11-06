//! Public project and transaction boundaries must reject malformed input as
//! native errors. These controls also keep real undo and redo branches alive.
use motion_core::{Command, Ease, Engine, Error, Layer, Project, Property};
use serde_json::{json, Value};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn project() -> Project {
    let mut project = Project::new(64, 48, 30, 90).unwrap();
    let mut layer = Layer::solid(1, "输入校验", [32., 24.], [16., 12., 0.], [1.; 4]);
    layer
        .transform
        .position
        .upsert(0, [16., 12., 0.], Ease::Linear)
        .unwrap();
    layer
        .transform
        .position
        .upsert(30, [48., 36., 0.], Ease::Linear)
        .unwrap();
    project.layers.push(layer);
    project.validate().unwrap();
    project
}

fn invalid(error: Error, expected: &str) {
    let Error::Invalid(cause) = error else {
        panic!("expected a native validation error for {expected}: {error:?}");
    };
    assert!(
        cause.contains(expected),
        "expected {expected:?}, received {cause:?}"
    );
}

#[test]
fn malformed_required_fields_and_truncated_wire_return_decode_errors_without_panicking() {
    let wire = serde_json::to_value(project()).unwrap();
    // These are mandatory current-schema fields, not semantically optional data.
    let fields = [
        "version",
        "library",
        "working_precision",
        "composition_id",
        "name",
        "width",
        "height",
        "fps",
        "frames",
        "work_area",
        "background",
        "assets",
        "model_assets",
        "tracking_assets",
        "camera",
        "layers",
    ];
    for field in fields {
        for remove in [false, true] {
            let mut malformed = wire.clone();
            if remove {
                malformed.as_object_mut().unwrap().remove(field).unwrap();
            } else {
                *malformed.get_mut(field).unwrap() = Value::Null;
            }
            let result = catch_unwind(|| serde_json::from_value::<Project>(malformed));
            let error = result
                .unwrap_or_else(|_| panic!("decoder panicked: field={field} remove={remove}"))
                .expect_err("malformed required field was accepted");
            assert!(error.is_data(), "field={field} remove={remove}: {error}");
            assert!(
                error.to_string().contains(field) || !remove,
                "missing field context: {error}"
            );
        }
    }
    for (pointer, value) in [
        ("/fps/numerator", json!(0)),
        ("/fps/denominator", json!(0)),
        ("/layers/0/transform/position/value", json!([1, 2])),
        (
            "/layers/0/transform/position/keys/0/value",
            json!([1, 2, 3, 4]),
        ),
        ("/layers/0/content/kind", json!("unknown_source")),
    ] {
        let mut malformed = wire.clone();
        *malformed.pointer_mut(pointer).expect("fixture path exists") = value;
        let error = catch_unwind(|| serde_json::from_value::<Project>(malformed))
            .unwrap_or_else(|_| panic!("decoder panicked: {pointer}"))
            .expect_err("malformed field was accepted");
        assert!(error.is_data(), "{pointer}: {error}");
    }
    let bytes = serde_json::to_vec(&wire).unwrap();
    // Includes cuts within UTF-8 text and nested animation stores; no parser
    // limit is disabled and no unbounded recursive input is generated.
    for end in (0..bytes.len()).step_by(17).chain([bytes.len() - 1]) {
        let error = catch_unwind(|| serde_json::from_slice::<Project>(&bytes[..end]))
            .unwrap_or_else(|_| panic!("decoder panicked at byte {end}"))
            .expect_err("truncated project was accepted");
        assert!(
            error.is_eof() || error.is_syntax() || error.is_data(),
            "byte={end}: {error}"
        );
    }
    let reopened: Project = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(reopened, project());
    Engine::new(reopened).unwrap();
}

#[test]
fn decoded_invalid_projects_preserve_native_causes_at_validation_and_engine_construction() {
    let wire = serde_json::to_value(project()).unwrap();
    let cases = [
        ("/version", json!(0), "unsupported project format"),
        ("/version", json!(u32::MAX), "unsupported project format"),
        ("/width", json!(0), "invalid composition size"),
        ("/width", json!(8193), "invalid composition size"),
        ("/height", json!(0), "invalid composition size"),
        ("/height", json!(8193), "invalid composition size"),
        (
            "/fps",
            json!({"numerator":1,"denominator":2}),
            "composition frame rate",
        ),
        (
            "/fps",
            json!({"numerator":241,"denominator":1}),
            "composition frame rate",
        ),
        ("/frames", json!(0), "invalid composition duration"),
        ("/frames", json!(36001), "invalid composition duration"),
        ("/work_area/in_frame", json!(90), "invalid work area"),
        ("/work_area/out_frame", json!(91), "invalid work area"),
        ("/name", json!("x".repeat(1025)), "project name too long"),
        (
            "/layers/0/id",
            json!(0),
            "layer IDs must be unique and nonzero",
        ),
        (
            "/layers/0/name",
            json!("x".repeat(1025)),
            "layer name too long",
        ),
        ("/layers/0/size/0", json!(0), "invalid layer size"),
        ("/layers/0/size/1", json!(32769), "invalid layer size"),
        (
            "/layers/0/transform/opacity/value",
            json!(-0.01),
            "opacity must be between zero and one",
        ),
        (
            "/layers/0/transform/opacity/value",
            json!(1.01),
            "opacity must be between zero and one",
        ),
        (
            "/layers/0/transform/position/keys/1/frame",
            json!(0),
            "keyframes must be unique and sorted",
        ),
        (
            "/layers/0/transform/position/keys/0/frame",
            json!(31),
            "keyframes must be unique and sorted",
        ),
    ];
    for (pointer, value, cause) in cases {
        let mut malformed = wire.clone();
        *malformed.pointer_mut(pointer).expect("fixture path exists") = value;
        // Decoding must succeed here, so a parser rejection cannot substitute
        // for testing the model's native validation channel.
        let project: Project = serde_json::from_value(malformed).unwrap();
        let before = serde_json::to_vec(&project).unwrap();
        invalid(
            catch_unwind(|| project.validate())
                .expect("validation panicked")
                .unwrap_err(),
            cause,
        );
        assert_eq!(
            serde_json::to_vec(&project).unwrap(),
            before,
            "validation changed {pointer}"
        );
        let result = catch_unwind(|| Engine::new(project));
        match result.expect("engine construction panicked") {
            Err(error) => invalid(error, cause),
            Ok(_) => panic!("engine accepted malformed project: {pointer}"),
        }
    }
    let mut duplicate = project();
    duplicate.layers.push(duplicate.layers[0].clone());
    invalid(
        duplicate.validate().unwrap_err(),
        "layer IDs must be unique and nonzero",
    );
    let mut missing = project();
    missing.layers[0].content = motion_core::Content::Image { asset: 999 };
    assert!(catch_unwind(|| missing.validate())
        .expect("missing asset validation panicked")
        .is_err());
}

#[test]
fn nonfinite_in_memory_tracks_return_errors_instead_of_being_flattened_by_json() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for animated in [false, true] {
            for component in 0..3 {
                let mut project = project();
                let track = &mut project.layers[0].transform.position;
                if animated {
                    track.keys[0].value[component] = value;
                } else {
                    track.value[component] = value;
                }
                let expected = if animated {
                    "keyframe contains a non-finite value"
                } else {
                    "track contains a non-finite static value"
                };
                invalid(
                    catch_unwind(|| project.validate())
                        .expect("nonfinite validation panicked")
                        .unwrap_err(),
                    expected,
                );
                match catch_unwind(|| Engine::new(project))
                    .expect("nonfinite engine construction panicked")
                {
                    Err(error) => invalid(error, expected),
                    Ok(_) => panic!("engine accepted nonfinite track"),
                }
            }
        }
    }
}

#[test]
fn malformed_batch_suffixes_restore_project_revision_and_both_history_branches() {
    let mut engine = Engine::new(project()).unwrap();
    engine
        .apply(Command::Rename {
            object: 1,
            name: "first edit".into(),
        })
        .unwrap();
    let first = engine.snapshot();
    engine
        .apply(Command::Rename {
            object: 1,
            name: "second edit".into(),
        })
        .unwrap();
    let second = engine.snapshot();
    assert!(engine.undo().unwrap());
    assert!(engine.can_undo() && engine.can_redo());
    let cases = vec![
        Command::SetVector {
            object: 999,
            property: Property::Position,
            frame: 0,
            value: [0.; 3],
        },
        Command::SetVector {
            object: 1,
            property: Property::Position,
            frame: 0,
            value: [f32::NAN, 0., 0.],
        },
        Command::SetVector {
            object: 1,
            property: Property::Scale,
            frame: 0,
            value: [f32::INFINITY, 1., 1.],
        },
        Command::SetScalar {
            object: 1,
            property: Property::Opacity,
            frame: 0,
            value: f32::NEG_INFINITY,
        },
        Command::Add {
            layer: engine.project().layers[0].clone(),
        },
        Command::SetWorkArea {
            in_frame: 80,
            out_frame: 20,
        },
    ];
    for suffix in cases {
        let before = engine.snapshot();
        let revision = engine.revision();
        let history = engine.history_diagnostics();
        let result = catch_unwind(AssertUnwindSafe(|| {
            engine.apply_batch(vec![
                Command::Rename {
                    object: 1,
                    name: "must roll back".into(),
                },
                suffix.clone(),
            ])
        }));
        let error = result.expect("malformed batch panicked").unwrap_err();
        match error {
            Error::Missing(999) | Error::Invalid(_) => {}
            other => panic!("unexpected error identity for {suffix:?}: {other:?}"),
        }
        assert_eq!(
            engine.snapshot(),
            before,
            "partial project publication: {suffix:?}"
        );
        assert_eq!(engine.revision(), revision, "revision changed: {suffix:?}");
        assert_eq!(
            engine.history_diagnostics(),
            history,
            "history changed: {suffix:?}"
        );
        assert!(engine.can_undo() && engine.can_redo());
    }
    assert!(engine.redo().unwrap());
    assert_eq!(engine.snapshot(), second);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.snapshot(), first);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.snapshot(), project());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.snapshot(), first);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.snapshot(), second);
}
