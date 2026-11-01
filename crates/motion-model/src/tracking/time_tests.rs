use super::*;
use crate::tracking::{
    asset::{Asset, Cause, Coordinates, Kind, Sample, Track, Visibility, VERSION},
    plate::Extent,
};
fn asset(pts: &[i64], ticks_per_second: u32) -> Asset {
    Asset {
        version: VERSION,
        source_id: "source".into(),
        source_sha256: [1; 32],
        source_extent: Extent::new(640, 480).unwrap(),
        coordinates: Coordinates::RightDownForward,
        ticks_per_second,
        nominal_rate: Rate::new(30_000, 1001).unwrap(),
        source_pts: pts.into(),
        meters_per_world_unit: None,
        tracks: vec![Track {
            id: "point".into(),
            kind: Kind::Point,
            reference_index: None,
            samples: vec![Sample {
                source_index: 0,
                visibility: Visibility::Occluded,
                confidence: 0.,
                observation: None,
                manual_correction: true,
            }],
        }],
    }
}
fn mapping() -> Mapping {
    Mapping::new(Time::ticks(0, 1).unwrap(), 0, Speed::new(1, 1).unwrap())
}
#[test]
fn rational_project_frames_locate_original_pts_without_rounded_fps() {
    let a = asset(&[0, 1500, 3003, 4504], 90_000);
    let t = Timeline::new(&a, mapping()).unwrap();
    assert_eq!(
        t.locate(Time::frame(1, Rate::new(60, 1).unwrap())),
        Ok(Location::At(1))
    );
    assert_eq!(
        t.locate(Time::frame(1, Rate::new(30_000, 1001).unwrap())),
        Ok(Location::At(2))
    );
    assert_eq!(
        t.locate(Time::frame(1, Rate::new(24_000, 1001).unwrap())),
        Ok(Location::Between {
            before: 2,
            after: 3,
            numerator: 3003,
            denominator: 6004
        })
    );
    assert_eq!(
        t.locate(Time::frame(1, Rate::new(30, 1).unwrap())),
        Ok(Location::Between {
            before: 1,
            after: 2,
            numerator: 500,
            denominator: 501
        })
    );
}
#[test]
fn variable_source_timestamps_override_nominal_source_rate() {
    let mut a = asset(&[-3003, 0, 3004, 9009], 90_000);
    let expected = Location::Between {
        before: 1,
        after: 2,
        numerator: 3003,
        denominator: 3004,
    };
    for rate in [Rate::new(24, 1).unwrap(), Rate::new(30_000, 1001).unwrap()] {
        a.nominal_rate = rate;
        let t = Timeline::new(&a, mapping()).unwrap();
        assert_eq!(
            t.locate(Time::ticks(3004, 90_000).unwrap()),
            Ok(Location::At(2))
        );
        assert_eq!(
            t.locate(Time::frame(1, Rate::new(30_000, 1001).unwrap())),
            Ok(expected)
        );
    }
}
#[test]
fn signed_clock_origins_and_pts_above_float_integer_precision_remain_exact() {
    let base = 9_007_199_254_740_993;
    let a = asset(&[base, base + 1, base + 2], 1_000_000_000);
    let map = Mapping::new(
        Time::ticks(i64::MIN, 1_000_000_000).unwrap(),
        base,
        Speed::new(1, 1).unwrap(),
    );
    let t = Timeline::new(&a, map).unwrap();
    for offset in 0..=2 {
        assert_eq!(
            t.locate(Time::ticks(i64::MIN + offset, 1_000_000_000).unwrap()),
            Ok(Location::At(offset as u32))
        );
    }
}
#[test]
fn negative_fraction_and_outside_range_are_explicit_without_clamping() {
    let a = asset(&[-10, 0, 10], 10);
    let t = Timeline::new(&a, mapping()).unwrap();
    assert_eq!(
        t.locate(Time::ticks(-1, 4).unwrap()),
        Ok(Location::Between {
            before: 0,
            after: 1,
            numerator: 3,
            denominator: 4
        })
    );
    assert_eq!(t.locate(Time::ticks(-2, 1).unwrap()), Ok(Location::Before));
    assert_eq!(t.locate(Time::ticks(2, 1).unwrap()), Ok(Location::After));
}
#[test]
fn reverse_speed_nonzero_anchor_and_explicit_freeze_preserve_time_direction() {
    let a = asset(&[-20, 0, 20], 10);
    let reverse = Mapping::new(Time::ticks(5, 1).unwrap(), 0, Speed::new(-2, 1).unwrap());
    let t = Timeline::new(&a, reverse).unwrap();
    assert_eq!(t.locate(Time::ticks(6, 1).unwrap()), Ok(Location::At(0)));
    assert_eq!(t.locate(Time::ticks(4, 1).unwrap()), Ok(Location::At(2)));
    let frozen = Mapping::new(
        Time::frame(i64::MIN, Rate::new(1, 999_999).unwrap()),
        0,
        Speed::new(0, 1).unwrap(),
    );
    assert_eq!(
        Timeline::new(&a, frozen)
            .unwrap()
            .locate(Time::frame(i64::MAX, Rate::new(1, 1_000_000).unwrap())),
        Ok(Location::At(1))
    );
}
#[test]
fn full_signed_pts_span_uses_checked_exact_weight_arithmetic() {
    let a = asset(&[i64::MIN, 0, i64::MAX], 1_000_000_000);
    let t = Timeline::new(&a, mapping()).unwrap();
    for (index, value) in a.source_pts.iter().enumerate() {
        assert_eq!(
            t.locate(Time::ticks(*value, 1_000_000_000).unwrap()),
            Ok(Location::At(index as u32))
        );
    }
    let Location::Between {
        before,
        after,
        numerator,
        denominator,
    } = t.locate(Time::ticks(-1, 2).unwrap()).unwrap()
    else {
        panic!("exact interior bracket required");
    };
    assert_eq!((before, after), (0, 1));
    assert_eq!(
        numerator * (1u128 << 63),
        denominator * ((1u128 << 63) - 500_000_000)
    );
}
#[test]
fn invalid_metadata_and_arithmetic_overflow_preserve_distinct_causes() {
    for scale in [0, 1_000_000_001] {
        assert_eq!(Time::ticks(0, scale), Err(Error::Timebase));
    }
    for (n, d) in [(1, 0), (1, 1_000_001), (1_000_001, 1), (i32::MIN, 1)] {
        assert_eq!(Speed::new(n, d), Err(Error::Speed));
    }
    let mut a = asset(&[0, 1], 1_000_000_000);
    let huge = Mapping::new(
        Time::frame(i64::MIN, Rate::new(1, 999_999).unwrap()),
        0,
        Speed::new(1_000_000, 1).unwrap(),
    );
    assert_eq!(
        Timeline::new(&a, huge)
            .unwrap()
            .locate(Time::frame(i64::MAX, Rate::new(1, 1_000_000).unwrap())),
        Err(Error::NumericRange)
    );
    a.source_pts.reverse();
    let error = Timeline::new(&a, mapping()).err().unwrap();
    assert!(matches!(
        error,
        Error::Asset(asset::Error {
            cause: Cause::TimestampOrder,
            ..
        })
    ));
    assert!(std::error::Error::source(&error).is_some());
}
#[test]
fn lookup_never_rewrites_sparse_manual_visibility_or_source_metadata() {
    let a = asset(&[-3003, 0, 3004, 9009], 90_000);
    let original = crate::tracking::encode::to_vec(&a).unwrap();
    let t = Timeline::new(&a, mapping()).unwrap();
    for value in -100..100 {
        let _ = t.locate(Time::ticks(value, 1000).unwrap()).unwrap();
    }
    assert_eq!(crate::tracking::encode::to_vec(&a).unwrap(), original);
    assert!(a.tracks[0].samples[0].manual_correction);
    assert_eq!(a.tracks[0].samples[0].visibility, Visibility::Occluded);
}
#[test]
fn mixed_clocks_match_independent_small_integer_oracle() {
    let a = asset(
        &[-5000, -3003, -17, 0, 13, 1500, 3003, 6007, 9009, 30_000],
        90_000,
    );
    for (fps_n, fps_d) in [
        (24, 1),
        (25, 1),
        (30, 1),
        (60, 1),
        (24_000, 1001),
        (30_000, 1001),
        (60_000, 1001),
    ] {
        for (speed_n, speed_d) in [(-3, 2), (0, 1), (1, 1), (2, 3)] {
            let anchor = Time::ticks(-7, 11).unwrap();
            let map = Mapping::new(anchor, -1234, Speed::new(speed_n, speed_d).unwrap());
            let t = Timeline::new(&a, map).unwrap();
            for frame in -20..100 {
                let n = (i128::from(frame) * fps_d * 11 + 7 * fps_n) * i128::from(speed_n) * 90_000
                    - 1234 * 11 * fps_n * i128::from(speed_d);
                let d = 11 * fps_n * i128::from(speed_d);
                let time = Time::frame(frame, Rate::new(fps_n as u32, fps_d as u32).unwrap());
                let result = t.locate(time).unwrap();
                let after = a
                    .source_pts
                    .iter()
                    .position(|pts| i128::from(*pts) * d >= n);
                match (after, result) {
                    (None, Location::After) => {}
                    (Some(0), Location::Before) => assert!(n < i128::from(a.source_pts[0]) * d),
                    (Some(index), Location::At(found)) => {
                        assert_eq!(index, found as usize);
                        assert_eq!(n, i128::from(a.source_pts[index]) * d);
                    }
                    (
                        Some(index),
                        Location::Between {
                            before,
                            after,
                            numerator,
                            denominator,
                        },
                    ) => {
                        assert_eq!((before as usize, after as usize), (index - 1, index));
                        let left = i128::from(a.source_pts[index - 1]);
                        let span = i128::from(a.source_pts[index]) - left;
                        assert_eq!(
                            numerator * (d * span) as u128,
                            denominator * (n - left * d) as u128
                        );
                        assert!(numerator > 0 && numerator < denominator);
                    }
                    other => panic!("oracle mismatch frame={frame} result={other:?}"),
                }
            }
        }
    }
}
