use super::*;
fn sample(index: u32) -> Sample {
    Sample {
        source_index: index,
        visibility: Visibility::Visible,
        confidence: 0.75,
        observation: Some(Observation::Point([12.25, -3.5])),
        manual_correction: false,
    }
}
fn asset() -> Asset {
    Asset {
        version: VERSION,
        source_id: "source-1".into(),
        source_sha256: [7; 32],
        source_extent: Extent::new(640, 480).unwrap(),
        coordinates: Coordinates::RightDownForward,
        ticks_per_second: 90_000,
        nominal_rate: Rate::new(30_000, 1001).unwrap(),
        source_pts: vec![-3003, 0, 3004, 9009],
        meters_per_world_unit: None,
        tracks: vec![Track {
            id: "point-1".into(),
            kind: Kind::Point,
            reference_index: None,
            samples: vec![sample(0), sample(2), sample(3)],
        }],
    }
}
fn cause(asset: &Asset) -> Cause {
    asset.validate().unwrap_err().cause
}
fn camera(a: &mut Asset) {
    a.meters_per_world_unit = Some(0.01);
    a.tracks[0].kind = Kind::Camera;
    let calibration = Calibration::new(
        a.source_extent,
        [400., 400.],
        [320., 240.],
        [0.; 3],
        [0.; 2],
    )
    .unwrap();
    for sample in &mut a.tracks[0].samples {
        sample.observation = Some(Observation::Camera {
            camera_to_world: Pose {
                position: [0., 0., -5.],
                orientation_xyzw: [0., 0., 0., 1.],
            },
            calibration,
        });
    }
}
fn plane(a: &mut Asset, matrix: [[f64; 3]; 3]) {
    a.tracks[0].kind = Kind::Plane;
    a.tracks[0].reference_index = Some(0);
    for sample in &mut a.tracks[0].samples {
        sample.observation = Some(Observation::Plane(matrix));
    }
}
#[test]
fn source_pts_preserve_negative_origin_vfr_gaps_and_exact_fraction() {
    let a = asset();
    assert!(a.validate().is_ok());
    assert_eq!(a.source_time(0), Some((-3003, 90_000)));
    assert_eq!(a.source_time(2), Some((3004, 90_000)));
    assert_eq!(a.source_time(4), None);
    assert_eq!(a.source_sha256, [7; 32]);
    assert_eq!(a.nominal_rate.ratio(), [30_000, 1001]);
}
#[test]
fn nominal_rate_is_bounded_reduced_and_never_used_to_regenerate_pts() {
    assert_eq!(Rate::new(60_000, 2002).unwrap().ratio(), [30_000, 1001]);
    for pair in [[0, 1], [1, 0], [1_000_001, 1]] {
        assert!(Rate::new(pair[0], pair[1]).is_err());
    }
    let mut a = asset();
    a.nominal_rate = Rate::new(24, 1).unwrap();
    assert!(a.validate().is_ok());
    assert_eq!(a.source_time(2), Some((3004, 90_000)));
}
#[test]
fn unknown_versions_and_missing_timebases_are_not_defaulted() {
    let mut a = asset();
    a.version += 1;
    assert_eq!(cause(&a), Cause::Version);
    a.version = VERSION;
    a.ticks_per_second = 0;
    assert_eq!(cause(&a), Cause::Timebase);
    assert_eq!(a.source_time(0), None);
    a.ticks_per_second = 1_000_000_001;
    assert_eq!(cause(&a), Cause::Timebase);
}
#[test]
fn duplicate_or_reversed_source_pts_are_not_silently_sorted() {
    let mut a = asset();
    a.source_pts[1] = a.source_pts[0];
    assert_eq!(cause(&a), Cause::TimestampOrder);
    assert_eq!(a.source_pts[1], -3003);
    a.source_pts[1] = -4000;
    assert_eq!(cause(&a), Cause::TimestampOrder);
}
#[test]
fn track_samples_keep_sparse_time_indices_and_precise_error_context() {
    let mut a = asset();
    a.tracks[0].samples[1].source_index = 9;
    assert_eq!(
        a.validate(),
        Err(Error {
            cause: Cause::SourceIndex,
            track: Some(0),
            sample: Some(1)
        })
    );
    a.tracks[0].samples[1].source_index = 0;
    assert_eq!(cause(&a), Cause::SampleOrder);
}
#[test]
fn occlusion_failure_and_manual_correction_are_preserved_without_interpolation() {
    let mut a = asset();
    a.tracks[0].samples[0].visibility = Visibility::Occluded;
    a.tracks[0].samples[0].observation = None;
    a.tracks[0].samples[1].visibility = Visibility::Lost;
    a.tracks[0].samples[1].observation = None;
    a.tracks[0].samples[2].manual_correction = true;
    assert!(a.validate().is_ok());
    assert!(a.tracks[0].samples[2].manual_correction);
    assert_eq!(a.tracks[0].samples[1].confidence, 0.75);
    a.tracks[0].samples[0].visibility = Visibility::Visible;
    assert_eq!(cause(&a), Cause::MissingObservation);
}
#[test]
fn invalid_confidence_or_coordinates_keep_distinct_causes() {
    let mut a = asset();
    for confidence in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        a.tracks[0].samples[0].confidence = confidence;
        assert_eq!(cause(&a), Cause::Confidence);
    }
    a.tracks[0].samples[0].confidence = 0.5;
    a.tracks[0].samples[0].observation = Some(Observation::Point([f64::NAN, 0.]));
    assert_eq!(cause(&a), Cause::NonFinite);
}
#[test]
fn identifiers_are_unique_bounded_and_do_not_enter_error_messages() {
    let mut a = asset();
    a.tracks.push(a.tracks[0].clone());
    assert_eq!(cause(&a), Cause::DuplicateIdentifier);
    assert!(!a.validate().unwrap_err().to_string().contains("point-1"));
    a.tracks.pop();
    a.tracks[0].id = "x".repeat(129);
    assert_eq!(cause(&a), Cause::Identifier);
    a.tracks[0].id = "bad\nname".into();
    assert_eq!(cause(&a), Cause::Identifier);
}
#[test]
fn empty_assets_and_excessive_tracks_are_refused() {
    let mut a = asset();
    a.source_pts.clear();
    assert_eq!(cause(&a), Cause::Count);
    let mut a = asset();
    a.tracks = vec![a.tracks[0].clone(); MAX_TRACKS + 1];
    assert_eq!(cause(&a), Cause::Count);
    let mut a = asset();
    a.tracks[0].samples.clear();
    assert_eq!(cause(&a), Cause::Count);
}
#[test]
fn camera_pose_requires_declared_units_source_calibration_and_unit_quaternion() {
    let mut a = asset();
    camera(&mut a);
    assert!(a.validate().is_ok());
    a.meters_per_world_unit = None;
    assert_eq!(cause(&a), Cause::WorldUnits);
    a.meters_per_world_unit = Some(0.01);
    if let Some(Observation::Camera {
        camera_to_world, ..
    }) = &mut a.tracks[0].samples[0].observation
    {
        camera_to_world.orientation_xyzw = [0.; 4];
    }
    assert_eq!(cause(&a), Cause::Quaternion);
    camera(&mut a);
    if let Some(Observation::Camera { calibration, .. }) = &mut a.tracks[0].samples[0].observation {
        *calibration = Calibration::new(
            Extent::new(1, 1).unwrap(),
            [1.; 2],
            [0.; 2],
            [0.; 3],
            [0.; 2],
        )
        .unwrap();
    }
    assert_eq!(cause(&a), Cause::CalibrationExtent);
}
#[test]
fn plane_condition_is_scale_invariant_and_reference_identity_is_required() {
    let mut a = asset();
    for scale in [1e-200, 1., 1e200] {
        plane(&mut a, [[scale, 0., 0.], [0., scale, 0.], [0., 0., scale]]);
        assert!(a.validate().is_ok());
    }
    a.tracks[0].reference_index = None;
    assert_eq!(cause(&a), Cause::PlaneReference);
    a.tracks[0].reference_index = Some(99);
    assert_eq!(cause(&a), Cause::PlaneReference);
    plane(&mut a, [[1., 0., 0.], [1., 0., 0.], [0., 0., 1.]]);
    assert_eq!(cause(&a), Cause::PlaneCondition);
}
#[test]
fn kinds_do_not_coerce_observations_or_ignore_unrelated_reference_fields() {
    let mut a = asset();
    a.tracks[0].kind = Kind::Camera;
    a.meters_per_world_unit = Some(1.);
    assert_eq!(cause(&a), Cause::ObservationKind);
    a.tracks[0].kind = Kind::Point;
    a.tracks[0].reference_index = Some(0);
    assert_eq!(cause(&a), Cause::PlaneReference);
}

#[test]
fn large_valid_pixel_translations_do_not_look_like_singular_homographies() {
    let mut a = asset();
    for shift in [8192., 16384., 32768.] {
        for scale in [1e-200, 1., 1e200, -1.] {
            let matrix = [
                [scale, 0., shift * scale],
                [0., scale, -shift * scale],
                [0., 0., scale],
            ];
            plane(&mut a, matrix);
            assert!(
                a.validate().is_ok(),
                "valid source-pixel translation must remain importable shift={shift} scale={scale}"
            );
            assert!(
                matches!(a.tracks[0].samples[0].observation,Some(Observation::Plane(value)) if value==matrix)
            );
        }
    }
}
#[test]
fn nearly_collapsed_axes_remain_rejected_under_explicit_condition_bound() {
    let mut a = asset();
    for scale in [1e-200, 1., 1e200] {
        plane(
            &mut a,
            [[scale, 0., 0.], [0., scale, 0.], [0., 0., 1e-15 * scale]],
        );
        assert_eq!(cause(&a), Cause::PlaneCondition);
        plane(
            &mut a,
            [
                [scale, scale, 0.],
                [scale, 1.000000000000001 * scale, 0.],
                [0., 0., scale],
            ],
        );
        assert_eq!(cause(&a), Cause::PlaneCondition);
    }
}
#[test]
fn invertible_ordinary_pixel_affine_and_perspective_maps_are_retained() {
    let mut a = asset();
    for matrix in [
        [[0., -1., 16000.], [1., 0., 12000.], [0., 0., 1.]],
        [[1., 0.25, 16000.], [-0.1, 1., 8000.], [0., 0., 1.]],
        [[1., 0., 4000.], [0., 1., 2000.], [0.00001, -0.00002, 1.]],
    ] {
        plane(&mut a, matrix);
        assert!(
            a.validate().is_ok(),
            "finite ordinary plane mapping must remain importable: {matrix:?}"
        );
    }
}
