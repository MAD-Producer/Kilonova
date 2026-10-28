use motion_model::depth::*;

fn descriptor() -> DepthDescriptor {
    DepthDescriptor {
        size: [4, 1],
        source: DepthSource {
            asset: 74,
            revision: 3,
            pts: DepthTime {
                ticks: 5,
                timescale: 24,
            },
        },
        geometry: DepthGeometry {
            source_size: [8, 4],
            crop: [2, 1, 4, 1],
            quarter_turns: 0,
            mirror_x: false,
        },
        model: DepthModel {
            id: "independent-test-field".into(),
            revision: "1".into(),
            sha256: [7; 32],
            processing: "raw-f32-test".into(),
        },
        measure: DepthMeasure::Relative {
            near: NearDirection::Increasing,
        },
        scale: 1.,
        offset: 0.,
        valid_range: None,
    }
}
fn anchors() -> [DepthAnchor; 2] {
    [
        DepthAnchor {
            value: 1.,
            camera_z_meters: 2.,
        },
        DepthAnchor {
            value: 2.,
            camera_z_meters: 1.,
        },
    ]
}
fn calibrated(
    curve: CalibrationCurve,
    policy: CalibrationExtrapolation,
) -> RelativeDepthCalibration {
    RelativeDepthCalibration::new(descriptor(), anchors(), curve, policy).unwrap()
}

#[test]
fn explicit_linear_and_inverse_calibration_have_independent_numeric_references() {
    let source = DepthResource::new(descriptor(), vec![1., 1.25, 1.5, 2.], None, None).unwrap();
    let before = source.values().to_vec();
    let linear = calibrated(CalibrationCurve::CameraZ, CalibrationExtrapolation::Reject);
    let inverse = calibrated(
        CalibrationCurve::InverseCameraZ,
        CalibrationExtrapolation::Reject,
    );
    let l = linear.bind(&source).unwrap();
    let i = inverse.bind(&source).unwrap();
    for (n, expected) in [2., 1.75, 1.5, 1.].into_iter().enumerate() {
        assert_eq!(l.camera_z_meters(n).unwrap(), Some(expected));
    }
    // Inverse Z = value/2; Z=2/value, independently of the implementation's interpolation.
    for (n, expected) in [2., 1.6, 4. / 3., 1.].into_iter().enumerate() {
        assert!((i.camera_z_meters(n).unwrap().unwrap() - expected).abs() < 1e-14);
    }
    assert_eq!(source.values(), before);
    assert!(source
        .camera_z_meters(0, None)
        .unwrap_err()
        .to_string()
        .contains("explicit calibration"));
    assert_eq!(linear.anchors(), &anchors());
    assert_eq!(linear.curve(), CalibrationCurve::CameraZ);
    assert_eq!(linear.extrapolation(), CalibrationExtrapolation::Reject);
    assert!(std::ptr::eq(l.resource(), &source));
    assert!(std::ptr::eq(l.calibration(), &linear));
}

#[test]
fn calibration_uses_decoded_values_and_keeps_validity_confidence_and_normal_misses() {
    let mut d = descriptor();
    d.scale = 2.;
    d.offset = -1.;
    let source = DepthResource::new(
        d.clone(),
        vec![1., f32::NAN, 1.5, f32::INFINITY],
        Some(vec![1, 0, 1, 0]),
        Some(vec![0.9, 0., 0.2, 0.]),
    )
    .unwrap();
    let c = RelativeDepthCalibration::new(
        d,
        anchors(),
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
    )
    .unwrap();
    let view = c.bind(&source).unwrap();
    assert_eq!(view.camera_z_meters(0).unwrap(), Some(2.));
    assert_eq!(view.camera_z_meters(2).unwrap(), Some(1.));
    for n in [1, 3, 4, usize::MAX] {
        assert_eq!(view.camera_z_meters(n).unwrap(), None);
    }
    assert_eq!(view.resource().validity(), Some(&[1, 0, 1, 0][..]));
    assert_eq!(view.resource().confidence(), Some(&[0.9, 0., 0.2, 0.][..]));
}

#[test]
fn stale_source_time_model_geometry_or_decoded_units_refuse_even_on_an_invalid_pixel() {
    let c = calibrated(CalibrationCurve::CameraZ, CalibrationExtrapolation::Reject);
    let mut variants = Vec::new();
    for field in 0..15 {
        let mut d = descriptor();
        match field {
            0 => d.source.asset += 1,
            1 => d.source.revision += 1,
            2 => d.source.pts.ticks += 1,
            3 => d.model.id.push('x'),
            4 => d.model.revision.push('x'),
            5 => d.model.processing.push('x'),
            6 => d.model.sha256[0] ^= 1,
            7 => d.geometry.mirror_x = true,
            8 => d.geometry.quarter_turns = 1,
            9 => d.geometry.crop[0] = 1,
            10 => d.geometry.source_size[0] = 10,
            11 => d.size = [2, 2],
            12 => d.scale = 2.,
            13 => d.offset = 1.,
            14 => d.valid_range = Some([0., 4.]),
            _ => unreachable!(),
        }
        variants.push(d);
    }
    for d in variants {
        let source = DepthResource::new(d, vec![f32::NAN; 4], Some(vec![0; 4]), None).unwrap();
        assert!(c
            .bind(&source)
            .err()
            .unwrap()
            .to_string()
            .contains("does not match"));
    }
    // Rationally equivalent PTS denotes the same sample and is intentionally accepted.
    let mut d = descriptor();
    d.source.pts = DepthTime {
        ticks: 10,
        timescale: 48,
    };
    let source = DepthResource::new(d, vec![1.; 4], None, None).unwrap();
    assert!(c.bind(&source).is_ok());
}

#[test]
fn anchor_order_and_declared_near_direction_are_preserved() {
    let c = calibrated(
        CalibrationCurve::InverseCameraZ,
        CalibrationExtrapolation::Reject,
    );
    let reversed = RelativeDepthCalibration::new(
        descriptor(),
        [anchors()[1], anchors()[0]],
        CalibrationCurve::InverseCameraZ,
        CalibrationExtrapolation::Reject,
    )
    .unwrap();
    let source = DepthResource::new(descriptor(), vec![1., 1.25, 1.5, 2.], None, None).unwrap();
    for n in 0..4 {
        assert_eq!(
            c.bind(&source).unwrap().camera_z_meters(n).unwrap(),
            reversed.bind(&source).unwrap().camera_z_meters(n).unwrap()
        );
    }
    let mut d = descriptor();
    d.measure = DepthMeasure::Relative {
        near: NearDirection::Decreasing,
    };
    assert!(RelativeDepthCalibration::new(
        d.clone(),
        anchors(),
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject
    )
    .is_err());
    let a = [
        DepthAnchor {
            value: 1.,
            camera_z_meters: 1.,
        },
        DepthAnchor {
            value: 2.,
            camera_z_meters: 2.,
        },
    ];
    assert!(RelativeDepthCalibration::new(
        d,
        a,
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject
    )
    .is_ok());
}

#[test]
fn extrapolation_is_an_explicit_policy_and_never_clamps_or_fabricates_validity() {
    let source = DepthResource::new(descriptor(), vec![0.5, 1., 2., 2.5], None, None).unwrap();
    let reject = calibrated(CalibrationCurve::CameraZ, CalibrationExtrapolation::Reject);
    let view = reject.bind(&source).unwrap();
    for n in [0, 3] {
        assert!(view
            .camera_z_meters(n)
            .unwrap_err()
            .to_string()
            .contains("extrapolation"));
    }
    let allow = calibrated(
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::AllowPositive,
    );
    let view = allow.bind(&source).unwrap();
    assert_eq!(view.camera_z_meters(0).unwrap(), Some(2.5));
    assert_eq!(view.camera_z_meters(3).unwrap(), Some(0.5));
    let bad = DepthResource::new(descriptor(), vec![-1., 0., 3., 4.], None, None).unwrap();
    let view = allow.bind(&bad).unwrap();
    for n in [2, 3] {
        assert!(view.camera_z_meters(n).is_err());
    }
    let inverse = calibrated(
        CalibrationCurve::InverseCameraZ,
        CalibrationExtrapolation::AllowPositive,
    );
    let view = inverse.bind(&bad).unwrap();
    for n in [0, 1] {
        assert!(view.camera_z_meters(n).is_err());
    }
    assert_eq!(bad.validity(), None);
}

#[test]
fn invalid_anchors_ranges_and_metric_relabeling_are_explicit_errors() {
    let try_new = |d, a| {
        RelativeDepthCalibration::new(
            d,
            a,
            CalibrationCurve::InverseCameraZ,
            CalibrationExtrapolation::Reject,
        )
    };
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut a = anchors();
        a[0].value = value;
        assert!(try_new(descriptor(), a).is_err());
    }
    for distance in [0., -1., f64::NAN, f64::INFINITY, f64::from_bits(1)] {
        let mut a = anchors();
        a[1].camera_z_meters = distance;
        assert!(try_new(descriptor(), a).is_err());
    }
    let mut a = anchors();
    a[1].value = a[0].value;
    assert!(try_new(descriptor(), a).is_err());
    let mut a = anchors();
    a[1].camera_z_meters = a[0].camera_z_meters;
    assert!(try_new(descriptor(), a).is_err());
    let mut d = descriptor();
    d.valid_range = Some([1., 1.5]);
    assert!(try_new(d, anchors()).is_err());
    for measure in [
        DepthMeasure::Metric {
            meters_per_unit: 1.,
            axis: DepthAxis::CameraZ,
        },
        DepthMeasure::InverseMetric {
            inverse_meters_per_unit: 1.,
            axis: DepthAxis::RayDistance,
        },
    ] {
        let mut d = descriptor();
        d.measure = measure;
        assert!(try_new(d, anchors()).is_err());
    }
}

#[test]
fn finite_extreme_anchors_keep_exact_endpoints_and_reject_interpolation_overflow() {
    let mut d = descriptor();
    d.scale = 1e300;
    let a = [
        DepthAnchor {
            value: 1e300,
            camera_z_meters: 2e200,
        },
        DepthAnchor {
            value: 2e300,
            camera_z_meters: 1e200,
        },
    ];
    let c = RelativeDepthCalibration::new(
        d.clone(),
        a,
        CalibrationCurve::InverseCameraZ,
        CalibrationExtrapolation::Reject,
    )
    .unwrap();
    let source = DepthResource::new(d, vec![1., 1.5, 2., 3.], None, None).unwrap();
    let view = c.bind(&source).unwrap();
    assert_eq!(view.camera_z_meters(0).unwrap(), Some(2e200));
    assert_eq!(view.camera_z_meters(2).unwrap(), Some(1e200));
    assert!((view.camera_z_meters(1).unwrap().unwrap() / 1e200 - 4. / 3.).abs() < 1e-14);
    assert!(view.camera_z_meters(3).is_err());
    let mut a = anchors();
    a[0].value = -f64::MAX;
    a[1].value = f64::MAX;
    assert!(RelativeDepthCalibration::new(
        descriptor(),
        a,
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::AllowPositive
    )
    .is_err());
}

fn camera(kind: motion_model::ProjectionKind, infinite: bool) -> motion_model::CameraPose {
    use glam::{Mat4, Vec3};
    let eye = Vec3::new(0., 0., 3.);
    let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
    let projection = match kind {
        motion_model::ProjectionKind::Orthographic => {
            Mat4::orthographic_rh(-1., 1., -1., 1., 0.1, 10.)
        }
        motion_model::ProjectionKind::Perspective => {
            if infinite {
                Mat4::perspective_infinite_rh(1., 1., 0.1)
            } else {
                Mat4::perspective_rh(1., 1., 0.1, 10.)
            }
        }
    };
    motion_model::CameraPose {
        eye,
        target: Vec3::ZERO,
        projection: kind,
        view_projection: projection * view,
    }
}

#[test]
fn camera_z_projects_to_independent_orthographic_finite_and_infinite_depth_formulas() {
    use motion_model::ProjectionKind::{Orthographic, Perspective};
    for (kind, infinite) in [
        (Orthographic, false),
        (Perspective, false),
        (Perspective, true),
    ] {
        let projection = DepthProjection::new(camera(kind, infinite), 20.).unwrap();
        // Physical metres × explicit world pixels/metre; clip distances are world pixels.
        for z in [0.2, 2., 9.] {
            let actual = projection
                .project_camera_z([0.2, 0.8], z / 20.)
                .unwrap()
                .unwrap() as f64;
            let expected = if kind == Orthographic {
                (z - 0.1) / 9.9
            } else if infinite {
                1. - 0.1 / z
            } else {
                (10. / 9.9) * (1. - 0.1 / z)
            };
            assert!(
                (actual - expected).abs() < 2e-6,
                "{kind:?}/{infinite}/{z}: {actual} != {expected}"
            );
        }
        assert_eq!(
            projection.project_camera_z([0.5, 0.5], 0.05 / 20.).unwrap(),
            None
        );
        if !infinite {
            assert_eq!(
                projection.project_camera_z([0.5, 0.5], 20. / 20.).unwrap(),
                None
            );
        }
        assert_eq!(projection.project_camera_z([-0.1, 0.5], 1.).unwrap(), None);
    }
}

#[test]
fn projected_fields_keep_crop_rotation_mirror_invalidity_and_exact_camera_identity() {
    use motion_model::ProjectionKind::Orthographic;
    let mut d = descriptor();
    d.size = [2, 2];
    d.geometry = DepthGeometry {
        source_size: [4, 4],
        crop: [1, 1, 2, 2],
        quarter_turns: 1,
        mirror_x: true,
    };
    let source = DepthResource::new(
        d.clone(),
        vec![1., 1.25, 1.5, 2.],
        Some(vec![1, 0, 1, 1]),
        None,
    )
    .unwrap();
    let c = RelativeDepthCalibration::new(
        d,
        anchors(),
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
    )
    .unwrap();
    let p = DepthProjection::new(camera(Orthographic, false), 1.).unwrap();
    let output = ProjectedDepth::from_calibrated(&c.bind(&source).unwrap(), p, [4, 4]).unwrap();
    assert_eq!(output.size(), [4, 4]);
    assert_eq!(output.source(), source.descriptor());
    assert!(output
        .projection()
        .matches_camera(camera(Orthographic, false)));
    let mut other = camera(Orthographic, false);
    other.eye.x += 0.01;
    assert!(!output.projection().matches_camera(other));
    for y in 0..4 {
        for x in 0..4 {
            let actual = output.samples()[y * 4 + x];
            let z = match (x, y) {
                (1, 1) => Some(2.),
                (2, 1) => Some(1.5),
                (2, 2) => Some(1.),
                _ => None,
            };
            if let Some(z) = z {
                assert_eq!(actual[1], 1.);
                assert!((actual[0] as f64 - (z - 0.1) / 9.9).abs() < 2e-6);
            } else {
                assert_eq!(actual, [1., 0.]);
            }
        }
    }
}

#[test]
fn invalid_projection_cameras_units_samples_and_output_budgets_refuse() {
    use motion_model::ProjectionKind::Orthographic;
    for units in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(DepthProjection::new(camera(Orthographic, false), units).is_err());
    }
    let mut bad = camera(Orthographic, false);
    bad.target = bad.eye;
    assert!(DepthProjection::new(bad, 1.).is_err());
    let mut bad = camera(Orthographic, false);
    bad.view_projection = glam::Mat4::ZERO;
    assert!(DepthProjection::new(bad, 1.).is_err());
    let p = DepthProjection::new(camera(Orthographic, false), 1.).unwrap();
    for z in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(p.project_camera_z([0.5, 0.5], z).is_err());
    }
    assert!(p.project_camera_z([f64::NAN, 0.5], 1.).is_err());
    let p = DepthProjection::new(camera(Orthographic, false), f64::MAX).unwrap();
    assert!(p.project_camera_z([0.5, 0.5], 2.).is_err());
    let c = calibrated(CalibrationCurve::CameraZ, CalibrationExtrapolation::Reject);
    let source = DepthResource::new(descriptor(), vec![1.; 4], None, None).unwrap();
    let view = c.bind(&source).unwrap();
    for size in [
        [0, 1],
        [1, 0],
        [MAX_DEPTH_DIMENSION + 1, 1],
        [u32::MAX, u32::MAX],
    ] {
        assert!(ProjectedDepth::from_calibrated(
            &view,
            DepthProjection::new(camera(Orthographic, false), 1.).unwrap(),
            size
        )
        .is_err());
    }
}
