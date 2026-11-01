use super::super::{asset::Cause, plate::Extent};
use super::*;
use std::error::Error as _;

fn close(a: [f64; 3], b: [f64; 3]) {
    for (a, b) in a.into_iter().zip(b) {
        assert!((a - b).abs() < 1e-10, "{a} != {b}");
    }
}
fn identity() -> Pose {
    Pose {
        position: [0.; 3],
        orientation_xyzw: [0., 0., 0., 1.],
    }
}
fn bases() -> Vec<Basis> {
    let mut result = Vec::new();
    for x in 1i8..=3 {
        for y in 1i8..=3 {
            for z in 1i8..=3 {
                for signs in 0..8 {
                    let rows = [x, y, z].map(|axis| {
                        if signs & (1 << (axis - 1)) == 0 {
                            axis
                        } else {
                            -axis
                        }
                    });
                    if let Ok(basis) = Basis::new(rows) {
                        result.push(basis);
                    }
                }
            }
        }
    }
    result
}
fn selected(basis: Basis, point: [f64; 3]) -> [f64; 3] {
    basis
        .rows()
        .map(|component| point[(component.unsigned_abs() - 1) as usize] * component.signum() as f64)
}
#[test]
fn signed_axis_bases_validate_and_invert_all_48_conventions() {
    let bases = bases();
    assert_eq!(bases.len(), 48);
    for basis in bases {
        let point = [2., 3., 5.];
        assert_eq!(selected(basis.inverse(), selected(basis, point)), point);
        assert_eq!(basis.inverse().inverse(), basis);
        close(
            (basis.matrix() * DVec3::from_array(point)).to_array(),
            selected(basis, point),
        );
    }
    for rows in [[0, 2, 3], [1, 1, 3], [1, -1, 2], [4, 2, 3], [-128, 2, 3]] {
        assert!(matches!(Basis::new(rows), Err(Error::Basis)));
    }
}
#[test]
fn documented_right_up_back_mapping_converts_units_origin_and_quarter_turn() {
    let convert = Conversion::new(
        Basis::RIGHT_UP_BACK,
        Basis::RIGHT_UP_BACK,
        0.01,
        0.001,
        [960., 540., 0.],
    )
    .unwrap();
    close(convert.point([1., 2., 3.]).unwrap(), [970., 520., -30.]);
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let source = Pose {
        position: [1., 2., 3.],
        orientation_xyzw: [0., 0., s, s],
    };
    let result = convert.pose(source).unwrap();
    close(result.position, [970., 520., -30.]);
    let q = DQuat::from_array(result.orientation_xyzw);
    close((q * DVec3::X).to_array(), [0., -1., 0.]);
    close((q * DVec3::Y).to_array(), [1., 0., 0.]);
    close((q * DVec3::Z).to_array(), [0., 0., 1.]);
    assert_eq!(source.orientation_xyzw, [0., 0., s, s]);
}
#[test]
fn independent_world_and_camera_bases_do_not_assume_the_same_up_axis() {
    let world = Basis::new([1, -3, 2]).unwrap();
    let convert = Conversion::new(world, Basis::RIGHT_UP_BACK, 1., 1., [0.; 3]).unwrap();
    let camera = convert.pose(identity()).unwrap();
    close(camera_point(camera, [1., 5., 2.]).unwrap(), [1., -2., 5.]);
    // A world Z-up source point [1,2,-5] becomes [1,5,2]. Camera local
    // +X,+Y,+Z become +X,-Y,-Z, independently of the source world's up axis.
    close(convert.point([1., 2., -5.]).unwrap(), [1., 5., 2.]);
}
#[test]
fn reflection_requires_consistent_world_and_camera_handedness() {
    assert!(matches!(
        Conversion::new(
            Basis::RIGHT_UP_FORWARD,
            Basis::RIGHT_DOWN_FORWARD,
            1.,
            1.,
            [0.; 3]
        ),
        Err(Error::Handedness)
    ));
    let convert = Conversion::new(
        Basis::RIGHT_UP_FORWARD,
        Basis::RIGHT_UP_FORWARD,
        1.,
        1.,
        [0.; 3],
    )
    .unwrap();
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let camera = convert
        .pose(Pose {
            orientation_xyzw: [0., 0., s, s],
            ..identity()
        })
        .unwrap();
    close(
        (DQuat::from_array(camera.orientation_xyzw) * DVec3::X).to_array(),
        [0., -1., 0.],
    );
    close(convert.point([1., 2., 3.]).unwrap(), [1., -2., 3.]);
}
#[test]
fn every_valid_basis_pair_preserves_known_camera_rays_and_inverse_pose() {
    let rotation = DQuat::from_rotation_y(0.7) * DQuat::from_rotation_z(-0.4);
    let source = Pose {
        position: [2., -4., 3.],
        orientation_xyzw: rotation.to_array(),
    };
    let ray = [0.25, -0.5, 2.];
    let source_world = DVec3::from_array(source.position) + rotation * DVec3::from_array(ray);
    let mut combinations = 0;
    for world in bases() {
        for camera in bases() {
            let Ok(convert) = Conversion::new(world, camera, 0.01, 0.1, [7., 11., -13.]) else {
                continue;
            };
            combinations += 1;
            let converted_pose = convert.pose(source).unwrap();
            let converted_point = convert.point(source_world.to_array()).unwrap();
            close(
                camera_point(converted_pose, converted_point).unwrap(),
                selected(camera, ray).map(|v| v * 0.1),
            );
            let inverse = convert.inverse().unwrap();
            close(
                inverse.point(converted_point).unwrap(),
                source_world.to_array(),
            );
            let restored_pose = inverse.pose(converted_pose).unwrap();
            close(restored_pose.position, source.position);
            let restored = DQuat::from_array(restored_pose.orientation_xyzw);
            for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
                close((restored * axis).to_array(), (rotation * axis).to_array());
            }
        }
    }
    assert_eq!(combinations, 1152);
}
#[test]
fn world_reprojection_has_an_independent_distorted_pixel_golden() {
    let camera = Pose {
        position: [2., 3., 4.],
        orientation_xyzw: [0., 0., 0., 1.],
    };
    let calibration = Calibration::new(
        Extent::new(640, 480).unwrap(),
        [400., 410.],
        [320., 240.],
        [0.1, 0.02, 0.003],
        [0.004, -0.005],
    )
    .unwrap();
    // World point minus eye -> [1,2,5], normalized [.2,.4], r^2=.2.
    // Brown-Conrady factor=1+.1*.2+.02*.04+.003*.008=1.020824.
    // Tangential X=.00064-.0014=-.00076, Y=.00208-.0008=.00128.
    let golden = [
        400. * (0.2 * 1.020824 - 0.00076) + 320.,
        410. * (0.4 * 1.020824 + 0.00128) + 240.,
    ];
    assert!(world_residual(camera, calibration, [3., 5., 9.], golden).unwrap() < 1e-10);
    assert!(
        (world_residual(
            camera,
            calibration,
            [3., 5., 9.],
            [golden[0] + 3., golden[1] + 4.]
        )
        .unwrap()
            - 5.)
            .abs()
            < 1e-10
    );
}
#[test]
fn converted_external_camera_and_plate_coordinates_reproject_consistently() {
    let convert = Conversion::new(
        Basis::RIGHT_UP_BACK,
        Basis::RIGHT_UP_BACK,
        0.01,
        1.,
        [100., 200., 300.],
    )
    .unwrap();
    let camera = convert
        .pose(Pose {
            position: [10., 20., 30.],
            ..identity()
        })
        .unwrap();
    let point = convert.point([20., 0., -20.]).unwrap();
    // Source camera sees [10,-20,-50]; canonical normalized ray is [.2,.4,1].
    let calibration = Calibration::new(
        Extent::new(640, 480).unwrap(),
        [400., 400.],
        [320., 240.],
        [0.; 3],
        [0.; 2],
    )
    .unwrap();
    assert!(world_residual(camera, calibration, point, [400., 400.]).unwrap() < 1e-9);
    let transform = super::super::plate::PlateTransform::new(
        Extent::new(640, 480).unwrap(),
        [100., 100., 400., 300.],
        super::super::plate::Rotation::Clockwise90,
        Extent::new(600, 800).unwrap(),
    )
    .unwrap();
    let output = transform.apply([400., 400.]).unwrap();
    let observed_source = transform.inverse(output).unwrap();
    assert!(world_residual(camera, calibration, point, observed_source).unwrap() < 1e-9);
}
#[test]
fn invalid_units_origins_points_and_overflows_are_explicit_errors() {
    for unit in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(matches!(
            Conversion::new(
                Basis::RIGHT_DOWN_FORWARD,
                Basis::RIGHT_DOWN_FORWARD,
                unit,
                1.,
                [0.; 3]
            ),
            Err(Error::Units)
        ));
        assert!(matches!(
            Conversion::new(
                Basis::RIGHT_DOWN_FORWARD,
                Basis::RIGHT_DOWN_FORWARD,
                1.,
                unit,
                [0.; 3]
            ),
            Err(Error::Units)
        ));
    }
    assert!(matches!(
        Conversion::new(
            Basis::RIGHT_DOWN_FORWARD,
            Basis::RIGHT_DOWN_FORWARD,
            1.,
            1.,
            [f64::NAN, 0., 0.]
        ),
        Err(Error::NonFinitePoint)
    ));
    assert!(matches!(
        Conversion::new(
            Basis::RIGHT_DOWN_FORWARD,
            Basis::RIGHT_DOWN_FORWARD,
            f64::MAX,
            f64::MIN_POSITIVE,
            [0.; 3]
        ),
        Err(Error::NumericRange)
    ));
    let c = Conversion::new(
        Basis::RIGHT_DOWN_FORWARD,
        Basis::RIGHT_DOWN_FORWARD,
        2.,
        1.,
        [0.; 3],
    )
    .unwrap();
    assert!(matches!(
        c.point([f64::MAX, 0., 0.]),
        Err(Error::NumericRange)
    ));
    assert!(matches!(
        c.point([f64::NAN, 0., 0.]),
        Err(Error::NonFinitePoint)
    ));
    assert!(matches!(
        camera_point(identity(), [f64::NAN, 0., 0.]),
        Err(Error::NonFinitePoint)
    ));
    let tiny = Conversion::new(
        Basis::RIGHT_DOWN_FORWARD,
        Basis::RIGHT_DOWN_FORWARD,
        f64::from_bits(1),
        1.,
        [0.; 3],
    )
    .unwrap();
    assert!(matches!(tiny.inverse(), Err(Error::NumericRange)));
}
#[test]
fn invalid_pose_and_visibility_causes_are_not_coerced_into_success() {
    let c = Conversion::new(
        Basis::RIGHT_DOWN_FORWARD,
        Basis::RIGHT_DOWN_FORWARD,
        1.,
        1.,
        [0.; 3],
    )
    .unwrap();
    let error = c
        .pose(Pose {
            orientation_xyzw: [0.; 4],
            ..identity()
        })
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Pose(asset::Error {
            cause: Cause::Quaternion,
            ..
        })
    ));
    assert!(error.source().is_some());
    let error = c
        .pose(Pose {
            position: [f64::INFINITY, 0., 0.],
            ..identity()
        })
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Pose(asset::Error {
            cause: Cause::NonFinite,
            ..
        })
    ));
    let calibration = Calibration::new(
        Extent::new(640, 480).unwrap(),
        [400.; 2],
        [320., 240.],
        [0.; 3],
        [0.; 2],
    )
    .unwrap();
    let error = world_residual(identity(), calibration, [0., 0., -1.], [0.; 2]).unwrap_err();
    assert!(matches!(
        error,
        Error::Projection(plate::Error::BehindCamera)
    ));
    assert!(error.source().is_some());
}
