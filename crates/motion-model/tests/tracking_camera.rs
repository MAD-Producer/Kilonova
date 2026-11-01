use glam::{Mat4, Vec3};
use motion_model::{
    tracking::{asset::*, camera::*, plate::*, point::*, shared::SharedData},
    *,
};
#[path = "support/tracking_camera.rs"]
mod fixture;
use fixture::camera_fixture;
fn sample(p: &Project, frame: f64) -> CameraSample {
    let mut world = Vec::new();
    let mut states = Vec::new();
    let mut cameras = Vec::new();
    hierarchy::matrices_with_cameras(
        p,
        frame,
        &mut world,
        &mut states,
        &AuthoredCameraSampler,
        &mut cameras,
    )
    .unwrap();
    cameras[p.layers.len()].unwrap()
}
fn pixel(p: &Project, s: CameraSample, point: [f32; 3]) -> [f64; 2] {
    let q = s.pose.view_projection * Vec3::from_array(point).extend(1.);
    [
        f64::from((q.x / q.w + 1.) * 0.5) * f64::from(p.width),
        f64::from((1. - q.y / q.w) * 0.5) * f64::from(p.height),
    ]
}
#[test]
fn off_centre_anisotropic_intrinsics_and_original_fractional_pts_reproject_known_world_points() {
    let p = camera_fixture();
    let s = sample(&p, 0.5);
    let actual = pixel(&p, s, [75., -25., -200.]);
    let expected = [
        (215. * 0.7 / 7. + 142.5) * 2.,
        (182.5 * 0.225 / 7. + 129.5) * 2.,
    ];
    for i in 0..2 {
        assert!(
            (actual[i] - expected[i]).abs() < 0.0002,
            "{actual:?} {expected:?}"
        );
    }
    assert!((s.pose.eye.x - 5.).abs() < 0.0001 && (s.pose.eye.y + 2.5).abs() < 0.0001);
    assert!(p.camera.position.keys.is_empty());
    assert!(p.camera.pose(0., p.width, p.height).is_err());
    let copy: Project = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    assert_eq!(copy, p);
    assert!(copy.tracking_assets[0]
        .data
        .shares_storage(&copy.clone().tracking_assets[0].data));
}
#[test]
fn calibrated_projection_matches_independent_crop_rotation_and_near_far_depth() {
    let source = Extent::new(320, 240).unwrap();
    let output = Extent::new(641, 479).unwrap();
    let lens = Calibration::new(source, [245., 190.], [142., 130.], [0.; 3], [0.; 2]).unwrap();
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::HalfTurn,
        Rotation::Clockwise270,
    ] {
        let plate = PlateTransform::new(source, [32., 24., 256., 192.], rotation, output).unwrap();
        let m = calibrated_projection(lens, plate, 1., 0.5, 10000.).unwrap();
        let q = m * Vec3::new(0.6, 0.3, -3.).extend(1.);
        let actual = [
            f64::from((q.x / q.w + 1.) * 0.5) * 641.,
            f64::from((1. - q.y / q.w) * 0.5) * 479.,
        ];
        let (a, b, w, h) = match rotation {
            Rotation::None => (159., 87., 256., 192.),
            Rotation::Clockwise90 => (105., 159., 192., 256.),
            Rotation::HalfTurn => (97., 105., 256., 192.),
            Rotation::Clockwise270 => (87., 97., 192., 256.),
        };
        let expected = [a / w * 641., b / h * 479.];
        for i in 0..2 {
            assert!(
                (actual[i] - expected[i]).abs() < 0.0001,
                "{rotation:?} {actual:?} {expected:?}"
            );
        }
        for (z, depth) in [(-0.5, 0.), (-10000., 1.)] {
            let q = m * Vec3::new(0., 0., z).extend(1.);
            assert!((q.z / q.w - depth).abs() < 0.000001);
        }
    }
}
#[test]
fn authored_keyed_corrections_parent_and_focal_multiplier_are_composed_once() {
    let mut p = camera_fixture();
    let before = sample(&p, 1.);
    let position = p.camera.position.value;
    p.camera.position.upsert(0, position, Ease::Linear).unwrap();
    let mut corrected = position;
    corrected[0] += 20.;
    p.camera
        .position
        .upsert(2, corrected, Ease::Linear)
        .unwrap();
    let mut target = p.camera.target.value;
    target[0] += 20.;
    p.camera
        .target
        .upsert(0, p.camera.target.value, Ease::Linear)
        .unwrap();
    p.camera.target.upsert(2, target, Ease::Linear).unwrap();
    let corrected = sample(&p, 1.);
    assert!((corrected.pose.eye.x - before.pose.eye.x - 10.).abs() < 0.0001);
    let mut parent = Layer::solid(8, "parent", [1.; 2], [340., 250., 0.], [0.; 4]);
    parent.content = Content::Null;
    parent.three_d = true;
    p.layers.push(parent);
    p.camera.parent = Some(ParentLink {
        object: Some(8),
        bind: Mat4::IDENTITY.to_cols_array_2d(),
    });
    let parented = sample(&p, 1.);
    assert!((parented.pose.eye.x - corrected.pose.eye.x - 20.).abs() < 0.0001);
    assert!((parented.pose.eye.y - corrected.pose.eye.y + 10.).abs() < 0.0001);
    assert_eq!(p.camera.position.keys.len(), 2);
    let mut p = camera_fixture();
    let baseline = pixel(&p, sample(&p, 0.), [70., -20., -200.]);
    p.camera.fov.value = 30.;
    let zoom = (45f64.to_radians() * 0.5).tan() / (30f64.to_radians() * 0.5).tan();
    let changed = pixel(&p, sample(&p, 0.), [70., -20., -200.]);
    let principal = [284., 260.];
    for i in 0..2 {
        assert!((changed[i] - (principal[i] + (baseline[i] - principal[i]) * zoom)).abs() < 0.0002);
    }
}
#[test]
fn quaternion_signs_reverse_freeze_and_local_camera_clips_keep_original_clock() {
    let mut p = camera_fixture();
    let mut data = (*p.tracking_assets[0].data).clone();
    if let Some(Observation::Camera {
        ref mut camera_to_world,
        ..
    }) = data.tracks[0].samples[1].observation
    {
        camera_to_world.orientation_xyzw = [0., 0., 0., -1.];
    }
    p.tracking_assets[0].data = SharedData::new(data).unwrap();
    assert!((sample(&p, 0.5).pose.eye.x - 5.).abs() < 0.0001);
    let binding = p.camera.tracking.as_mut().unwrap();
    binding.source_anchor_pts = 0;
    binding.speed_numerator = -1;
    assert!((sample(&p, 1.).pose.eye.x - 10.).abs() < 0.0001);
    p.camera.tracking.as_mut().unwrap().speed_numerator = 0;
    assert!((sample(&p, 80.).pose.eye.x - 20.).abs() < 0.0001);
    let mut l = Layer::camera(7, "camera", p.width, p.height);
    l.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 100,
        offset_frame: 10,
    });
    if let Content::Camera { ref mut camera } = l.content {
        **camera = p.camera.clone();
        camera.tracking.as_mut().unwrap().speed_numerator = -1;
    }
    p.camera = Camera::new(p.width, p.height);
    p.layers.push(l);
    let mut worlds = Vec::new();
    let mut states = Vec::new();
    let mut cameras = Vec::new();
    hierarchy::matrices_with_cameras(
        &p,
        11.,
        &mut worlds,
        &mut states,
        &AuthoredCameraSampler,
        &mut cameras,
    )
    .unwrap();
    assert!((cameras[0].unwrap().pose.eye.x - 10.).abs() < 0.0001);
}
#[test]
fn unavailable_observations_follow_explicit_policy_but_distortion_never_silently_disappears() {
    let mut p = camera_fixture();
    let b = p.camera.tracking.as_mut().unwrap();
    b.max_gap_ticks = 0;
    let error = p
        .camera
        .tracking
        .as_ref()
        .unwrap()
        .sample(&p, &p.camera, 0, 0.5, Mat4::IDENTITY)
        .unwrap_err()
        .to_string();
    assert!(error.contains("local_frame=0.5") && error.contains("gap_ticks=2000"));
    p.camera.tracking.as_mut().unwrap().missing = Missing::AuthoredOnly;
    let s = sample(&p, 0.5);
    assert!(s.pose.eye.x.abs() < 0.0001);
    let mut p = camera_fixture();
    let mut data = (*p.tracking_assets[0].data).clone();
    if let Some(Observation::Camera {
        ref mut calibration,
        ..
    }) = data.tracks[0].samples[0].observation
    {
        *calibration = Calibration::new(
            data.source_extent,
            [210., 180.],
            [142., 130.],
            [0.01, 0., 0.],
            [0.; 2],
        )
        .unwrap();
    }
    p.tracking_assets[0].data = SharedData::new(data).unwrap();
    p.camera.tracking.as_mut().unwrap().missing = Missing::AuthoredOnly;
    let err = p
        .camera
        .tracking
        .as_ref()
        .unwrap()
        .sample(&p, &p.camera, 0, 0., Mat4::IDENTITY)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("distortion requires explicit output stage") && err.contains("radial=[0.01"),
        "{err}"
    );
}
#[test]
fn bad_selection_units_basis_and_correction_reference_are_refused() {
    for case in 0..5 {
        let mut p = camera_fixture();
        let b = p.camera.tracking.as_mut().unwrap();
        match case {
            0 => b.track = "absent".into(),
            1 => b.world_basis = [1, 2, -3],
            2 => b.target_meters_per_unit = 0.,
            3 => b.correction_reference.world.orientation_xyzw = [0.; 4],
            _ => b.far = b.near,
        };
        assert!(p.validate().is_err());
    }
    assert!(serde_json::from_str::<Command>(r#"{"op":"set_tracking_camera","object":0}"#).is_err());
}
