use glam::{Mat4, Vec3};
use motion_model::{depth::*, CameraPose, ProjectionKind};

fn camera() -> CameraPose {
    CameraPose {
        eye: Vec3::new(0., 0., 100.),
        target: Vec3::ZERO,
        projection: ProjectionKind::Orthographic,
        view_projection: Mat4::orthographic_rh(-40., 40., -20., 20., 0.5, 1000.)
            * Mat4::look_at_rh(Vec3::new(0., 0., 100.), Vec3::ZERO, Vec3::Y),
    }
}
fn resource(measure: DepthMeasure, values: Vec<f32>, validity: Option<Vec<u8>>) -> DepthResource {
    DepthResource::new(
        DepthDescriptor {
            size: [2, 1],
            source: DepthSource {
                asset: 76,
                revision: 1,
                pts: DepthTime {
                    ticks: 1,
                    timescale: 24,
                },
            },
            geometry: DepthGeometry {
                source_size: [8, 4],
                crop: [0, 0, 8, 4],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "authored-metric-geometry".into(),
                revision: "1".into(),
                sha256: [76; 32],
                processing: "camera-z-control-v1".into(),
            },
            measure,
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
fn reference() -> DepthReference {
    DepthReference {
        camera: camera(),
        world: Mat4::IDENTITY,
        size: [80., 40.],
    }
}

#[test]
fn metric_and_inverse_camera_z_reach_surfaces_and_clip_depth_with_explicit_units() {
    for (measure, values) in [
        (
            DepthMeasure::Metric {
                meters_per_unit: 0.001,
                axis: DepthAxis::CameraZ,
            },
            vec![8000., 12000.],
        ),
        (
            DepthMeasure::InverseMetric {
                inverse_meters_per_unit: 1000.,
                axis: DepthAxis::CameraZ,
            },
            vec![1. / 8000., 1. / 12000.],
        ),
    ] {
        let resource = resource(measure, values, None);
        let reference = reference();
        let surface =
            DepthSurface::from_camera_z(&resource, &reference, 10., MAX_DEPTH_RESOURCE_BYTES)
                .unwrap();
        assert_eq!(surface.vertices().len(), 36);
        for vertex in surface.vertices() {
            let point = Vec3::from_array(vertex.position);
            let distance = if vertex.uv[0] < 0.5 { 80. } else { 120. };
            assert!(((reference.camera.eye - point).z - distance).abs() < 2e-5);
            assert!((point.x - (vertex.uv[0] - 0.5) * 80.).abs() < 2e-5);
            assert!((point.y - (0.5 - vertex.uv[1]) * 40.).abs() < 2e-5);
            assert!((vertex.texture_weight - 1.).abs() < 1e-6);
        }
        let projected = ProjectedDepth::from_camera_z(
            &resource,
            DepthProjection::new(camera(), 10.).unwrap(),
            [4, 2],
        )
        .unwrap();
        assert_eq!(projected.source(), resource.descriptor());
        for (index, value) in projected.samples().iter().enumerate() {
            let distance = if index % 4 < 2 { 80. } else { 120. };
            assert!((f64::from(value[0]) - (distance - 0.5) / 999.5).abs() < 1e-6);
            assert_eq!(value[1], 1.);
        }
    }
}

#[test]
fn metric_invalid_samples_keep_flat_geometry_and_missing_clip_validity() {
    let resource = resource(
        DepthMeasure::Metric {
            meters_per_unit: 1.,
            axis: DepthAxis::CameraZ,
        },
        vec![f32::NAN, 8.],
        Some(vec![0, 1]),
    );
    let reference = reference();
    let surface =
        DepthSurface::from_camera_z(&resource, &reference, 10., MAX_DEPTH_RESOURCE_BYTES).unwrap();
    for vertex in surface.vertices() {
        assert!((vertex.position[2] - if vertex.uv[0] < 0.5 { 0. } else { 20. }).abs() < 2e-5);
    }
    let projected = ProjectedDepth::from_camera_z(
        &resource,
        DepthProjection::new(camera(), 10.).unwrap(),
        [2, 1],
    )
    .unwrap();
    assert_eq!(projected.samples()[0], [1., 0.]);
    assert_eq!(projected.samples()[1][1], 1.);
}

#[test]
fn uncalibrated_relative_ray_distance_and_too_small_geometry_budgets_refuse() {
    for measure in [
        DepthMeasure::Relative {
            near: NearDirection::Increasing,
        },
        DepthMeasure::Metric {
            meters_per_unit: 1.,
            axis: DepthAxis::RayDistance,
        },
        DepthMeasure::InverseMetric {
            inverse_meters_per_unit: 1.,
            axis: DepthAxis::RayDistance,
        },
    ] {
        let resource = resource(measure, vec![1., 2.], None);
        assert!(DepthSurface::from_camera_z(
            &resource,
            &reference(),
            10.,
            MAX_DEPTH_RESOURCE_BYTES
        )
        .unwrap_err()
        .to_string()
        .contains("explicit metric camera-Z"));
        assert!(ProjectedDepth::from_camera_z(
            &resource,
            DepthProjection::new(camera(), 10.).unwrap(),
            [2, 1]
        )
        .unwrap_err()
        .to_string()
        .contains("explicit metric camera-Z"));
    }
    let resource = resource(
        DepthMeasure::Metric {
            meters_per_unit: 1.,
            axis: DepthAxis::CameraZ,
        },
        vec![8., 12.],
        None,
    );
    assert!(DepthSurface::from_camera_z(&resource, &reference(), 10., 1)
        .unwrap_err()
        .to_string()
        .contains("byte budget"));
    assert!(ProjectedDepth::from_camera_z(
        &resource,
        DepthProjection::new(camera(), 10.).unwrap(),
        [4097, 1]
    )
    .unwrap_err()
    .to_string()
    .contains("dimensions exceed limits"));
}
