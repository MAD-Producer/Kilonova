use glam::{DQuat, DVec3};
use motion_model::tracking::{asset, colmap::*, decode, encode, plate, pose};

const CAMERA: &[u8] = b"# authored fixture\r\n19 PINHOLE 800 600 700 710 400 300\r\n";
const IMAGE: &[u8] = b"53 1 0 0 0 -1 -2 -3 19 frame001.png\n\n";

fn source(count: usize) -> Source {
    Source {
        id: "source-plate".into(),
        sha256: [0x37; 32],
        extent: plate::Extent::new(800, 600).unwrap(),
        ticks_per_second: 90_000,
        nominal_rate: asset::Rate::new(30_000, 1001).unwrap(),
        pts: (0..count)
            .map(|n| -12_345 + n as i64 * 3003 + (n / 2) as i64)
            .collect(),
    }
}
fn frames(count: usize) -> Vec<Frame> {
    (0..count)
        .map(|n| Frame {
            source_index: n as u32,
            image_name: format!("frame{:03}.png", n + 1),
            confidence: 0.73,
        })
        .collect()
}
fn options() -> Options {
    Options {
        track_id: "camera-solve".into(),
        world_basis: pose::Basis::RIGHT_DOWN_FORWARD,
        source_meters_per_unit: 1.,
        target_meters_per_unit: 1.,
        target_origin: [0.; 3],
    }
}
fn convert_one(cameras: &[u8], images: &[u8]) -> Result<Converted, Error> {
    convert(cameras, images, source(1), &frames(1), options())
}
fn camera(result: &Converted, index: usize) -> (asset::Pose, plate::Calibration) {
    let Some(asset::Observation::Camera {
        camera_to_world,
        calibration,
    }) = result.asset.tracks[0].samples[index].observation
    else {
        panic!("expected a registered camera observation");
    };
    (camera_to_world, calibration)
}
fn near<const N: usize>(actual: [f64; N], expected: [f64; N]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!((a - e).abs() < 1e-10, "{actual:?} != {expected:?}");
    }
}

#[test]
fn exact_original_bytes_clock_identity_and_missing_frames_survive_canonical_roundtrip() {
    let images = [
        IMAGE,
        b"# extra unmapped photograph\n999 1 0 0 0 0 0 0 19 extra.jpg\n1 2 -1 3 4 7654321\n",
    ]
    .concat();
    let input = source(3);
    let expected_pts = input.pts.clone();
    let result = convert(CAMERA, &images, input, &frames(3), options()).unwrap();
    assert_eq!(result.originals.cameras(), CAMERA);
    assert_eq!(result.originals.images(), images);
    assert_eq!(result.options.track_id, "camera-solve");
    assert_eq!(result.unmapped_images, 1);
    assert_eq!(result.asset.source_pts, expected_pts);
    assert_eq!(result.asset.nominal_rate.ratio(), [30_000, 1001]);
    assert_eq!(result.asset.source_sha256, [0x37; 32]);
    assert_eq!(result.asset.ticks_per_second, 90_000);
    assert_eq!(result.asset.meters_per_world_unit, Some(1.));
    let samples = &result.asset.tracks[0].samples;
    assert_eq!(samples[0].visibility, asset::Visibility::Visible);
    assert_eq!(samples[0].confidence, 0.73);
    assert!(!samples[0].manual_correction);
    near(camera(&result, 0).0.position, [1., 2., 3.]);
    for sample in &samples[1..] {
        assert_eq!(sample.visibility, asset::Visibility::Lost);
        assert_eq!(sample.confidence, 0.);
        assert!(sample.observation.is_none());
        assert!(!sample.manual_correction);
    }
    let bytes = encode::to_vec(&result.asset).unwrap();
    assert_eq!(decode::from_slice(&bytes).unwrap(), result.asset);
    let description = format!("{:?}", result.originals);
    assert!(!description.contains("extra.jpg"));
}

#[test]
fn hamilton_world_to_camera_is_inverted_instead_of_relabelled() {
    // Independent quarter-turn about Z: camera x=-world y, camera y=world x.
    let result = convert_one(
        CAMERA,
        b"3 0.7071067811865476 0 0 0.7071067811865476 2 -4 1 19 frame001.png\n\n",
    )
    .unwrap();
    let (pose, calibration) = camera(&result, 0);
    near(pose.position, [4., 2., -1.]);
    // Camera coordinates of world [5,0,4] are [2,1,5].
    let local = DQuat::from_array(pose.orientation_xyzw).conjugate()
        * (DVec3::new(5., 0., 4.) - DVec3::from_array(pose.position));
    near(local.to_array(), [2., 1., 5.]);
    near(calibration.project(local.to_array()).unwrap(), [680., 442.]);
}

#[test]
fn declared_world_axes_scale_and_origin_are_applied_once() {
    let mut settings = options();
    settings.world_basis = pose::Basis::RIGHT_UP_BACK;
    settings.source_meters_per_unit = 0.01;
    settings.target_meters_per_unit = 0.1;
    settings.target_origin = [10., 20., 30.];
    let result = convert(CAMERA, IMAGE, source(1), &frames(1), settings).unwrap();
    let (pose, _) = camera(&result, 0);
    near(pose.position, [10.1, 19.8, 29.7]);
    near(
        (DQuat::from_array(pose.orientation_xyzw) * DVec3::Z).to_array(),
        [0., 0., -1.],
    );
    assert_eq!(result.asset.meters_per_world_unit, Some(0.1));
    assert_eq!(result.options.world_basis, pose::Basis::RIGHT_UP_BACK);
}

#[test]
fn five_supported_models_preserve_pixel_origin_and_all_declared_coefficients() {
    // Hand-computed projections of normalized [x,y]=[0.2,-0.1].
    let cases = [
        ("SIMPLE_PINHOLE 800 600 700 400 300", [540., 230.]),
        ("PINHOLE 800 600 700 710 400 300", [540., 229.]),
        ("SIMPLE_RADIAL 800 600 700 400 300 0.1", [540.7, 229.65]),
        ("RADIAL 800 600 700 400 300 0.1 -0.02", [540.693, 229.6535]),
        (
            "OPENCV 800 600 700 710 400 300 0.1 -0.02 0.003 -0.004",
            [540.245, 228.91125],
        ),
    ];
    for (model, expected) in cases {
        let document = format!("19 {model}\n");
        let result = convert_one(document.as_bytes(), IMAGE).unwrap();
        let calibration = camera(&result, 0).1;
        near(calibration.principal(), [400., 300.]);
        near(calibration.project([2., -1., 10.]).unwrap(), expected);
    }
}

#[test]
fn names_are_exact_case_sensitive_mappings_not_filename_or_fps_inference() {
    let result = convert_one(CAMERA, b"0 1 0 0 0 0 0 0 19 Frame001.png\n\n").unwrap();
    assert_eq!(result.unmapped_images, 1);
    assert_eq!(
        result.asset.tracks[0].samples[0].visibility,
        asset::Visibility::Lost
    );
    assert_eq!(result.asset.source_pts, [-12_345]);
}

#[test]
fn malformed_camera_rows_fail_at_the_original_input_location() {
    let cases = [
        "19 PINHOLE 800 600 700 710 400",       // truncated
        "19 PINHOLE 800 600 700 710 400 300 1", // extra coefficient
        "19 PINHOLE 0 600 700 710 400 300",
        "19 PINHOLE 800 600 -700 710 400 300",
        "19 PINHOLE 800 600 NaN 710 400 300",
        "19 PINHOLE 800 600 700 710 inf 300",
        "19 OPENCV_FISHEYE 800 600 700 710 400 300 0 0 0 0",
        "19 FULL_OPENCV 800 600 700 710 400 300 0 0 0 0 0 0 0 0",
        "-1 PINHOLE 800 600 700 710 400 300",
    ];
    for line in cases {
        let error = convert_one(format!("# header\n{line}\n").as_bytes(), IMAGE).unwrap_err();
        assert_eq!(error.input, Some(Input::Cameras), "{line}");
        assert_eq!(error.line, Some(2), "{line}");
    }
    let duplicate = [CAMERA, CAMERA].concat();
    assert!(convert_one(&duplicate, IMAGE)
        .unwrap_err()
        .to_string()
        .contains("duplicate camera"));
}

#[test]
fn malformed_image_records_never_produce_a_partial_or_repaired_solve() {
    let cases = [
        "53 1 0 0 0 -1 -2 -3 19 frame001.png\n", // absent keypoint line
        "53 1 0 0 0 -1 -2 -3 19 frame001.png\n1 2\n",
        "53 1 0 0 0 -1 -2 -3 19 frame001.png\nNaN 2 -1\n",
        "53 1 0 0 0 -1 -2 -3 19 frame001.png\n1 2 -2\n",
        "53 2 0 0 0 -1 -2 -3 19 frame001.png\n\n", // nonunit quaternion
        "53 1 0 0 0 -1 -2 -3 999 frame001.png\n\n", // dangling camera
        "53 1 0 0 0 -1 -2 -3 19 frame001.png extra\n\n",
        "53 1 0 0 0 inf -2 -3 19 frame001.png\n\n",
    ];
    for document in cases {
        let error = convert_one(CAMERA, document.as_bytes()).unwrap_err();
        assert_eq!(error.input, Some(Input::Images), "{document}");
        assert!(error.line.is_some());
    }
    let duplicate = [IMAGE, IMAGE].concat();
    assert!(convert_one(CAMERA, &duplicate)
        .unwrap_err()
        .to_string()
        .contains("duplicate image identifier"));
    let duplicate_name = [IMAGE, b"72 1 0 0 0 0 0 0 19 frame001.png\n\n"].concat();
    assert!(convert_one(CAMERA, &duplicate_name)
        .unwrap_err()
        .to_string()
        .contains("duplicate registered image name"));
}

#[test]
fn extent_mismatch_retains_frame_line_and_underlying_pose_cause_is_typed() {
    let error = convert_one(b"19 PINHOLE 801 600 700 710 400 300\n", IMAGE).unwrap_err();
    assert_eq!(
        (error.input, error.line, error.frame),
        (Some(Input::Images), Some(1), Some(0))
    );
    let error = convert_one(CAMERA, b"53 0 0 0 0 0 0 0 19 frame001.png\n\n").unwrap_err();
    assert!(matches!(
        error.cause,
        Cause::Pose(asset::Error {
            cause: asset::Cause::Quaternion,
            ..
        })
    ));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn invalid_mapping_timebase_units_and_reflections_are_rejected_without_sorting() {
    let mut mapping = frames(2);
    mapping[1].source_index = 0;
    assert_eq!(
        convert(CAMERA, IMAGE, source(2), &mapping, options())
            .unwrap_err()
            .frame,
        Some(0)
    );
    mapping = frames(2);
    mapping[1].image_name = mapping[0].image_name.clone();
    assert!(convert(CAMERA, IMAGE, source(2), &mapping, options()).is_err());
    for confidence in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        let mut mapping = frames(1);
        mapping[0].confidence = confidence;
        assert!(convert(CAMERA, IMAGE, source(1), &mapping, options()).is_err());
    }
    let mut clock = source(2);
    clock.pts.reverse();
    assert!(matches!(
        convert(CAMERA, IMAGE, clock, &frames(2), options())
            .unwrap_err()
            .cause,
        Cause::Asset(_)
    ));
    let mut clock = source(1);
    clock.ticks_per_second = 0;
    assert!(convert(CAMERA, IMAGE, clock, &frames(1), options()).is_err());
    let mut settings = options();
    settings.source_meters_per_unit = 0.;
    assert!(matches!(
        convert(CAMERA, IMAGE, source(1), &frames(1), settings)
            .unwrap_err()
            .cause,
        Cause::Conversion(pose::Error::Units)
    ));
    let mut settings = options();
    settings.world_basis = pose::Basis::RIGHT_UP_FORWARD;
    assert!(matches!(
        convert(CAMERA, IMAGE, source(1), &frames(1), settings)
            .unwrap_err()
            .cause,
        Cause::Conversion(pose::Error::Handedness)
    ));
}

#[test]
fn bounded_documents_and_names_do_not_leak_payloads_in_errors() {
    let huge = vec![b' '; MAX_TEXT_BYTES + 1];
    assert!(convert_one(&huge, IMAGE)
        .unwrap_err()
        .to_string()
        .contains("byte limit"));
    let long_line = vec![b'x'; MAX_LINE_BYTES + 1];
    assert!(convert_one(&long_line, IMAGE).is_err());
    assert!(convert_one(&[0xff], IMAGE).is_err());
    let secret = "token=not-for-logs";
    let document = format!("19 {secret} 800 600 1 1 1 1\n");
    assert!(!convert_one(document.as_bytes(), IMAGE)
        .unwrap_err()
        .to_string()
        .contains(secret));
    let mut mapping = frames(1);
    mapping[0].image_name = "x".repeat(1025);
    assert!(convert(CAMERA, IMAGE, source(1), &mapping, options()).is_err());
    assert!(convert(CAMERA, IMAGE, source(1), &[], options()).is_err());
}
