use motion_model::{
    tracking::{lens::LensWarp, plate::*},
    *,
};

#[test]
fn a_small_derivative_without_an_enclosing_inverse_disk_is_not_admitted() {
    let extent = Extent::new(200, 200).unwrap();
    let plate = PlateTransform::new(extent, [0., 0., 200., 200.], Rotation::None, extent).unwrap();
    let focal = 100. * 2f64.sqrt() / 1.2;
    let calibration =
        |k| Calibration::new(extent, [focal; 2], [100.; 2], [k, 0., 0.], [0.; 2]).unwrap();
    // At the smallest candidate radius ~1.2, ||Jg||~0.432 is below 0.45,
    // but the inverse corner lies outside that disk. Larger enclosing disks
    // exceed the derivative limit. Testing just B would falsely admit this.
    assert!(LensWarp::new(calibration(-0.1), plate, 1.)
        .unwrap_err()
        .to_string()
        .contains("cannot certify a unique domain"));
    let valid = LensWarp::new(calibration(-0.05), plate, 1.).unwrap();
    let inverse = valid.inverse([0., 0.]).unwrap();
    let back = valid.forward(inverse).unwrap();
    assert!(back[0].hypot(back[1]) < 1e-7);
}
#[path = "support/photographed_camera.rs"]
mod measured;

#[test]
fn real_fitted_radial_cancellation_is_certified_without_reducing_distortion() {
    let m = measured::Measurements::load();
    let c = m.calibration();
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::HalfTurn,
        Rotation::Clockwise270,
    ] {
        let plate =
            PlateTransform::new(c.source(), [0., 0., 640., 480.], rotation, c.source()).unwrap();
        let lens = LensWarp::new(c, plate, 1.).unwrap();
        assert_eq!(lens.calibration(), c);
        assert!(lens.derivative_bound() < 0.45);
        let raster = lens.raster(1.).unwrap();
        let [x, y, w, h] = raster.rect();
        for oracle in &m.inverse_oracle {
            let pixel = plate.apply(oracle.distorted).unwrap();
            let expected = plate.apply(oracle.ideal).unwrap();
            let actual = lens.inverse(pixel).unwrap();
            assert!(
                (actual[0] - expected[0]).hypot(actual[1] - expected[1]) < 1e-6,
                "{rotation:?} {actual:?} {expected:?}"
            );
            assert!(actual[0] >= x && actual[0] <= x + w && actual[1] >= y && actual[1] <= y + h);
            let back = lens.forward(actual).unwrap();
            assert!((back[0] - pixel[0]).hypot(back[1] - pixel[1]) < 1e-7);
            let drag = lens.inverse_in_certified_domain(pixel).unwrap();
            assert_eq!(drag, actual);
        }
    }
}

#[test]
fn imported_measured_poses_match_external_projection_and_disjoint_real_corners() {
    let m = measured::Measurements::load();
    let p = m.project();
    for (i, view) in m.views.iter().enumerate() {
        assert!(view
            .pose_fit_indices
            .iter()
            .all(|n| !view.validation_indices.contains(n)));
        assert_eq!(view.pose_fit_indices.len(), 27);
        assert_eq!(view.validation_indices.len(), 27);
        let sampled = p
            .camera
            .tracking
            .as_ref()
            .unwrap()
            .sample(&p, &p.camera, 0, i as f64, glam::Mat4::IDENTITY)
            .unwrap();
        let lens = sampled.lens.unwrap();
        let mut sum = 0.;
        let mut max = 0f64;
        for &index in &view.validation_indices {
            let [x, y, z] = m.object_points[index];
            let world = glam::Vec4::new(
                (x * 100.) as f32,
                (-y * 100.) as f32,
                (-z * 100.) as f32,
                1.,
            );
            let q = sampled.pose.view_projection * world;
            let ideal = [
                f64::from(q.x / q.w + 1.) * 320.,
                f64::from(1. - q.y / q.w) * 240.,
            ];
            let actual = lens.forward(ideal).unwrap();
            let external = view.projected[index];
            assert!(
                (actual[0] - external[0]).hypot(actual[1] - external[1]) < 0.0003,
                "{} {index}: {actual:?} {external:?}",
                view.name
            );
            let observed = m.measured_corners[&view.name][index];
            let error = (actual[0] - observed[0]).hypot(actual[1] - observed[1]);
            sum += error * error;
            max = max.max(error);
        }
        let rms = (sum / 27.).sqrt();
        assert!(rms <= 1. && max <= 2.5, "{} rms={rms} max={max}", view.name);
        assert!((rms - view.held_out_rms).abs() < 0.0003);
        assert!((max - view.held_out_max).abs() < 0.0003);
    }
    assert!(p.camera.position.keys.is_empty());
    let copy: Project = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    assert_eq!(copy, p);
}
