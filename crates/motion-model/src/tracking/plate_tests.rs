use super::*;
fn size() -> Extent {
    Extent::new(640, 480).unwrap()
}
fn calibration() -> Calibration {
    Calibration::new(size(), [100., 200.], [320., 240.], [0.; 3], [0.; 2]).unwrap()
}
fn close(a: [f64; 2], b: [f64; 2]) {
    for axis in 0..2 {
        assert!((a[axis] - b[axis]).abs() < 1e-9, "{a:?} != {b:?}");
    }
}
#[test]
fn known_camera_projects_in_declared_image_coordinates() {
    let camera = calibration();
    assert_eq!(camera.source(), size());
    close(camera.project([1., 2., 10.]).unwrap(), [330., 280.]);
    close(camera.project([0., 0., 5.]).unwrap(), [320., 240.]);
    assert_eq!(camera.residual([1., 2., 10.], [333., 284.]).unwrap(), 5.);
}
#[test]
fn radial_and_tangential_models_have_independent_analytic_controls() {
    let radial = Calibration::new(size(), [100.; 2], [0.; 2], [0.2, 0., 0.], [0.; 2]).unwrap();
    close(radial.project([0.5, 0., 1.]).unwrap(), [52.5, 0.]);
    let tangent = Calibration::new(size(), [1.; 2], [0.; 2], [0.; 3], [0.1, 0.2]).unwrap();
    close(tangent.project([1., 2., 1.]).unwrap(), [2.8, 4.1]);
}
#[test]
fn crop_rotation_then_resize_has_known_pixel_controls() {
    let plain = PlateTransform::new(
        size(),
        [100., 50., 200., 100.],
        Rotation::None,
        Extent::new(400, 200).unwrap(),
    )
    .unwrap();
    close(plain.apply([125., 75.]).unwrap(), [50., 50.]);
    let rotated = PlateTransform::new(
        size(),
        [100., 50., 200., 100.],
        Rotation::Clockwise90,
        Extent::new(200, 400).unwrap(),
    )
    .unwrap();
    close(rotated.apply([125., 75.]).unwrap(), [150., 50.]);
    assert_eq!(rotated.source(), size());
    assert_eq!(rotated.output().pixels(), [200, 400]);
    for (rotation, output, expected) in [
        (Rotation::HalfTurn, [400, 200], [350., 150.]),
        (Rotation::Clockwise270, [200, 400], [50., 350.]),
    ] {
        let transform = PlateTransform::new(
            size(),
            [100., 50., 200., 100.],
            rotation,
            Extent::new(output[0], output[1]).unwrap(),
        )
        .unwrap();
        close(transform.apply([125., 75.]).unwrap(), expected);
    }
}
#[test]
fn every_rotation_roundtrips_subpixel_and_off_image_points() {
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::HalfTurn,
        Rotation::Clockwise270,
    ] {
        let transform = PlateTransform::new(
            size(),
            [10.5, 20.25, 200.5, 100.75],
            rotation,
            Extent::new(317, 291).unwrap(),
        )
        .unwrap();
        for n in 0..100 {
            let point = [f64::from(n) * 7.123 - 40., f64::from(n) * 3.71 - 50.];
            close(
                transform.inverse(transform.apply(point).unwrap()).unwrap(),
                point,
            );
        }
    }
}
#[test]
fn pixel_centers_are_not_confused_with_edges_or_clamped() {
    let transform = PlateTransform::new(
        size(),
        [0., 0., 640., 480.],
        Rotation::Clockwise90,
        Extent::new(480, 640).unwrap(),
    )
    .unwrap();
    close(transform.apply([0.5, 0.5]).unwrap(), [479.5, 0.5]);
    close(transform.apply([-1., -2.]).unwrap(), [482., -1.]);
    assert!(calibration().project([100., 0., 1.]).unwrap()[0] > 640.);
}
#[test]
fn behind_camera_and_non_finite_data_keep_distinct_causes() {
    let camera = calibration();
    for z in [0., -1.] {
        assert_eq!(camera.project([0., 0., z]), Err(Error::BehindCamera));
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(camera.project([bad, 0., 1.]), Err(Error::NonFinitePoint));
        assert_eq!(
            camera.residual([0., 0., 1.], [bad, 0.]),
            Err(Error::NonFinitePoint)
        );
    }
    assert_eq!(
        camera.project([f64::MAX, 1., f64::MIN_POSITIVE]),
        Err(Error::NonFiniteProjection)
    );
}
#[test]
fn malformed_calibration_and_extent_are_rejected_before_projection() {
    for [w, h] in [[0, 480], [640, 0], [32769, 1]] {
        assert_eq!(Extent::new(w, h), Err(Error::InvalidExtent));
    }
    for focal in [[0., 1.], [-1., 1.], [1., f64::NAN]] {
        assert!(matches!(
            Calibration::new(size(), focal, [0.; 2], [0.; 3], [0.; 2]),
            Err(Error::InvalidCalibration)
        ));
    }
    assert!(matches!(
        Calibration::new(size(), [1.; 2], [0.; 2], [f64::INFINITY, 0., 0.], [0.; 2]),
        Err(Error::InvalidCalibration)
    ));
}
#[test]
fn invalid_crop_never_becomes_a_silent_default_transform() {
    for crop in [
        [0., 0., 0., 1.],
        [-1., 0., 1., 1.],
        [600., 0., 41., 10.],
        [0., 479., 1., 2.],
        [0., 0., f64::NAN, 1.],
    ] {
        assert!(matches!(
            PlateTransform::new(size(), crop, Rotation::None, size()),
            Err(Error::InvalidCrop)
        ));
    }
}
