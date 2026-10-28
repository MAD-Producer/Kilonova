use glam::{Mat4, Vec3};
use motion_model::{depth::*, Camera, CameraPose};

fn camera() -> CameraPose {
    CameraPose {
        eye: Vec3::new(0., 0., 100.),
        target: Vec3::ZERO,
        projection: Camera::new(96, 64).pose(0., 96, 64).unwrap().projection,
        view_projection: Mat4::perspective_rh(60f32.to_radians(), 2., 0.5, 1000.)
            * Mat4::look_at_rh(Vec3::new(0., 0., 100.), Vec3::ZERO, Vec3::Y),
    }
}
fn resource(size: [u32; 2], geometry: DepthGeometry, validity: Option<Vec<u8>>) -> DepthResource {
    let n = size[0] as usize * size[1] as usize;
    DepthResource::new(
        DepthDescriptor {
            size,
            source: DepthSource {
                asset: 7,
                revision: 2,
                pts: DepthTime {
                    ticks: 0,
                    timescale: 1,
                },
            },
            geometry,
            model: DepthModel {
                id: "owned".into(),
                revision: "1".into(),
                sha256: [1; 32],
                processing: "owned-v1".into(),
            },
            measure: DepthMeasure::Relative {
                near: NearDirection::Increasing,
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        },
        (0..n).map(|i| (i % 2) as f32).collect(),
        validity,
        None,
    )
    .unwrap()
}
fn geometry() -> DepthGeometry {
    DepthGeometry {
        source_size: [8, 4],
        crop: [0, 0, 8, 4],
        quarter_turns: 0,
        mirror_x: false,
    }
}
fn calibration(resource: &DepthResource) -> RelativeDepthCalibration {
    RelativeDepthCalibration::new(
        resource.descriptor().clone(),
        [
            DepthAnchor {
                value: 0.,
                camera_z_meters: 12.,
            },
            DepthAnchor {
                value: 1.,
                camera_z_meters: 8.,
            },
        ],
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
    )
    .unwrap()
}
#[test]
fn reference_surface_preserves_source_projection_and_moves_with_layer_transform() {
    let resource = resource([2, 1], geometry(), None);
    let bound = calibration(&resource);
    let reference = DepthReference {
        camera: camera(),
        world: Mat4::from_rotation_z(0.2) * Mat4::from_translation(Vec3::new(90., 3., -5.)),
        size: [80., 40.],
    };
    let surface = DepthSurface::new(&bound.bind(&resource).unwrap(), &reference, 10.).unwrap();
    assert_eq!(surface.vertices().len(), 36);
    assert_eq!(surface.reference_size(), reference.size);
    let forward = (reference.camera.target - reference.camera.eye).normalize();
    for v in surface.vertices() {
        let local = Vec3::from_array(v.position);
        let world = reference.world.transform_point3(local);
        let flat = reference.world.transform_point3(Vec3::new(
            (v.uv[0] - 0.5) * 80.,
            (0.5 - v.uv[1]) * 40.,
            0.,
        ));
        let a = reference.camera.view_projection * world.extend(1.);
        let b = reference.camera.view_projection * flat.extend(1.);
        assert!((a.x / a.w - b.x / b.w).abs() < 2e-6 && (a.y / a.w - b.y / b.w).abs() < 2e-6);
        let z = if v.uv[0] < 0.5 { 120. } else { 80. };
        assert!(((world - reference.camera.eye).dot(forward) - z).abs() < 2e-5);
        let moved = Mat4::from_translation(Vec3::X * 17.) * reference.world;
        assert!((moved.transform_point3(local) - world - Vec3::X * 17.).length() < 2e-5);
    }
    let projection = DepthProjection::new(camera(), 10.).unwrap();
    assert!(projection
        .project_camera_z([1.4, 0.5], 8.)
        .unwrap()
        .is_none());
    assert!(projection
        .unproject_camera_z([1.4, 0.5], 8.)
        .unwrap()
        .is_finite());
}
#[test]
fn oriented_crop_and_invalid_samples_keep_original_flat_unknown_geometry() {
    for turns in 0..4 {
        for mirror in [false, true] {
            let g = DepthGeometry {
                crop: [2, 1, 4, 2],
                quarter_turns: turns,
                mirror_x: mirror,
                ..geometry()
            };
            let resource = resource([2, 1], g.clone(), Some(vec![0, 1]));
            let calibration = calibration(&resource);
            let reference = DepthReference {
                camera: camera(),
                world: Mat4::IDENTITY,
                size: [80., 40.],
            };
            let surface =
                DepthSurface::new(&calibration.bind(&resource).unwrap(), &reference, 10.).unwrap();
            let mut known = 0;
            let mut unknown = 0;
            for v in surface.vertices() {
                let uv = v.uv.map(f64::from);
                let known_sample = g
                    .source_uv_to_depth_uv(uv)
                    .unwrap()
                    .is_some_and(|[u, _]| (u * 2.) as usize >= 1);
                if known_sample {
                    known += 1;
                    assert!((v.position[2] - 20.).abs() < 1e-4);
                } else {
                    unknown += 1;
                    assert_eq!(v.position[2], 0.);
                }
            }
            assert!(known > 0 && unknown > 0);
        }
    }
}
#[test]
fn surface_refuses_over_budget_and_saved_reference_rejects_singular_geometry() {
    let resource = resource([1400, 1400], geometry(), None);
    let binding = calibration(&resource);
    let reference = DepthReference {
        camera: camera(),
        world: Mat4::IDENTITY,
        size: [80., 40.],
    };
    assert!(
        DepthSurface::new(&binding.bind(&resource).unwrap(), &reference, 10.)
            .unwrap_err()
            .to_string()
            .contains("budget")
    );
    let source = self::resource([2, 1], geometry(), None);
    let saved = SavedDepthCalibration::new(
        &source,
        PreparedDepthInput {
            rgb_size: [8, 4],
            rgb_sha256: [1; 32],
        },
        *calibration(&source).anchors(),
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
        10.,
    )
    .unwrap()
    .with_reference(reference)
    .unwrap();
    let json = serde_json::to_value(&saved).unwrap();
    assert_eq!(
        serde_json::from_value::<SavedDepthCalibration>(json.clone()).unwrap(),
        saved
    );
    let mut bad = json;
    bad["reference"]["world"] = serde_json::to_value([0.; 16]).unwrap();
    assert!(serde_json::from_value::<SavedDepthCalibration>(bad).is_err());
}

#[test]
fn orthographic_reference_keeps_parallel_rays_and_unit_texture_weights() {
    let mut observer = motion_model::Observer::new(96, 64);
    observer.view = motion_model::ObservationView::Top;
    let camera = observer.pose(96, 64).unwrap();
    let reference = DepthReference {
        camera,
        world: Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        size: [80., 40.],
    };
    let resource = resource([2, 1], geometry(), None);
    let binding = calibration(&resource);
    let surface = DepthSurface::new(&binding.bind(&resource).unwrap(), &reference, 10.).unwrap();
    let forward = (camera.target - camera.eye).normalize();
    for vertex in surface.vertices() {
        assert!((vertex.texture_weight - 1.).abs() < 1e-6);
        let point = reference
            .world
            .transform_point3(Vec3::from_array(vertex.position));
        let expected = if vertex.uv[0] < 0.5 { 120. } else { 80. };
        assert!(((point - camera.eye).dot(forward) - expected).abs() < 2e-5);
    }
}
