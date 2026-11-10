use motion_model::depth::*;

fn resource(inverse: bool, revision: u64) -> DepthResource {
    DepthResource::new(
        DepthDescriptor {
            size: [2, 1],
            source: DepthSource {
                asset: 75,
                revision,
                pts: DepthTime {
                    ticks: 0,
                    timescale: 24,
                },
            },
            geometry: DepthGeometry {
                source_size: [2, 1],
                crop: [0, 0, 2, 1],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "authored-physical-control".into(),
                revision: "r1".into(),
                sha256: [75; 32],
                processing: "explicit-camera-z".into(),
            },
            measure: if inverse {
                DepthMeasure::InverseMetric {
                    inverse_meters_per_unit: 0.25,
                    axis: DepthAxis::CameraZ,
                }
            } else {
                DepthMeasure::Metric {
                    meters_per_unit: 0.25,
                    axis: DepthAxis::CameraZ,
                }
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        },
        if inverse {
            vec![0.5, 1.]
        } else {
            vec![32., 16.]
        },
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

#[test]
fn physical_placement_preserves_units_and_exact_authoring_identity_across_revision_rebinds() {
    for inverse in [false, true] {
        let original = resource(inverse, 1);
        let saved = SavedDepthCalibration::physical(&original, input(), 20.).unwrap();
        assert!(saved.is_physical());
        let wire = serde_json::to_value(&saved).unwrap();
        for key in ["anchors", "curve", "extrapolation"] {
            assert!(wire.get(key).is_none(), "{key}");
        }
        let reopened: SavedDepthCalibration = serde_json::from_value(wire).unwrap();
        assert_eq!(reopened, saved);
        let current = resource(inverse, 2);
        reopened.validate_binding(&current, input()).unwrap();
        assert!(
            reopened.rebind(&current, input()).is_err(),
            "physical samples never become relative calibration"
        );
        assert_eq!(current.camera_z_meters(0, None).unwrap(), Some(8.));
        assert_eq!(current.camera_z_meters(1, None).unwrap(), Some(4.));
        let mut changed = input();
        changed.rgb_sha256[0] ^= 1;
        assert!(reopened.validate_binding(&current, changed).is_err());
        for mutation in 0..10 {
            let mut d = current.descriptor().clone();
            match mutation {
                0 => d.source.asset += 1,
                1 => d.source.pts.ticks += 1,
                2 => d.geometry.mirror_x = true,
                3 => d.model.sha256[0] ^= 1,
                4 => d.model.processing.push_str("-changed"),
                5 => d.scale = 2.,
                6 => d.offset = 0.5,
                7 => d.valid_range = Some([0., 100.]),
                8 => d.size = [1, 2],
                _ => {
                    d.measure = DepthMeasure::Metric {
                        meters_per_unit: 1.,
                        axis: DepthAxis::CameraZ,
                    }
                }
            }
            assert!(
                reopened.validate_binding_descriptor(&d, input()).is_err(),
                "inverse={inverse} mutation={mutation}"
            );
        }
    }
}

#[test]
fn physical_placement_rejects_relative_parameters_ray_distances_and_incomplete_relative_wire() {
    let source = resource(false, 1);
    let saved = SavedDepthCalibration::physical(&source, input(), 20.).unwrap();
    for key in ["anchors", "curve", "extrapolation"] {
        let mut value = serde_json::to_value(&saved).unwrap();
        value[key] = match key {
            "anchors" => {
                serde_json::json!([{"value":0., "camera_z_meters":8.}, {"value":1., "camera_z_meters":4.}])
            }
            "curve" => serde_json::json!("camera_z"),
            _ => serde_json::json!("reject"),
        };
        assert!(
            serde_json::from_value::<SavedDepthCalibration>(value).is_err(),
            "{key}"
        );
    }
    for mutation in 0..5 {
        let mut value = serde_json::to_value(&saved).unwrap();
        match mutation {
            0 => {
                value["interpretation"]["measure"] =
                    serde_json::json!({"kind":"relative", "near":"increasing"})
            }
            1 => value["interpretation"]["measure"]["axis"] = serde_json::json!("ray_distance"),
            2 => value["pixels_per_metre"] = serde_json::json!(0.),
            3 => value["unexpected"] = serde_json::json!(true),
            _ => value["interpretation"]["measure"]["meters_per_unit"] = serde_json::json!(0.),
        }
        assert!(
            serde_json::from_value::<SavedDepthCalibration>(value).is_err(),
            "mutation={mutation}"
        );
    }
    let mut d = source.descriptor().clone();
    d.measure = DepthMeasure::Relative {
        near: NearDirection::Increasing,
    };
    let relative = DepthResource::new(d, vec![1., 2.], None, None).unwrap();
    assert!(SavedDepthCalibration::physical(&relative, input(), 20.).is_err());
    let relative_saved = SavedDepthCalibration::new(
        &relative,
        input(),
        [
            DepthAnchor {
                value: 1.,
                camera_z_meters: 8.,
            },
            DepthAnchor {
                value: 2.,
                camera_z_meters: 4.,
            },
        ],
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
        20.,
    )
    .unwrap();
    for key in ["anchors", "curve", "extrapolation"] {
        let mut value = serde_json::to_value(&relative_saved).unwrap();
        value.as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<SavedDepthCalibration>(value).is_err(),
            "{key}"
        );
    }
}
