use motion_model::{Ease, SpatialPathBudget, SpatialTangents, Track};

fn track(points: &[[f32; 3]], frames: &[i32]) -> Track<[f32; 3]> {
    let mut result = Track::constant(points[0]);
    for (point, frame) in points.iter().zip(frames) {
        result.upsert(*frame, *point, Ease::Linear).unwrap();
    }
    result
}

fn cubic(points: [[f32; 3]; 4]) -> Track<[f32; 3]> {
    let mut result = track(&[points[0], points[3]], &[0, 10]);
    result
        .set_spatial(
            0,
            Some(SpatialTangents {
                incoming: None,
                outgoing: Some(std::array::from_fn(|i| points[1][i] - points[0][i])),
            }),
        )
        .unwrap();
    result
        .set_spatial(
            10,
            Some(SpatialTangents {
                incoming: Some(std::array::from_fn(|i| points[2][i] - points[3][i])),
                outgoing: None,
            }),
        )
        .unwrap();
    result
}

// Independent composite Simpson integration of the analytic derivative. It
// neither uses the production subdivision/LUT nor chord-length estimates.
fn integral(points: [[f64; 3]; 4], end: f64) -> f64 {
    let speed = |t: f64| {
        (0..3).fold(0f64, |norm, i| {
            norm.hypot(
                3. * (1. - t).powi(2) * (points[1][i] - points[0][i])
                    + 6. * (1. - t) * t * (points[2][i] - points[1][i])
                    + 3. * t * t * (points[3][i] - points[2][i]),
            )
        })
    };
    let steps = 20_000;
    let delta = end / f64::from(steps);
    let mut total = speed(0.) + speed(end);
    for i in 1..steps {
        total += speed(f64::from(i) * delta) * if i % 2 == 0 { 2. } else { 4. };
    }
    total * delta / 3.
}

#[test]
fn constant_speed_times_stay_fractional_and_preserve_author_storage_and_prepared_snapshot() {
    let mut source = track(
        &[[0., 0., 0.], [1., 0., 0.], [3., 0., 0.], [9., 0., 0.]],
        &[0, 3, 6, 10],
    );
    let before = serde_json::to_vec(&source).unwrap();
    let path = source
        .prepare_spatial_path(SpatialPathBudget::default())
        .unwrap();
    assert_eq!(path.total_length(), 9.);
    assert_eq!(
        path.constant_speed_times(0, 3, 0., 10.).unwrap(),
        vec![0., 10. / 9., 10. / 3., 10.]
    );
    assert_eq!(serde_json::to_vec(&source).unwrap(), before);
    source.set_at(3, [2., 0., 0.]).unwrap();
    let edited = source
        .prepare_spatial_path(SpatialPathBudget::default())
        .unwrap();
    assert_eq!(
        edited.constant_speed_times(0, 3, 0., 10.).unwrap(),
        vec![0., 20. / 9., 10. / 3., 10.]
    );
    assert_eq!(
        path.constant_speed_times(0, 3, 0., 10.).unwrap()[1],
        10. / 9.
    );
    assert_eq!(
        source.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        [0, 3, 6, 10]
    );
}

#[test]
fn closed_cubic_uses_arc_length_and_distance_queries_match_independent_integrals() {
    let points = [[0., 0., 0.], [20., 30., 5.], [-15., 25., -5.], [0., 0., 0.]];
    let source = cubic(points);
    let before = source.clone();
    let budget = SpatialPathBudget::default();
    let path = source.prepare_spatial_path(budget).unwrap();
    let reference = points.map(|p| p.map(f64::from));
    let expected = integral(reference, 1.);
    assert!(expected > 30.);
    let tolerance = expected * budget.relative_tolerance;
    assert!((path.total_length() - expected).abs() <= tolerance);
    assert!(path.node_count() > 2 && path.node_count() <= budget.nodes);
    for index in 0..=16 {
        let distance = path.total_length() * f64::from(index) / 16.;
        let sample = path.sample_distance(distance).unwrap();
        assert_eq!(sample.interval, 0);
        assert!(
            (integral(reference, sample.parameter) - distance).abs() <= 4. * tolerance,
            "index={index} distance={distance} parameter={}",
            sample.parameter
        );
        for axis in 0..3 {
            let t = sample.parameter;
            let value = reference[0][axis] * (1. - t).powi(3)
                + 3. * reference[1][axis] * (1. - t).powi(2) * t
                + 3. * reference[2][axis] * (1. - t) * t * t
                + reference[3][axis] * t.powi(3);
            assert!((sample.position[axis] - value).abs() < 0.000000000001);
        }
    }
    assert_eq!(
        path.sample_distance(path.total_length()).unwrap().position,
        [0.; 3]
    );
    assert_eq!(source, before);
}

#[test]
fn collinear_cubic_with_nonlinear_parameterization_is_not_treated_as_linear_parameter() {
    let source = cubic([[0.; 3], [0.; 3], [0.; 3], [1., 0., 0.]]);
    let budget = SpatialPathBudget::default();
    let path = source.prepare_spatial_path(budget).unwrap();
    assert_eq!(path.total_length(), 1.);
    assert!(path.node_count() > 2);
    for i in 0..=32 {
        let distance = f64::from(i) / 32.;
        let sample = path.sample_distance(distance).unwrap();
        assert!((sample.position[0] - distance).abs() <= budget.relative_tolerance);
        assert_eq!(sample.position[1..], [0., 0.]);
    }
    assert!((path.sample_distance(0.125).unwrap().parameter - 0.5).abs() < 0.0001);
}

#[test]
fn repeated_positions_keep_distinct_entries_and_stationary_paths_keep_authored_time_ratios() {
    let source = track(
        &[[0.; 3], [0.; 3], [3., 0., 0.], [9., 0., 0.]],
        &[0, 3, 6, 10],
    );
    let path = source
        .prepare_spatial_path(SpatialPathBudget::default())
        .unwrap();
    assert_eq!(
        path.constant_speed_times(0, 3, 0., 10.).unwrap(),
        [0., 0., 10. / 3., 10.]
    );
    let stationary = track(&[[7.; 3]; 3], &[0, 2, 10]);
    let path = stationary
        .prepare_spatial_path(SpatialPathBudget::default())
        .unwrap();
    assert_eq!(path.total_length(), 0.);
    assert_eq!(
        path.constant_speed_times(0, 2, 100., 200.).unwrap(),
        [100., 120., 200.]
    );
    assert_eq!(path.sample_distance(0.).unwrap().position, [7.; 3]);
    assert!(path.sample_distance(1.).is_err());
}

#[test]
fn local_span_avoids_cancellation_and_tiny_or_large_paths_keep_their_metric() {
    let source = track(
        &[[1e30, 0., 0.], [0.; 3], [1., 0., 0.], [3., 0., 0.]],
        &[0, 10, 20, 30],
    );
    let path = source
        .prepare_spatial_path(SpatialPathBudget::default())
        .unwrap();
    assert_eq!(
        path.constant_speed_times(1, 3, 0., 9.).unwrap(),
        [0., 3., 9.]
    );
    assert_eq!(
        path.sample_distance(path.total_length()).unwrap().position,
        [3., 0., 0.]
    );
    assert_eq!(
        path.sample_interval_distance(2, 1.).unwrap().position,
        [2., 0., 0.]
    );
    for scale in [1e-25, 1e25] {
        let source = track(&[[0.; 3], [0., 0., scale]], &[i32::MIN, i32::MAX]);
        let path = source
            .prepare_spatial_path(SpatialPathBudget::default())
            .unwrap();
        assert_eq!(path.total_length(), f64::from(scale));
        let value = path.sample_distance(path.total_length() * 0.5).unwrap();
        assert!((value.position[2] / f64::from(scale) - 0.5).abs() < 0.00001);
        assert_eq!(
            path.constant_speed_times(0, 1, -0.125, 0.75).unwrap(),
            [-0.125, 0.75]
        );
        let source = track(
            &[[0.; 3], [scale, 0., 0.], [2. * scale, 0., 0.]],
            &[0, 10, 20],
        );
        let path = source
            .prepare_spatial_path(SpatialPathBudget::default())
            .unwrap();
        for end in [1e-300, 1e300] {
            let times = path.constant_speed_times(0, 2, 0., end).unwrap();
            assert!(times[1].is_finite() && times[1] > 0.);
            assert!((times[1] / end - 0.5).abs() < f64::EPSILON * 4.);
        }
    }
}

#[test]
fn resource_limits_invalid_geometry_and_bad_queries_reject_without_changing_authored_values() {
    let source = cubic([[0.; 3], [20., 30., 5.], [-15., 25., -5.], [0.; 3]]);
    let before = serde_json::to_vec(&source).unwrap();
    for budget in [
        SpatialPathBudget {
            nodes: 2,
            ..Default::default()
        },
        SpatialPathBudget {
            subdivisions: 0,
            ..Default::default()
        },
        SpatialPathBudget {
            depth: 0,
            ..Default::default()
        },
        SpatialPathBudget {
            intervals: 0,
            ..Default::default()
        },
        SpatialPathBudget {
            nodes: usize::MAX,
            ..Default::default()
        },
        SpatialPathBudget {
            relative_tolerance: f64::NAN,
            ..Default::default()
        },
        SpatialPathBudget {
            absolute_tolerance: -1.,
            ..Default::default()
        },
    ] {
        assert!(source.prepare_spatial_path(budget).is_err(), "{budget:?}");
        assert_eq!(serde_json::to_vec(&source).unwrap(), before);
    }
    let path = source
        .prepare_spatial_path(SpatialPathBudget::default())
        .unwrap();
    for distance in [f64::NAN, f64::INFINITY, -1., path.total_length() + 1.] {
        assert!(path.sample_distance(distance).is_err());
    }
    for (first, last, start, end) in [
        (0, 0, 0., 1.),
        (0, 2, 0., 1.),
        (0, 1, 1., 0.),
        (0, 1, f64::NAN, 1.),
    ] {
        assert!(path.constant_speed_times(first, last, start, end).is_err());
    }
    assert!(path.interval_length(1).is_err());
    for (interval, distance) in [
        (1, 0.),
        (0, -1.),
        (0, f64::NAN),
        (0, path.total_length() + 1.),
    ] {
        assert!(path.sample_interval_distance(interval, distance).is_err());
    }
    let mut scalar = Track::constant(0f32);
    scalar.upsert(0, 0., Ease::Linear).unwrap();
    scalar.upsert(1, 1., Ease::Linear).unwrap();
    assert!(scalar
        .prepare_spatial_path(SpatialPathBudget::default())
        .is_err());
    let mut color = Track::constant([0f32; 4]);
    color.upsert(0, [0.; 4], Ease::Linear).unwrap();
    color.upsert(1, [1.; 4], Ease::Linear).unwrap();
    assert!(color
        .prepare_spatial_path(SpatialPathBudget::default())
        .is_err());
    let mut separated = track(&[[0.; 3], [1.; 3]], &[0, 10]);
    separated.separate().unwrap();
    assert!(separated
        .prepare_spatial_path(SpatialPathBudget::default())
        .is_err());
    let mut invalid = source.clone();
    invalid.keys[1].frame = 0;
    assert!(invalid
        .prepare_spatial_path(SpatialPathBudget::default())
        .is_err());
    invalid = source.clone();
    invalid.keys[1].value[0] = f32::INFINITY;
    assert!(invalid
        .prepare_spatial_path(SpatialPathBudget::default())
        .is_err());
}

#[test]
fn two_dimensional_tracks_keep_both_coordinates_and_later_fixed_spans() {
    let mut source = Track::constant([0f32; 2]);
    for (frame, value) in [
        (0, [0., 0.]),
        (5, [3., 4.]),
        (10, [3., 4.]),
        (20, [9., 12.]),
    ] {
        source.upsert(frame, value, Ease::Linear).unwrap();
    }
    let path = source
        .prepare_spatial_path(SpatialPathBudget::default())
        .unwrap();
    assert_eq!(path.total_length(), 15.);
    assert_eq!(path.interval_length(1).unwrap(), 0.);
    assert_eq!(
        path.constant_speed_times(1, 3, 1.25, 5.75).unwrap(),
        [1.25, 1.25, 5.75]
    );
    assert_eq!(path.sample_distance(2.5).unwrap().position, [1.5, 2., 0.]);
}

#[test]
fn preparation_limits_apply_to_the_whole_path_instead_of_resetting_per_interval() {
    let source = track(&[[0.; 3], [1.; 3], [2.; 3]], &[0, 10, 20]);
    let before = source.clone();
    let error = source
        .prepare_spatial_path(SpatialPathBudget {
            nodes: 3,
            ..Default::default()
        })
        .unwrap_err();
    assert!(error.to_string().contains("interval=1"));
    assert!(error.to_string().contains("node budget exhausted"));
    let error = source
        .prepare_spatial_path(SpatialPathBudget {
            intervals: 1,
            ..Default::default()
        })
        .unwrap_err();
    assert!(error.to_string().contains("interval budget exhausted"));
    assert_eq!(source, before);
    let prepared = source
        .prepare_spatial_path(SpatialPathBudget {
            nodes: 4,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(prepared.node_count(), 4);
}
