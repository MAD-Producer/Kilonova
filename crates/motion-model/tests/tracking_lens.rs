use motion_model::tracking::{lens::LensWarp, plate::*};

fn lens(rotation: Rotation, k: [f64; 3], t: [f64; 2], zoom: f64) -> LensWarp {
    let source = Extent::new(320, 240).unwrap();
    LensWarp::new(
        Calibration::new(source, [245., 210.], [154., 119.], k, t).unwrap(),
        PlateTransform::new(
            source,
            [16., 12., 288., 216.],
            rotation,
            Extent::new(640, 480).unwrap(),
        )
        .unwrap(),
        zoom,
    )
    .unwrap()
}
const ROTATIONS: [Rotation; 4] = [
    Rotation::None,
    Rotation::Clockwise90,
    Rotation::HalfTurn,
    Rotation::Clockwise270,
];

#[test]
fn inverse_matches_independent_newton_oracle_after_crop_rotation_and_focal_correction() {
    for rotation in ROTATIONS {
        for zoom in [0.8, 1., 1.5] {
            let lens = lens(
                rotation,
                [0.004, -0.00005, 0.000001],
                [0.0008, -0.0006],
                zoom,
            );
            for pixel in [
                [0., 0.],
                [640., 0.],
                [640., 480.],
                [0., 480.],
                [321.3, 172.9],
            ] {
                let source = lens.plate().inverse(pixel).unwrap();
                let c = lens.calibration();
                let f = c.focal().map(|v| v * zoom);
                let centre = c.principal();
                let d = [
                    (source[0] - centre[0]) / f[0],
                    (source[1] - centre[1]) / f[1],
                ];
                // Independent Newton solve with the analytic Brown-Conrady
                // Jacobian, not the production fixed-point iteration or its helper.
                let mut q = d;
                for _ in 0..12 {
                    let [x, y] = q;
                    let r = x * x + y * y;
                    let [a, b, k3] = c.radial();
                    let [p, s] = c.tangential();
                    let radial = 1. + a * r + b * r * r + k3 * r * r * r;
                    let derivative = a + 2. * b * r + 3. * k3 * r * r;
                    let error = [
                        x * radial + 2. * p * x * y + s * (r + 2. * x * x) - d[0],
                        y * radial + p * (r + 2. * y * y) + 2. * s * x * y - d[1],
                    ];
                    let j00 = radial + 2. * x * x * derivative + 2. * p * y + 6. * s * x;
                    let j11 = radial + 2. * y * y * derivative + 6. * p * y + 2. * s * x;
                    let j01 = 2. * x * y * derivative + 2. * p * x + 2. * s * y;
                    let det = j00 * j11 - j01 * j01;
                    q[0] -= (j11 * error[0] - j01 * error[1]) / det;
                    q[1] -= (j00 * error[1] - j01 * error[0]) / det;
                }
                let expected = lens
                    .plate()
                    .apply([f[0] * q[0] + centre[0], f[1] * q[1] + centre[1]])
                    .unwrap();
                let actual = lens.inverse(pixel).unwrap();
                assert!(
                    (actual[0] - expected[0]).hypot(actual[1] - expected[1]) < 1e-8,
                    "{rotation:?} {zoom} {actual:?} {expected:?}"
                );
                let back = lens.forward(actual).unwrap();
                assert!((back[0] - pixel[0]).hypot(back[1] - pixel[1]) < 1e-7);
                let projected = lens.calibration().project([q[0], q[1], 1.]).unwrap();
                let corrected = [
                    centre[0] + zoom * (projected[0] - centre[0]),
                    centre[1] + zoom * (projected[1] - centre[1]),
                ];
                let direct = lens.plate().apply(corrected).unwrap();
                assert!((direct[0] - pixel[0]).hypot(direct[1] - pixel[1]) < 1e-7);
            }
        }
    }
}
#[test]
fn certified_overscan_covers_continuous_inverse_and_projection_at_preview_scales() {
    for rotation in ROTATIONS {
        for sign in [-1., 1.] {
            let lens = lens(rotation, [sign * 0.012, 0.0001, 0.], [0.001, -0.0008], 1.);
            for scale in [0.125, 0.5, 1., 2.] {
                let raster = lens.raster(scale).unwrap();
                lens.validate_raster(raster).unwrap();
                let [x, y, w, h] = raster.rect();
                let projection = raster.projection(lens.output_size()).unwrap();
                for gy in 0..=24 {
                    for gx in 0..=32 {
                        let p = lens.inverse([gx as f64 * 20., gy as f64 * 20.]).unwrap();
                        assert!(p[0] >= x && p[0] <= x + w && p[1] >= y && p[1] <= y + h);
                        let q = projection
                            * glam::Vec4::new(
                                (p[0] / 640. * 2. - 1.) as f32,
                                (1. - p[1] / 480. * 2.) as f32,
                                0.37,
                                1.,
                            );
                        assert!((f64::from(q.x) - (2. * (p[0] - x) / w - 1.)).abs() < 1e-6);
                        assert!((f64::from(q.y) - (1. - 2. * (p[1] - y) / h)).abs() < 1e-6);
                        assert_eq!(q.z, 0.37);
                        assert_eq!(q.w, 1.);
                    }
                }
                assert!((w * scale - raster.size()[0] as f64).abs() < 1e-8);
                assert!((h * scale - raster.size()[1] as f64).abs() < 1e-8);
            }
        }
    }
}
#[test]
fn identity_retains_the_original_raster_and_does_not_invent_a_crop() {
    for rotation in ROTATIONS {
        let lens = lens(rotation, [0.; 3], [0.; 2], 1.3);
        assert!(lens.is_identity());
        for scale in [0.125, 1., 2.] {
            let r = lens.raster(scale).unwrap();
            assert_eq!(r.rect(), [0., 0., 640., 480.]);
            assert_eq!(r.size(), [(640. * scale) as u32, (480. * scale) as u32]);
        }
        for p in [[0., 0.], [640., 480.], [321.5, 210.25]] {
            let q = lens.inverse(p).unwrap();
            assert!((q[0] - p[0]).hypot(q[1] - p[1]) < 1e-10);
        }
    }
}
#[test]
fn folding_unrepresentable_domains_and_incomplete_rasters_fail_explicitly() {
    let source = Extent::new(320, 240).unwrap();
    let plate = PlateTransform::new(source, [0., 0., 320., 240.], Rotation::None, source).unwrap();
    let c = |k| Calibration::new(source, [245., 210.], [154., 119.], k, [0.; 2]).unwrap();
    let error = LensWarp::new(c([-2., 0., 0.]), plate, 1.)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("cannot certify a unique domain") && error.contains("derivative_bound=")
    );
    for zoom in [0., -1., f64::NAN, f64::INFINITY, 1e100] {
        assert!(LensWarp::new(c([0.; 3]), plate, zoom).is_err());
    }
    let c = Calibration::new(source, [1e9; 2], [0.; 2], [0., 0., 1e50], [0.; 2]).unwrap();
    assert!(LensWarp::new(c, plate, 1.)
        .unwrap_err()
        .to_string()
        .contains("finite shader range"));
    let warp = lens(Rotation::None, [-0.01, 0., 0.], [0.; 2], 1.);
    let too_small = lens(Rotation::None, [0.; 3], [0.; 2], 1.)
        .raster(1.)
        .unwrap();
    assert!(warp.validate_raster(too_small).is_err());
    for point in [[-1., 0.], [0., 481.], [f64::NAN, 0.]] {
        assert!(warp.inverse(point).is_err());
    }
    for scale in [0., -1., 17., f64::NAN] {
        assert!(warp.raster(scale).is_err());
    }
    assert!(warp.forward([1e20, 1e20]).is_err());
    let huge = Extent::new(32768, 32768).unwrap();
    let c = Calibration::new(huge, [1e5; 2], [16000.; 2], [0.; 3], [0.; 2]).unwrap();
    let warp = LensWarp::new(
        c,
        PlateTransform::new(huge, [0., 0., 32768., 32768.], Rotation::None, huge).unwrap(),
        1.,
    )
    .unwrap();
    assert!(warp
        .raster(1.)
        .unwrap_err()
        .to_string()
        .contains("16-megapixel"));
}
