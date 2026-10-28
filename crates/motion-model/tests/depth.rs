use motion_model::depth::*;

fn descriptor() -> DepthDescriptor {
    DepthDescriptor {
        size: [2, 2],
        source: DepthSource {
            asset: 7,
            revision: 11,
            pts: DepthTime {
                ticks: -1,
                timescale: 30,
            },
        },
        geometry: DepthGeometry {
            source_size: [8, 6],
            crop: [2, 1, 4, 4],
            quarter_turns: 0,
            mirror_x: false,
        },
        model: DepthModel {
            id: "test-owned-depth".into(),
            revision: "1".into(),
            sha256: [9; 32],
            processing: "test-raw-f32".into(),
        },
        measure: DepthMeasure::Relative {
            near: NearDirection::Increasing,
        },
        scale: 2.0,
        offset: -1.0,
        valid_range: None,
    }
}

#[test]
fn relative_depth_preserves_float_values_without_claiming_world_distance() {
    let depth = DepthResource::new(descriptor(), vec![0.0, 0.001, 0.5, 40.0], None, None).unwrap();
    assert_eq!(depth.sample(0), Some(-1.0));
    assert_eq!(depth.sample(3), Some(79.0));
    assert!(depth.sample(1).unwrap() > -1.0 && depth.sample(1).unwrap() < -0.99);
    assert!(depth
        .camera_z_meters(2, None)
        .unwrap_err()
        .to_string()
        .contains("calibration"));
    assert_eq!(depth.confidence(), None);
    assert_eq!(depth.validity(), None);
    assert_eq!(
        depth.descriptor().measure.near_direction(),
        NearDirection::Increasing
    );
    // The same prohibition applies to a plausible-looking normalised prediction.
    let mut d = descriptor();
    d.scale = 1.0;
    d.offset = 0.0;
    let normalised = DepthResource::new(d, vec![0.0, 0.25, 0.75, 1.0], None, None).unwrap();
    assert!(normalised.camera_z_meters(2, Some(1.0)).is_err());
}

#[test]
fn metric_inverse_and_ray_depth_require_explicit_units_and_axis_conversion() {
    let mut d = descriptor();
    d.scale = 1.0;
    d.offset = 0.0;
    d.measure = DepthMeasure::Metric {
        meters_per_unit: 0.001,
        axis: DepthAxis::CameraZ,
    };
    let depth =
        DepthResource::new(d.clone(), vec![1000.0, 2000.0, 3000.0, 4000.0], None, None).unwrap();
    assert_eq!(depth.camera_z_meters(1, None).unwrap(), Some(2.0));
    assert_eq!(d.measure.near_direction(), NearDirection::Decreasing);
    d.measure = DepthMeasure::InverseMetric {
        inverse_meters_per_unit: 0.5,
        axis: DepthAxis::RayDistance,
    };
    let inverse = DepthResource::new(d, vec![1.0, 2.0, 4.0, 8.0], None, None).unwrap();
    assert!(inverse.camera_z_meters(1, None).is_err());
    assert_eq!(inverse.camera_z_meters(1, Some(0.5)).unwrap(), Some(0.5));
    for cosine in [0.0, -1.0, 1.001, f64::NAN, f64::INFINITY] {
        assert!(inverse.camera_z_meters(1, Some(cosine)).is_err());
    }
    for value in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(
            DepthResource::new(inverse.descriptor().clone(), vec![value; 4], None, None).is_err()
        );
    }
}

#[test]
fn validity_and_confidence_are_independent_and_never_fabricated() {
    let depth = DepthResource::new(
        descriptor(),
        vec![0.0, f32::NAN, 0.5, f32::INFINITY],
        Some(vec![1, 0, 1, 0]),
        Some(vec![0.9, 0.0, 0.2, 0.0]),
    )
    .unwrap();
    assert_eq!(depth.sample(1), None);
    assert_eq!(depth.sample(3), None);
    assert_eq!(depth.sample(4), None);
    assert_eq!(depth.camera_z_meters(1, None).unwrap(), None);
    assert_eq!(depth.sample(2), Some(0.0));
    assert_eq!(depth.confidence().unwrap()[2], 0.2);
    assert!(DepthResource::new(descriptor(), vec![f32::NAN; 4], None, None).is_err());
    for mask in [vec![1; 3], vec![2; 4]] {
        assert!(DepthResource::new(descriptor(), vec![0.0; 4], Some(mask), None).is_err());
    }
    for scores in [vec![0.0; 3], vec![1.01; 4], vec![f32::NAN; 4]] {
        assert!(DepthResource::new(descriptor(), vec![0.0; 4], None, Some(scores)).is_err());
    }
}

#[test]
fn source_crop_rotation_and_mirror_map_continuous_uv_without_edge_clamping() {
    let mut geometry = descriptor().geometry;
    let source = [0.375, 0.5]; // crop-relative (1/4, 1/2)
    for (turn, expected) in [[0.25, 0.5], [0.5, 0.25], [0.75, 0.5], [0.5, 0.75]]
        .into_iter()
        .enumerate()
    {
        geometry.quarter_turns = turn as u8;
        geometry.mirror_x = false;
        assert_eq!(
            geometry.source_uv_to_depth_uv(source).unwrap(),
            Some(expected)
        );
        geometry.mirror_x = true;
        assert_eq!(
            geometry.source_uv_to_depth_uv(source).unwrap(),
            Some([1.0 - expected[0], expected[1]])
        );
    }
    assert_eq!(geometry.source_uv_to_depth_uv([0.0, 0.0]).unwrap(), None);
    assert!(geometry.source_uv_to_depth_uv([f64::NAN, 0.0]).is_err());
    geometry.quarter_turns = 4;
    assert!(geometry.source_uv_to_depth_uv(source).is_err());
    geometry.quarter_turns = 0;
    geometry.crop = [u32::MAX, 1, 4, 4];
    assert!(geometry.validate().is_err());
}

#[test]
fn exact_source_pts_distinguishes_replacement_and_seek_without_float_rounding() {
    let source = descriptor().source;
    let mut other = source.clone();
    other.pts = DepthTime {
        ticks: -2,
        timescale: 60,
    };
    assert!(source.same_sample(&other));
    other.pts.ticks += 1;
    assert!(!source.same_sample(&other));
    other = source.clone();
    other.revision += 1;
    assert!(!source.same_sample(&other));
    other = source.clone();
    other.asset += 1;
    assert!(!source.same_sample(&other));
    other = source.clone();
    other.pts.timescale = 0;
    assert!(!source.same_sample(&other));
    assert!(other.pts.validate().is_err());
    assert!(DepthTime {
        ticks: i64::MAX,
        timescale: u32::MAX
    }
    .same_instant(DepthTime {
        ticks: i64::MAX,
        timescale: u32::MAX
    }));
}

#[test]
fn descriptor_roundtrip_requires_semantics_and_bounded_model_identity() {
    let expected = descriptor();
    let value = serde_json::to_value(&expected).unwrap();
    let restored: DepthDescriptor = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(expected, restored);
    restored.validate().unwrap();
    for name in ["measure", "source", "model", "geometry", "scale", "offset"] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(name);
        assert!(
            serde_json::from_value::<DepthDescriptor>(missing).is_err(),
            "{name} is required"
        );
    }
    let mut invalid = expected.clone();
    invalid.model.id = "a".repeat(257);
    assert!(invalid.validate().is_err());
    invalid = expected.clone();
    invalid.model.processing = "bad\nprocessing".into();
    assert!(invalid.validate().is_err());
    invalid = expected;
    invalid.model.revision.clear();
    assert!(invalid.validate().is_err());
}

#[test]
fn dimensions_units_scale_and_declared_range_are_checked_before_installation() {
    let original = descriptor();
    for size in [[0, 1], [MAX_DEPTH_DIMENSION + 1, 1], [u32::MAX, u32::MAX]] {
        let mut d = original.clone();
        d.size = size;
        assert!(d.validate().is_err());
    }
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut d = original.clone();
        d.scale = scale;
        assert!(d.validate().is_err());
        d = original.clone();
        d.measure = DepthMeasure::Metric {
            meters_per_unit: scale,
            axis: DepthAxis::CameraZ,
        };
        assert!(d.validate().is_err());
    }
    let mut d = original.clone();
    d.offset = f64::INFINITY;
    assert!(d.validate().is_err());
    d = original.clone();
    d.scale = 1.0;
    d.offset = 0.0;
    d.valid_range = Some([0.0, 1.0]);
    assert!(DepthResource::new(d.clone(), vec![0.0, 0.5, 1.0, 2.0], None, None).is_err());
    DepthResource::new(
        d.clone(),
        vec![0.0, 0.5, 1.0, 2.0],
        Some(vec![1, 1, 1, 0]),
        None,
    )
    .unwrap();
    for range in [[1.0, 1.0], [1.0, 0.0], [0.0, f64::NAN]] {
        d.valid_range = Some(range);
        assert!(d.validate().is_err());
    }
    assert!(DepthResource::new(original, vec![0.0; 3], None, None).is_err());
}

#[test]
fn immutable_resource_identity_distinguishes_new_predictions_with_identical_metadata() {
    let original = DepthResource::new(descriptor(), vec![0., 1., 0., 1.], None, None).unwrap();
    let token = original.cache_identity();
    let cloned = original.clone();
    assert_eq!(token, cloned.cache_identity());
    let changed = DepthResource::new(descriptor(), vec![1., 0., 1., 0.], None, None).unwrap();
    assert_ne!(token, changed.cache_identity());
    let repeated =
        DepthResource::new(descriptor(), original.values().to_vec(), None, None).unwrap();
    assert_ne!(token, repeated.cache_identity());
    drop(original);
    assert_eq!(token, cloned.cache_identity());
}
