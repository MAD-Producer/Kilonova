use motion_model::{AuthoredCameraSampler, Camera, CameraSampler, DepthOfField, Track};

fn aperture() -> DepthOfField {
    DepthOfField {
        focus_distance: Track::constant(100.),
        aperture_diameter: Track::constant(10.),
        max_radius_pixels: 8.,
    }
}
#[test]
fn saved_camera_aperture_samples_its_own_clock_and_rejects_unbounded_values() {
    let mut camera = Camera::new(1920, 1080);
    camera.depth_of_field = Some(aperture());
    camera
        .depth_of_field
        .as_mut()
        .unwrap()
        .focus_distance
        .set_animated(0, true)
        .unwrap();
    camera
        .depth_of_field
        .as_mut()
        .unwrap()
        .focus_distance
        .set_at(0, 100.)
        .unwrap();
    camera
        .depth_of_field
        .as_mut()
        .unwrap()
        .focus_distance
        .set_at(10, 200.)
        .unwrap();
    camera.validate(60).unwrap();
    let encoded = serde_json::to_vec(&camera).unwrap();
    let restored: Camera = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(restored, camera);
    for (frame, focus) in [(0., 100.), (10., 200.)] {
        let sample = AuthoredCameraSampler
            .sample(&restored, frame, 30, [1920, 1080], glam::Mat4::IDENTITY)
            .unwrap();
        assert_eq!(sample.depth_of_field.unwrap().focus_distance, focus);
    }
    for invalid in [0., f32::NAN, f32::INFINITY, 17.] {
        let mut bad = restored.clone();
        bad.depth_of_field.as_mut().unwrap().max_radius_pixels = invalid;
        assert!(bad.validate(60).is_err());
    }
    let mut bad = restored.clone();
    bad.depth_of_field.as_mut().unwrap().focus_distance.value = 0.;
    assert!(bad.validate(60).is_err());
    let mut bad = restored;
    bad.depth_of_field.as_mut().unwrap().aperture_diameter.value = -1.;
    assert!(bad.validate_local().is_err());
    let pinhole = Camera::new(1920, 1080);
    assert!(AuthoredCameraSampler
        .sample(&pinhole, 0., 30, [1920, 1080], glam::Mat4::IDENTITY)
        .unwrap()
        .depth_of_field
        .is_none());
}
