use super::*;
use serde_json::{json, Value};
use std::error::Error as _;

fn fixture() -> Value {
    json!({
        "version": VERSION, "source_id": "plate-1", "source_sha256": "07".repeat(32),
        "source_extent": [640, 480], "coordinates": "right_down_forward",
        "ticks_per_second": 90000, "nominal_rate": [30000, 1001],
        "source_pts": [-3003, 0, 3004], "meters_per_world_unit": null,
        "tracks": [{"id": "point-1", "kind": "point", "reference_index": null,
            "samples": [
                {"source_index": 0, "visibility": "visible", "confidence": 0.75,
                 "observation": {"kind": "point", "pixels": [12.25, -3.5]}, "manual_correction": true},
                {"source_index": 2, "visibility": "lost", "confidence": 0.0,
                 "observation": null, "manual_correction": false}
            ]}]
    })
}
fn decode(value: &Value) -> Result<Asset, Error> {
    from_slice(&serde_json::to_vec(value).unwrap())
}
fn camera(value: &mut Value) {
    value["meters_per_world_unit"] = json!(0.01);
    value["tracks"][0]["kind"] = json!("camera");
    value["tracks"][0]["samples"][0]["observation"] = json!({
        "kind": "camera", "camera_to_world": {"position": [1.0, 2.0, -5.0],
            "orientation_xyzw": [0.0, 0.0, 0.0, 1.0]},
        "calibration": {"source_extent": [640, 480], "focal": [400.0, 410.0],
            "principal": [320.5, 240.5], "radial": [0.01, -0.001, 0.0001], "tangential": [0.002, -0.003]}
    });
}
fn json_reason(error: Error) -> &'static str {
    match error {
        Error::Json {
            reason,
            line,
            column,
        } => {
            assert!(line >= 1 && column >= 1);
            reason
        }
        other => panic!("expected JSON boundary error, got {other}"),
    }
}
#[test]
fn point_import_preserves_exact_source_timing_sparse_visibility_and_manual_state() {
    let a = decode(&fixture()).unwrap();
    assert_eq!(a.source_sha256, [7; 32]);
    assert_eq!(a.source_pts, [-3003, 0, 3004]);
    assert_eq!(a.source_time(2), Some((3004, 90000)));
    assert_eq!(a.nominal_rate.ratio(), [30000, 1001]);
    let samples = &a.tracks[0].samples;
    assert_eq!(samples.len(), 2);
    assert!(samples[0].manual_correction);
    assert!(matches!(
        samples[0].observation,
        Some(Observation::Point([12.25, -3.5]))
    ));
    assert_eq!(samples[1].source_index, 2);
    assert_eq!(samples[1].visibility, Visibility::Lost);
    assert!(samples[1].observation.is_none());
}
#[test]
fn plane_and_camera_import_preserve_reference_units_pose_and_lens_data() {
    let mut p = fixture();
    p["tracks"][0]["kind"] = json!("plane");
    p["tracks"][0]["reference_index"] = json!(1);
    p["tracks"][0]["samples"][0]["observation"] = json!({"kind":"plane", "matrix":
        [[2.0, 0.0, 10.0], [0.0, 2.0, -4.0], [0.0, 0.0, 2.0]]});
    let a = decode(&p).unwrap();
    assert_eq!(a.tracks[0].reference_index, Some(1));
    assert!(matches!(
        a.tracks[0].samples[0].observation,
        Some(Observation::Plane([
            [2.0, 0.0, 10.0],
            [0.0, 2.0, -4.0],
            [0.0, 0.0, 2.0]
        ]))
    ));
    let mut c = fixture();
    camera(&mut c);
    let a = decode(&c).unwrap();
    assert_eq!(a.meters_per_world_unit, Some(0.01));
    let Some(Observation::Camera {
        camera_to_world,
        calibration,
    }) = a.tracks[0].samples[0].observation
    else {
        panic!("camera missing")
    };
    assert_eq!(camera_to_world.position, [1.0, 2.0, -5.0]);
    assert_eq!(camera_to_world.orientation_xyzw, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(calibration.focal(), [400.0, 410.0]);
    assert_eq!(calibration.principal(), [320.5, 240.5]);
    assert_eq!(calibration.radial(), [0.01, -0.001, 0.0001]);
    assert_eq!(calibration.tangential(), [0.002, -0.003]);
}
#[test]
fn unknown_fields_are_rejected_at_every_object_boundary() {
    let paths = [
        "",
        "/tracks/0",
        "/tracks/0/samples/0",
        "/tracks/0/samples/0/observation",
        "/tracks/0/samples/0/observation/camera_to_world",
        "/tracks/0/samples/0/observation/calibration",
    ];
    for path in paths {
        let mut value = fixture();
        camera(&mut value);
        value
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected_secret".into(), json!("private"));
        assert_eq!(
            json_reason(decode(&value).unwrap_err()),
            "unknown field",
            "{path}"
        );
    }
}
#[test]
fn required_metadata_and_manual_intent_are_never_defaulted() {
    for field in [
        "version",
        "source_id",
        "source_sha256",
        "source_extent",
        "coordinates",
        "ticks_per_second",
        "nominal_rate",
        "source_pts",
        "tracks",
    ] {
        let mut value = fixture();
        value.as_object_mut().unwrap().remove(field);
        assert_eq!(
            json_reason(decode(&value).unwrap_err()),
            "missing required field",
            "{field}"
        );
    }
    let mut value = fixture();
    value["tracks"][0]["samples"][0]
        .as_object_mut()
        .unwrap()
        .remove("manual_correction");
    assert_eq!(
        json_reason(decode(&value).unwrap_err()),
        "missing required field"
    );
    // Only semantically optional fields may be omitted.
    let mut value = fixture();
    value
        .as_object_mut()
        .unwrap()
        .remove("meters_per_world_unit");
    value["tracks"][0]
        .as_object_mut()
        .unwrap()
        .remove("reference_index");
    value["tracks"][0]["samples"][1]
        .as_object_mut()
        .unwrap()
        .remove("observation");
    assert!(decode(&value).is_ok());
}
#[test]
fn duplicate_keys_and_trailing_documents_are_not_hidden_by_a_value_intermediate() {
    let raw = serde_json::to_string(&fixture()).unwrap();
    let duplicate = format!("{{\"version\":1,{}", &raw[1..]);
    assert_eq!(
        json_reason(from_slice(duplicate.as_bytes()).unwrap_err()),
        "duplicate field"
    );
    let duplicate = raw.replace(
        "\"pixels\":[12.25,-3.5]",
        "\"pixels\":[12.25,-3.5],\"pixels\":[0,0]",
    );
    assert_ne!(raw, duplicate);
    assert_eq!(
        json_reason(from_slice(duplicate.as_bytes()).unwrap_err()),
        "duplicate field"
    );
    assert_eq!(
        json_reason(from_slice(format!("{raw}{raw}").as_bytes()).unwrap_err()),
        "invalid syntax"
    );
}
#[test]
fn unsupported_enum_values_shapes_and_recursion_are_rejected() {
    for pointer in [
        "/coordinates",
        "/tracks/0/kind",
        "/tracks/0/samples/0/visibility",
        "/tracks/0/samples/0/observation/kind",
    ] {
        let mut value = fixture();
        *value.pointer_mut(pointer).unwrap() = json!("unsupported");
        assert_eq!(
            json_reason(decode(&value).unwrap_err()),
            "unsupported enum value"
        );
    }
    let mut value = fixture();
    value["source_extent"] = json!([640, 480, 1]);
    assert_eq!(
        json_reason(decode(&value).unwrap_err()),
        "incorrect value type or shape"
    );
    let nested = format!("{}0{}", "[".repeat(150), "]".repeat(150));
    let raw = serde_json::to_string(&fixture()).unwrap().replace(
        "\"source_pts\":[-3003,0,3004]",
        &format!("\"source_pts\":{nested}"),
    );
    assert!(from_slice(raw.as_bytes()).is_err());
    assert_eq!(
        json_reason(from_slice(b"{").unwrap_err()),
        "incomplete input"
    );
}
#[test]
fn limits_apply_during_sequence_and_utf8_identifier_decoding() {
    assert_eq!(
        serde_json::from_str::<List<u8, 2>>("[1,2]").unwrap().0,
        [1, 2]
    );
    assert!(serde_json::from_str::<List<u8, 2>>("[1,2,3]").is_err());
    assert!(serde_json::from_str::<List<u8, 0>>("[]").is_ok());
    assert!(serde_json::from_str::<List<u8, 0>>("[0]").is_err());
    assert_eq!(
        serde_json::from_str::<Text<3>>(r#""\u4e2d""#).unwrap().0,
        "中"
    );
    assert!(serde_json::from_str::<Text<2>>(r#""\u4e2d""#).is_err());
    let mut value = fixture();
    value["source_id"] = json!("x".repeat(129));
    assert_eq!(
        json_reason(decode(&value).unwrap_err()),
        "identifier length exceeded"
    );
}
#[test]
fn byte_limit_bounds_reader_consumption_and_preserves_io_cause() {
    struct Unlimited {
        consumed: usize,
    }
    impl Read for Unlimited {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            out.fill(b' ');
            self.consumed += out.len();
            Ok(out.len())
        }
    }
    let mut reader = Unlimited { consumed: 0 };
    assert!(matches!(from_reader(&mut reader), Err(Error::InputLimit)));
    assert_eq!(reader.consumed, MAX_JSON_BYTES + 1);
    assert!(matches!(
        from_slice(&vec![b' '; MAX_JSON_BYTES + 1]),
        Err(Error::InputLimit)
    ));
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "input denied",
            ))
        }
    }
    let error = from_reader(Broken).unwrap_err();
    let source = error
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
}
#[test]
fn invalid_hash_and_calibration_return_safe_structured_context() {
    for hash in ["x".repeat(64), "00".repeat(31)] {
        let mut value = fixture();
        value["source_sha256"] = json!(hash);
        assert!(matches!(decode(&value), Err(Error::Field("source_sha256"))));
    }
    let mut value = fixture();
    camera(&mut value);
    value["tracks"][0]["samples"][0]["observation"]["calibration"]["focal"] = json!([0, 400]);
    let error = decode(&value).unwrap_err();
    assert!(matches!(
        error,
        Error::Geometry {
            track: Some(0),
            sample: Some(0),
            ..
        }
    ));
    assert!(error.source().is_some());
}
#[test]
fn semantic_validation_keeps_precise_track_and_sample_locations() {
    let mut value = fixture();
    value["version"] = json!(VERSION + 1);
    assert!(matches!(
        decode(&value),
        Err(Error::Asset(super::super::asset::Error {
            cause: Cause::Version,
            ..
        }))
    ));
    let mut value = fixture();
    value["source_pts"] = json!([0, 0, 3004]);
    assert!(matches!(
        decode(&value),
        Err(Error::Asset(super::super::asset::Error {
            cause: Cause::TimestampOrder,
            ..
        }))
    ));
    let mut value = fixture();
    value["tracks"][0]["samples"][0]["confidence"] = json!(2.0);
    assert!(matches!(
        decode(&value),
        Err(Error::Asset(super::super::asset::Error {
            cause: Cause::Confidence,
            track: Some(0),
            sample: Some(0)
        }))
    ));
}
#[test]
fn parser_errors_never_retain_unknown_keys_values_or_unbounded_user_text() {
    let secret = "credential-private-".repeat(200);
    let mut value = fixture();
    value
        .as_object_mut()
        .unwrap()
        .insert(secret.clone(), json!(secret));
    let error = decode(&value).unwrap_err();
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains("credential-private"));
    assert!(rendered.len() < 256);
    assert!(error.source().is_none());
    let mut value = fixture();
    value["coordinates"] = json!(secret);
    let error = decode(&value).unwrap_err();
    assert!(!format!("{error} {error:?}").contains("credential-private"));
}
