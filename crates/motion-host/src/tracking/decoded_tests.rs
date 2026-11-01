use super::*;
use crate::tracking::{
    asset::{Asset, Coordinates, Kind, Observation, Rate, Sample, Track, Visibility, VERSION},
    plate::Extent,
};

fn solve(ticks_per_second: u32, source_pts: Vec<i64>) -> Asset {
    Asset {
        version: VERSION,
        source_id: "independent decoded clock control".into(),
        source_sha256: [3; 32],
        source_extent: Extent::new(8, 6).unwrap(),
        coordinates: Coordinates::RightDownForward,
        ticks_per_second,
        nominal_rate: Rate::new(30, 1).unwrap(),
        source_pts,
        meters_per_world_unit: None,
        tracks: vec![Track {
            id: "point".into(),
            kind: Kind::Point,
            reference_index: None,
            samples: vec![Sample {
                source_index: 0,
                visibility: Visibility::Visible,
                confidence: 1.,
                observation: Some(Observation::Point([2., 3.])),
                manual_correction: false,
            }],
        }],
    }
}

#[test]
fn signed_origin_and_rational_vfr_gaps_remain_exact() {
    let asset = solve(90000, vec![-3003, 0, 3003, 9009]);
    let mut timeline = VideoTimeline::new(&asset, 1001, 30000).unwrap();
    for pts in [-1, 0, 1, 3] {
        timeline.observe(pts, 8, 6).unwrap();
    }
    timeline.finish().unwrap();
}

#[test]
fn sub_microsecond_differences_are_not_rounded_into_agreement() {
    let asset = solve(1_000_000_000, vec![1_000_001, 1_000_002]);
    let mut timeline = VideoTimeline::new(&asset, 1, 1_000_000_000).unwrap();
    timeline.observe(1_000_001, 8, 6).unwrap();
    assert!(matches!(
        timeline.observe(1_000_003, 8, 6),
        Err(Error::Timestamp { index: 1, .. })
    ));
    assert!(timeline.finish().is_err());
}

#[test]
fn decoded_frames_cannot_be_missing_or_extra() {
    let asset = solve(30, vec![0, 1]);
    let mut missing = VideoTimeline::new(&asset, 1, 30).unwrap();
    missing.observe(0, 8, 6).unwrap();
    assert!(matches!(
        missing.finish(),
        Err(Error::Count {
            expected: 2,
            actual: 1
        })
    ));
    let mut extra = VideoTimeline::new(&asset, 1, 30).unwrap();
    for pts in [0, 1] {
        extra.observe(pts, 8, 6).unwrap();
    }
    assert!(matches!(
        extra.observe(2, 8, 6),
        Err(Error::Count {
            expected: 2,
            actual: 3
        })
    ));
    assert!(extra.finish().is_err());
}

#[test]
fn duplicate_and_out_of_order_frames_are_not_repaired() {
    let asset = solve(30, vec![0, 1, 2]);
    for pts in [0, -1, 2] {
        let mut timeline = VideoTimeline::new(&asset, 1, 30).unwrap();
        timeline.observe(0, 8, 6).unwrap();
        assert!(matches!(
            timeline.observe(pts, 8, 6),
            Err(Error::Timestamp { index: 1, .. })
        ));
    }
}

#[test]
fn a_failed_frame_permanently_rejects_the_acquisition() {
    let asset = solve(30, vec![0]);
    let mut timeline = VideoTimeline::new(&asset, 1, 30).unwrap();
    assert!(matches!(
        timeline.observe(0, 7, 6),
        Err(Error::Dimensions { index: Some(0), .. })
    ));
    assert!(timeline.observe(0, 8, 6).is_err());
    assert!(timeline.finish().is_err());
}

#[test]
fn nonpositive_native_clocks_and_invalid_assets_are_rejected() {
    let asset = solve(30, vec![0]);
    for (num, den) in [(0, 30), (-1, 30), (1, 0), (1, -30)] {
        assert!(matches!(
            VideoTimeline::new(&asset, num, den),
            Err(Error::Invalid(_))
        ));
    }
    let asset = solve(30, vec![1, 1]);
    assert!(matches!(
        VideoTimeline::new(&asset, 1, 30),
        Err(Error::Asset(_))
    ));
}

#[test]
fn extreme_signed_pts_are_compared_without_float_or_integer_overflow() {
    let asset = solve(1_000_000_000, vec![i64::MIN, i64::MAX]);
    let mut timeline = VideoTimeline::new(&asset, 1, 1_000_000_000).unwrap();
    timeline.observe(i64::MIN, 8, 6).unwrap();
    timeline.observe(i64::MAX, 8, 6).unwrap();
    timeline.finish().unwrap();
    let mut timeline = VideoTimeline::new(&asset, i32::MAX, 1).unwrap();
    assert!(matches!(
        timeline.observe(i64::MIN, 8, 6),
        Err(Error::Timestamp { .. })
    ));
}
