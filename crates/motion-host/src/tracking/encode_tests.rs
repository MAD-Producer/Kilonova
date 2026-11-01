use super::super::{decode, plate::Extent};
use super::*;
use std::error::Error as _;

fn sample(index: u32, observation: Observation) -> Sample {
    Sample {
        source_index: index,
        visibility: Visibility::Visible,
        confidence: 0.8123456789012345,
        observation: Some(observation),
        manual_correction: true,
    }
}
fn fixture() -> Asset {
    let source_extent = Extent::new(1920, 1080).unwrap();
    let calibration = Calibration::new(
        source_extent,
        [1123.4567890123457, 1197.5135724680135],
        [960.125, 540.375],
        [0.012345678901234567, -0.00001568391579135792, 0.0],
        [-0.0, 0.0002718281828459045],
    )
    .unwrap();
    Asset {
        version: VERSION,
        source_id: "plate-中文".into(),
        source_sha256: std::array::from_fn(|i| i as u8),
        source_extent,
        coordinates: Coordinates::RightDownForward,
        ticks_per_second: 90000,
        nominal_rate: Rate::new(30000, 1001).unwrap(),
        source_pts: vec![-9007199254740993, -3003, 0, 3004, 9007199254740993],
        meters_per_world_unit: Some(0.01),
        tracks: vec![
            Track {
                id: "point".into(),
                kind: Kind::Point,
                reference_index: None,
                samples: vec![
                    sample(0, Observation::Point([-0.0, 0.12345678901234568])),
                    Sample {
                        source_index: 3,
                        visibility: Visibility::Lost,
                        confidence: 0.0,
                        observation: None,
                        manual_correction: false,
                    },
                ],
            },
            Track {
                id: "plane".into(),
                kind: Kind::Plane,
                reference_index: Some(1),
                samples: vec![sample(
                    2,
                    Observation::Plane([[2.0, 0.0, 10.0], [0.0, 2.0, -4.0], [0.0, 0.0, 2.0]]),
                )],
            },
            Track {
                id: "camera".into(),
                kind: Kind::Camera,
                reference_index: None,
                samples: vec![
                    sample(
                        1,
                        Observation::Camera {
                            camera_to_world: Pose {
                                position: [
                                    0.12345678901234568,
                                    -2.345678901234567,
                                    10.000000000000002,
                                ],
                                orientation_xyzw: [0.0, 0.0, 0.6, 0.8],
                            },
                            calibration,
                        },
                    ),
                    Sample {
                        source_index: 4,
                        visibility: Visibility::Occluded,
                        ..sample(
                            4,
                            Observation::Camera {
                                camera_to_world: Pose {
                                    position: [1.0, 2.0, 3.0],
                                    orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
                                },
                                calibration: Calibration::new(
                                    source_extent,
                                    [1200.0, 1250.0],
                                    [961.0, 541.0],
                                    [0.0; 3],
                                    [0.0; 2],
                                )
                                .unwrap(),
                            },
                        )
                    },
                ],
            },
        ],
    }
}
fn same_floats<const N: usize>(a: [f64; N], b: [f64; N]) {
    assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits));
}
#[test]
fn real_json_roundtrip_keeps_all_kinds_exact_time_and_editing_metadata() {
    let original = fixture();
    let bytes = to_vec(&original).unwrap();
    let result = decode::from_slice(&bytes).unwrap();
    assert_eq!(result.source_id, original.source_id);
    assert_eq!(result.source_sha256, original.source_sha256);
    assert_eq!(result.source_extent, original.source_extent);
    assert_eq!(result.coordinates, original.coordinates);
    assert_eq!(result.ticks_per_second, original.ticks_per_second);
    assert_eq!(result.nominal_rate, original.nominal_rate);
    assert_eq!(result.source_pts, original.source_pts);
    assert_eq!(result.meters_per_world_unit, original.meters_per_world_unit);
    assert_eq!(result.tracks.len(), original.tracks.len());
    for (a, b) in original.tracks.iter().zip(&result.tracks) {
        assert_eq!(
            (&a.id, a.kind, a.reference_index),
            (&b.id, b.kind, b.reference_index)
        );
        assert_eq!(a.samples.len(), b.samples.len());
        for (a, b) in a.samples.iter().zip(&b.samples) {
            assert_eq!(
                (a.source_index, a.visibility, a.manual_correction),
                (b.source_index, b.visibility, b.manual_correction)
            );
            assert_eq!(a.confidence.to_bits(), b.confidence.to_bits());
            match (a.observation, b.observation) {
                (None, None) => {}
                (Some(Observation::Point(a)), Some(Observation::Point(b))) => same_floats(a, b),
                (Some(Observation::Plane(a)), Some(Observation::Plane(b))) => {
                    for (a, b) in a.into_iter().zip(b) {
                        same_floats(a, b);
                    }
                }
                (
                    Some(Observation::Camera {
                        camera_to_world: a,
                        calibration: ac,
                    }),
                    Some(Observation::Camera {
                        camera_to_world: b,
                        calibration: bc,
                    }),
                ) => {
                    same_floats(a.position, b.position);
                    same_floats(a.orientation_xyzw, b.orientation_xyzw);
                    assert_eq!(ac.source(), bc.source());
                    same_floats(ac.focal(), bc.focal());
                    same_floats(ac.principal(), bc.principal());
                    same_floats(ac.radial(), bc.radial());
                    same_floats(ac.tangential(), bc.tangential());
                }
                _ => panic!("observation kind changed"),
            }
        }
    }
    // Stable output also detects mutation or omitted/extra fields on readback.
    assert_eq!(bytes, to_vec(&result).unwrap());
}
#[test]
fn invalid_assets_are_rejected_before_json_can_turn_non_finite_values_into_null() {
    let mut a = fixture();
    a.tracks[0].samples[0].confidence = f64::NAN;
    let error = to_vec(&a).unwrap_err();
    assert!(matches!(
        error,
        Error::InvalidAsset(asset::Error {
            cause: Cause::Confidence,
            track: Some(0),
            sample: Some(0)
        })
    ));
    assert!(error.source().is_some());
    assert!(a.tracks[0].samples[0].confidence.is_nan());
    let mut a = fixture();
    a.version += 1;
    assert!(matches!(
        to_vec(&a),
        Err(Error::InvalidAsset(asset::Error {
            cause: Cause::Version,
            ..
        }))
    ));
}
#[test]
fn finite_point_values_roundtrip_without_losing_binary64_bits() {
    let mut values = vec![
        0.0,
        -0.0,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        -f64::from_bits(1),
        0.1,
        1.0000000000000002,
    ];
    let mut bits = 0x6d6f74696f6e3838u64;
    for _ in 0..2048 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        let value = f64::from_bits(bits);
        if value.is_finite() {
            values.push(value);
        }
    }
    let mut a = fixture();
    a.tracks.truncate(1);
    a.source_pts = (0..values.len() as i64).collect();
    a.tracks[0].samples = values
        .iter()
        .enumerate()
        .map(|(i, &value)| sample(i as u32, Observation::Point([value, -value])))
        .collect();
    let result = decode::from_slice(&to_vec(&a).unwrap()).unwrap();
    for (&value, sample) in values.iter().zip(&result.tracks[0].samples) {
        let Some(Observation::Point(point)) = sample.observation else {
            panic!("point missing")
        };
        same_floats([value, -value], point);
    }
}
#[test]
fn bounded_output_never_returns_truncated_json_or_writes_beyond_limit() {
    let mut out = Output {
        bytes: Vec::new(),
        limit: 3,
        exceeded: false,
    };
    out.write_all(b"abc").unwrap();
    assert!(out.write_all(b"d").is_err());
    assert_eq!(out.bytes, b"abc");
    assert!(out.exceeded);
    assert_eq!(bounded_json(&"a", 3).unwrap(), b"\"a\"");
    assert!(matches!(bounded_json(&"ab", 3), Err(Error::OutputLimit)));
    let mut a = fixture();
    a.tracks[0].samples = (0..MAX_OBSERVATIONS)
        .map(|i| sample(i as u32, Observation::Point([0.5, 0.5])))
        .collect();
    a.source_pts = (0..MAX_OBSERVATIONS as i64).collect();
    // Keep the other track indices valid; the aggregate typed limit is separate.
    a.tracks.truncate(1);
    a.validate().unwrap();
    assert!(matches!(to_vec(&a), Err(Error::OutputLimit)));
}
