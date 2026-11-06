use motion_model::{depth::blend::*, depth::*, Camera};

fn source(asset: u64, frame: i64) -> DepthSource {
    DepthSource {
        asset,
        revision: 1,
        pts: DepthTime {
            ticks: frame,
            timescale: 24,
        },
    }
}
fn model() -> DepthModel {
    DepthModel {
        id: "authored-test-depth-blend".into(),
        revision: "1".into(),
        sha256: [76; 32],
        processing: "registered-camera-z-linear-v1".into(),
    }
}
fn geometry(size: [u32; 2]) -> DepthGeometry {
    DepthGeometry {
        source_size: size,
        crop: [0, 0, size[0], size[1]],
        quarter_turns: 0,
        mirror_x: false,
    }
}
fn field(
    size: [u32; 2],
    frame: DepthSource,
    values: Vec<f32>,
    validity: Option<Vec<u8>>,
) -> DepthResource {
    DepthResource::new(
        DepthDescriptor {
            size,
            source: frame,
            geometry: geometry(size),
            model: model(),
            measure: DepthMeasure::Metric {
                meters_per_unit: 1.,
                axis: DepthAxis::CameraZ,
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        },
        values,
        validity,
        None,
    )
    .unwrap()
}
fn camera() -> DepthProjection {
    DepthProjection::new(Camera::new(4, 2).pose(0., 4, 2).unwrap(), 1.).unwrap()
}
fn input<'a>(depth: &'a DepthResource, camera: &'a DepthProjection) -> DepthBlendInput<'a> {
    DepthBlendInput::new(depth, None, camera, None).unwrap()
}
fn options(weight: f64) -> DepthBlendOptions {
    DepthBlendOptions {
        weight,
        output_model: model(),
        byte_budget: 1024,
    }
}
fn identity(a: &DepthResource, b: &DepthResource) -> DepthRegistration {
    DepthRegistration::same_sample(
        DepthImageFrame::from_depth(a),
        DepthImageFrame::from_depth(b),
    )
    .unwrap()
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn endpoints_ignore_unused_invalid_depth_and_do_not_sample_the_mask_at_zero_weight() {
    let camera = camera();
    let missing = field([2, 1], source(1, 0), vec![f32::NAN; 2], Some(vec![0; 2]));
    let present = field([2, 1], source(1, 0), vec![2., 4.], None);
    let mask = DepthWeightMask::new(
        source(1, 0),
        [2, 1],
        geometry([2, 1]),
        vec![f32::NAN; 2],
        Some(vec![0; 2]),
    )
    .unwrap();
    let registration = identity(&missing, &present);
    let result = blend_depth(
        input(&missing, &camera),
        input(&present, &camera),
        &registration,
        Some((&mask, &registration)),
        options(0.),
    )
    .unwrap();
    assert_eq!(result.resource.values(), &[2., 4.]);
    assert_eq!(result.resource.validity().unwrap(), &[1, 1]);
    assert_eq!(
        (
            result.metrics.ai_reads,
            result.metrics.reference_reads,
            result.metrics.mask_reads
        ),
        (0, 2, 0)
    );
    let result = blend_depth(
        input(&present, &camera),
        input(&missing, &camera),
        &registration,
        None,
        options(1.),
    )
    .unwrap();
    assert_eq!(result.resource.values(), &[2., 4.]);
    assert_eq!(
        (result.metrics.ai_reads, result.metrics.reference_reads),
        (2, 0)
    );
    let result = blend_depth(
        input(&present, &camera),
        input(&missing, &camera),
        &registration,
        None,
        options(0.5),
    )
    .unwrap();
    assert_eq!(result.resource.validity().unwrap(), &[0, 0]);
    assert_eq!(result.metrics.invalid_pixels, 2);
}

#[test]
fn black_white_gray_and_invalid_mask_samples_are_linear_scalars_without_alpha_fallback() {
    let camera = camera();
    let a = field([4, 1], source(1, 0), vec![2., 4., 6., 8.], None);
    let b = field([4, 1], source(1, 0), vec![10.; 4], None);
    let registration = identity(&a, &b);
    let mask = DepthWeightMask::new(
        source(1, 0),
        [4, 1],
        geometry([4, 1]),
        vec![0., 1., 0.5, f32::NAN],
        Some(vec![1, 1, 1, 0]),
    )
    .unwrap();
    let result = blend_depth(
        input(&a, &camera),
        input(&b, &camera),
        &registration,
        Some((&mask, &registration)),
        options(0.5),
    )
    .unwrap();
    assert_eq!(result.resource.values(), &[10., 7., 9., 0.]);
    assert_eq!(result.resource.validity().unwrap(), &[1, 1, 1, 0]);
    assert_eq!(result.resource.confidence(), None);
    assert_eq!(
        (
            result.metrics.ai_reads,
            result.metrics.reference_reads,
            result.metrics.mask_reads
        ),
        (2, 3, 4)
    );
    assert_eq!(
        (
            result.metrics.output_pixels,
            result.metrics.output_bytes,
            result.metrics.invalid_pixels
        ),
        (4, 20, 1)
    );
    assert_eq!(result.provenance.mask_size, Some([4, 1]));
}

#[test]
fn relative_calibration_and_millimetres_are_converted_before_interpolation() {
    let camera = camera();
    let raw = field([2, 1], source(1, 0), vec![0.25, 0.75], None);
    let mut descriptor = raw.descriptor().clone();
    descriptor.measure = DepthMeasure::Relative {
        near: NearDirection::Increasing,
    };
    descriptor.scale = 2.;
    descriptor.offset = -0.5;
    let a = DepthResource::new(
        descriptor.clone(),
        vec![0.25, 0.75],
        None,
        Some(vec![0., 1.]),
    )
    .unwrap();
    assert!(DepthBlendInput::new(&a, None, &camera, None)
        .err()
        .unwrap()
        .to_string()
        .contains("relative calibration"));
    let calibration = RelativeDepthCalibration::new(
        descriptor,
        [
            DepthAnchor {
                value: 0.,
                camera_z_meters: 8.,
            },
            DepthAnchor {
                value: 1.,
                camera_z_meters: 4.,
            },
        ],
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
    )
    .unwrap();
    let mut descriptor = raw.descriptor().clone();
    descriptor.measure = DepthMeasure::Metric {
        meters_per_unit: 0.001,
        axis: DepthAxis::CameraZ,
    };
    let b = DepthResource::new(descriptor, vec![2000., 4000.], None, None).unwrap();
    let result = blend_depth(
        DepthBlendInput::new(&a, Some(&calibration), &camera, None).unwrap(),
        input(&b, &camera),
        &identity(&a, &b),
        None,
        options(0.5),
    )
    .unwrap();
    assert_eq!(result.resource.values(), &[5., 4.]);
    assert_eq!(
        result.resource.descriptor().measure,
        DepthMeasure::Metric {
            meters_per_unit: 1.,
            axis: DepthAxis::CameraZ
        }
    );
    assert_eq!(result.resource.descriptor().scale, 1.);
    assert_eq!(result.resource.descriptor().offset, 0.);
    assert_eq!(result.provenance.ai_calibration.unwrap(), calibration);
}

#[test]
fn inverse_ray_distance_requires_explicit_grid_cosines_and_uses_camera_z_metres() {
    let camera = camera();
    let raw = field([2, 1], source(1, 0), vec![1.; 2], None);
    let mut descriptor = raw.descriptor().clone();
    descriptor.measure = DepthMeasure::InverseMetric {
        inverse_meters_per_unit: 1.,
        axis: DepthAxis::RayDistance,
    };
    let a = DepthResource::new(descriptor, vec![0.25, 0.5], None, None).unwrap();
    assert!(DepthBlendInput::new(&a, None, &camera, None).is_err());
    assert!(DepthBlendInput::new(&a, None, &camera, Some(&[0.5])).is_err());
    assert!(DepthBlendInput::new(&a, None, &camera, Some(&[0., 0.5])).is_err());
    let b = field([2, 1], source(1, 0), vec![4., 3.], None);
    let result = blend_depth(
        DepthBlendInput::new(&a, None, &camera, Some(&[0.5, 0.5])).unwrap(),
        input(&b, &camera),
        &identity(&a, &b),
        None,
        options(0.5),
    )
    .unwrap();
    assert_eq!(result.resource.values(), &[3., 2.]);
    assert_eq!(result.metrics.ray_cosine_reads, 2);
}

#[test]
fn differing_extent_crop_rotation_mirror_and_grid_use_the_explicit_pixel_registration() {
    let camera = camera();
    let a = field([4, 2], source(1, 1), vec![1.; 8], None);
    let mut descriptor = a.descriptor().clone();
    descriptor.source = source(2, 0);
    descriptor.size = [2, 2];
    descriptor.geometry = DepthGeometry {
        source_size: [8, 4],
        crop: [2, 0, 4, 4],
        quarter_turns: 1,
        mirror_x: true,
    };
    let b = DepthResource::new(descriptor, vec![10., 20., 30., 40.], None, None).unwrap();
    assert!(DepthRegistration::same_sample(
        DepthImageFrame::from_depth(&a),
        DepthImageFrame::from_depth(&b)
    )
    .is_err());
    let registration = DepthRegistration::new(
        DepthImageFrame::from_depth(&a),
        DepthImageFrame::from_depth(&b),
        [[2., 0., 0.], [0., 2., 0.], [0., 0., 1.]],
    )
    .unwrap();
    let result = blend_depth(
        input(&a, &camera),
        input(&b, &camera),
        &registration,
        None,
        options(0.),
    )
    .unwrap();
    assert_eq!(
        result.resource.values(),
        &[0., 10., 30., 0., 0., 20., 40., 0.]
    );
    assert_eq!(
        result.resource.validity().unwrap(),
        &[0, 1, 1, 0, 0, 1, 1, 0]
    );
    assert_eq!(
        (
            result.metrics.ai_reads,
            result.metrics.reference_reads,
            result.metrics.invalid_pixels
        ),
        (0, 4, 4)
    );
}

#[test]
fn stale_target_reference_and_mask_metadata_are_rejected_before_sampling() {
    let camera = camera();
    let a = field([2, 1], source(1, 0), vec![2.; 2], None);
    let b = field([2, 1], source(1, 0), vec![4.; 2], None);
    let registration = identity(&a, &b);
    for (asset, revision, pts) in [(1, 1, 1), (1, 2, 0), (2, 1, 0)] {
        let mut changed = source(asset, pts);
        changed.revision = revision;
        let stale = field([2, 1], changed.clone(), vec![3.; 2], None);
        let error = blend_depth(
            input(&stale, &camera),
            input(&b, &camera),
            &registration,
            None,
            options(0.5),
        )
        .err()
        .unwrap();
        assert!(error
            .to_string()
            .contains("source, revision, PTS or extent"));
        let error = blend_depth(
            input(&a, &camera),
            input(&stale, &camera),
            &registration,
            None,
            options(0.5),
        )
        .err()
        .unwrap();
        assert!(error
            .to_string()
            .contains("source, revision, PTS or extent"));
        let mask =
            DepthWeightMask::new(changed, [2, 1], geometry([2, 1]), vec![1.; 2], None).unwrap();
        assert!(blend_depth(
            input(&a, &camera),
            input(&b, &camera),
            &registration,
            Some((&mask, &registration)),
            options(0.5)
        )
        .is_err());
    }
    let mut equivalent = source(1, 0);
    equivalent.pts.timescale = 48;
    let equivalent = field([2, 1], equivalent, vec![6.; 2], None);
    assert!(blend_depth(
        input(&equivalent, &camera),
        input(&b, &camera),
        &registration,
        None,
        options(0.5)
    )
    .is_ok());
}

#[test]
fn invalid_registration_weights_mask_and_budget_keep_explicit_errors() {
    let camera = camera();
    let a = field([2, 1], source(1, 0), vec![2.; 2], None);
    let b = field([2, 1], source(1, 0), vec![4.; 2], None);
    let registration = identity(&a, &b);
    for matrix in [
        [[0.; 3]; 3],
        [[1., 0., 0.], [0., 0., 0.], [0., 0., 1.]],
        [[1., 0., 0.], [0., 1., 0.], [1., 0., -1.]],
        [[f64::NAN, 0., 0.], [0., 1., 0.], [0., 0., 1.]],
    ] {
        assert!(DepthRegistration::new(
            DepthImageFrame::from_depth(&a),
            DepthImageFrame::from_depth(&b),
            matrix
        )
        .is_err());
    }
    for weight in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        assert!(blend_depth(
            input(&a, &camera),
            input(&b, &camera),
            &registration,
            None,
            options(weight)
        )
        .is_err());
    }
    let mut low_budget = options(0.5);
    low_budget.byte_budget = 9;
    assert!(blend_depth(
        input(&a, &camera),
        input(&b, &camera),
        &registration,
        None,
        low_budget
    )
    .err()
    .unwrap()
    .to_string()
    .contains("output byte budget"));
    for values in [vec![f32::NAN, 0.], vec![-0.01, 1.], vec![0., 1.01]] {
        assert!(
            DepthWeightMask::new(source(1, 0), [2, 1], geometry([2, 1]), values, None).is_err()
        );
    }
    assert_eq!(a.values(), &[2., 2.]);
    assert_eq!(b.values(), &[4., 4.]);
}

#[test]
fn different_cameras_world_units_and_unrepresentable_float_depth_are_not_silently_mixed() {
    let camera = camera();
    let a = field([2, 1], source(1, 0), vec![2.; 2], None);
    let b = field([2, 1], source(1, 0), vec![4.; 2], None);
    let registration = identity(&a, &b);
    let other_units = DepthProjection::new(camera.camera(), 2.).unwrap();
    let mut other_camera = Camera::new(4, 2);
    other_camera.position.value[0] += 1.;
    let other_camera = DepthProjection::new(other_camera.pose(0., 4, 2).unwrap(), 1.).unwrap();
    for other in [other_units, other_camera] {
        assert!(blend_depth(
            input(&a, &camera),
            input(&b, &other),
            &registration,
            None,
            options(0.5)
        )
        .err()
        .unwrap()
        .to_string()
        .contains("common camera"));
    }
    for unit in [1e100, 1e-100] {
        let mut descriptor = a.descriptor().clone();
        descriptor.measure = DepthMeasure::Metric {
            meters_per_unit: unit,
            axis: DepthAxis::CameraZ,
        };
        let excessive = DepthResource::new(descriptor, vec![1.; 2], None, None).unwrap();
        assert!(blend_depth(
            input(&excessive, &camera),
            input(&b, &camera),
            &registration,
            None,
            options(1.)
        )
        .err()
        .unwrap()
        .to_string()
        .contains("pixel=0: metres not representable"));
    }
}

#[test]
fn moving_subject_requires_per_frame_registration_and_exposes_static_reference_ghosting() {
    let camera = camera();
    let reference = field(
        [6, 1],
        source(1, 0),
        vec![10., 10., 2., 10., 10., 10.],
        None,
    );
    let first_registration = identity(&reference, &reference);
    for (frame, noisy_depth, expected_depth) in [(0, 2.2, 2.1), (1, 1.8, 1.9), (2, 2.1, 2.05)] {
        let mut values = vec![10.; 6];
        values[2 + frame] = noisy_depth;
        let current = field([6, 1], source(1, frame as i64), values, None);
        let registration = DepthRegistration::new(
            DepthImageFrame::from_depth(&current),
            DepthImageFrame::from_depth(&reference),
            [[1., 0., -(frame as f64)], [0., 1., 0.], [0., 0., 1.]],
        )
        .unwrap();
        let result = blend_depth(
            input(&current, &camera),
            input(&reference, &camera),
            &registration,
            None,
            options(0.5),
        )
        .unwrap();
        close(
            f64::from(result.resource.values()[2 + frame]),
            expected_depth,
        );
        assert_eq!(
            (
                result.metrics.ai_reads,
                result.metrics.reference_reads,
                result.metrics.invalid_pixels
            ),
            (6, 6 - frame as u64, frame as u64)
        );
        if frame != 0 {
            close(f64::from(result.resource.values()[2]), 10.);
            assert!(blend_depth(
                input(&current, &camera),
                input(&reference, &camera),
                &first_registration,
                None,
                options(0.5)
            )
            .is_err());
            // Deliberately wrong, explicitly authored identity alignment: the
            // original subject location becomes a 6m ghost rather than 10m.
            let wrong = DepthRegistration::new(
                DepthImageFrame::from_depth(&current),
                DepthImageFrame::from_depth(&reference),
                [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            )
            .unwrap();
            let wrong = blend_depth(
                input(&current, &camera),
                input(&reference, &camera),
                &wrong,
                None,
                options(0.5),
            )
            .unwrap();
            close(f64::from(wrong.resource.values()[2]), 6.);
        }
    }
}
