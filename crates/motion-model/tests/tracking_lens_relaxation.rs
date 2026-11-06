//! Regression from independently recalibrated real photographs; not a synthetic
//! roundtrip through the same production inverse or a relaxed admission limit.
use motion_model::tracking::{lens::LensWarp, plate::*};
use serde::Deserialize;

#[derive(Deserialize)]
struct Oracle {
    extent: [u32; 2],
    focal: [f64; 2],
    principal: [f64; 2],
    radial: [f64; 3],
    tangential: [f64; 2],
    samples: Vec<Sample>,
}
#[derive(Deserialize)]
struct Sample {
    distorted: [f64; 2],
    undistorted: [f64; 2],
}

#[test]
fn strong_photographed_lens_matches_external_inverse_without_weakening_the_certificate() {
    let data: Oracle =
        serde_json::from_str(include_str!("fixtures/tracking-opencv-inverse.json")).unwrap();
    let source = Extent::new(data.extent[0], data.extent[1]).unwrap();
    let calibration = Calibration::new(
        source,
        data.focal,
        data.principal,
        data.radial,
        data.tangential,
    )
    .unwrap();
    assert_eq!(motion_model::tracking::lens::INVERSE_ITERATIONS, 48);
    assert_eq!(data.samples.len(), 63);
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::HalfTurn,
        Rotation::Clockwise270,
    ] {
        let plate = PlateTransform::new(
            source,
            [0., 0., 640., 480.],
            rotation,
            Extent::new(719, 431).unwrap(),
        )
        .unwrap();
        let lens = LensWarp::new(calibration, plate, 1.).unwrap();
        assert!(lens.inverse_step() > 1.);
        assert!(lens.derivative_bound() < 0.45);
        assert_eq!(lens.inverse_step(), f64::from(lens.inverse_step() as f32));
        assert_eq!(lens.calibration(), calibration);
        let raster = lens.raster(1.).unwrap();
        lens.validate_raster(raster).unwrap();
        let [x, y, w, h] = raster.rect();
        for sample in &data.samples {
            let p = plate.apply(sample.distorted).unwrap();
            let expected = plate.apply(sample.undistorted).unwrap();
            let actual = lens.inverse(p).unwrap();
            assert!(
                (actual[0] - expected[0]).hypot(actual[1] - expected[1]) < 1e-7,
                "{rotation:?} at {p:?}: {actual:?} != {expected:?}"
            );
            assert!(actual[0] >= x && actual[0] <= x + w && actual[1] >= y && actual[1] <= y + h);
            let back = lens.forward(actual).unwrap();
            assert!((back[0] - p[0]).hypot(back[1] - p[1]) < 1e-7);
        }
    }
}

#[test]
fn ordinary_lenses_keep_step_one_and_folded_domains_still_fail() {
    let size = Extent::new(640, 480).unwrap();
    let plate = PlateTransform::new(size, [0., 0., 640., 480.], Rotation::None, size).unwrap();
    for radial in [[0.; 3], [0.01, 0., 0.], [-0.01, 0.0001, 0.]] {
        let c = Calibration::new(size, [537.; 2], [320., 240.], radial, [0.; 2]).unwrap();
        assert_eq!(LensWarp::new(c, plate, 1.).unwrap().inverse_step(), 1.);
    }
    for radial in [[-2., 0., 0.], [-1., -0.1, 0.]] {
        let c = Calibration::new(size, [537.; 2], [320., 240.], radial, [0.; 2]).unwrap();
        assert!(LensWarp::new(c, plate, 1.)
            .unwrap_err()
            .to_string()
            .contains("cannot certify a unique domain"));
    }
}
