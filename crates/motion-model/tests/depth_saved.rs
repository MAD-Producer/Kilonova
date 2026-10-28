use motion_model::depth::*;

fn resource(revision: u64) -> DepthResource {
    DepthResource::new(
        DepthDescriptor {
            size: [2, 1],
            source: DepthSource {
                asset: 7,
                revision,
                pts: DepthTime {
                    ticks: 0,
                    timescale: 1,
                },
            },
            geometry: DepthGeometry {
                source_size: [2, 1],
                crop: [0, 0, 2, 1],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "owned-test".into(),
                revision: "model-r1".into(),
                sha256: [7; 32],
                processing: "owned-v1".into(),
            },
            measure: DepthMeasure::Relative {
                near: NearDirection::Increasing,
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        },
        vec![0.25, 2.],
        None,
        None,
    )
    .unwrap()
}
fn input() -> PreparedDepthInput {
    PreparedDepthInput {
        rgb_size: [2, 1],
        rgb_sha256: [42; 32],
    }
}
fn anchors() -> [DepthAnchor; 2] {
    [
        DepthAnchor {
            value: 0.25,
            camera_z_meters: 8.,
        },
        DepthAnchor {
            value: 2.,
            camera_z_meters: 4.,
        },
    ]
}

#[test]
fn saved_calibration_rebinds_only_identical_authoring_inputs_without_weakening_live_revision() {
    let original = resource(77);
    let current = resource(145);
    let live = RelativeDepthCalibration::new(
        original.descriptor().clone(),
        anchors(),
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
    )
    .unwrap();
    assert!(live.bind(&current).is_err());
    let saved = SavedDepthCalibration::new(
        &original,
        input(),
        anchors(),
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
        20.,
    )
    .unwrap();
    let value = serde_json::to_value(&saved).unwrap();
    assert!(value.get("revision").is_none());
    assert_eq!(value["interpretation"]["model"]["revision"], "model-r1");
    let reopened: SavedDepthCalibration = serde_json::from_value(value).unwrap();
    assert_eq!(reopened, saved);
    let bound = reopened.rebind(&current, input()).unwrap();
    assert_eq!(bound.descriptor().source.revision, 145);
    assert_eq!(
        bound.bind(&current).unwrap().camera_z_meters(0).unwrap(),
        Some(8.)
    );
    assert_eq!(
        bound.bind(&current).unwrap().camera_z_meters(1).unwrap(),
        Some(4.)
    );
    assert!(bound.bind(&original).is_err());
    let mut changed_input = input();
    changed_input.rgb_sha256[0] ^= 1;
    assert!(saved.rebind(&current, changed_input).is_err());
    for mutation in 0..8 {
        let mut descriptor = current.descriptor().clone();
        match mutation {
            0 => descriptor.source.asset += 1,
            1 => descriptor.source.pts.ticks += 1,
            2 => descriptor.geometry.mirror_x = true,
            3 => descriptor.model.sha256[0] ^= 1,
            4 => descriptor.model.processing = "different-processing".into(),
            5 => descriptor.scale = 2.,
            6 => descriptor.offset = 0.5,
            _ => descriptor.valid_range = Some([0., 4.]),
        }
        let changed = DepthResource::new(descriptor, vec![0.25, 2.], None, None).unwrap();
        assert!(
            saved.rebind(&changed, input()).is_err(),
            "mutation={mutation}"
        );
    }
}

#[test]
fn saved_calibration_deserialization_validates_units_anchors_and_bounded_contracts() {
    let saved = SavedDepthCalibration::new(
        &resource(77),
        input(),
        anchors(),
        CalibrationCurve::InverseCameraZ,
        CalibrationExtrapolation::AllowPositive,
        20.,
    )
    .unwrap();
    for mutation in 0..7 {
        let mut value = serde_json::to_value(&saved).unwrap();
        match mutation {
            0 => value["pixels_per_metre"] = serde_json::json!(0.),
            1 => value["anchors"][0]["camera_z_meters"] = serde_json::json!(-1.),
            2 => value["input"]["rgb_sha256"] = serde_json::json!([1, 2]),
            3 => value["input"]["rgb_size"] = serde_json::json!([5000, 1]),
            4 => value["interpretation"]["model"]["id"] = serde_json::json!("x".repeat(1000)),
            5 => value["unexpected"] = serde_json::json!(1),
            _ => {
                value["interpretation"]["measure"] =
                    serde_json::json!({"kind":"metric","meters_per_unit":1.,"axis":"camera_z"})
            }
        }
        assert!(
            serde_json::from_value::<SavedDepthCalibration>(value).is_err(),
            "mutation={mutation}"
        );
    }
}
