use motion_model::depth::*;

fn sample(start: u64, end: u64, asset: u64) -> VideoDepthSample {
    let resource = DepthResource::new(
        DepthDescriptor {
            size: [1, 1],
            source: DepthSource {
                asset,
                revision: 1,
                pts: DepthTime {
                    ticks: start as i64,
                    timescale: 1_000_000,
                },
            },
            geometry: DepthGeometry {
                source_size: [1, 1],
                crop: [0, 0, 1, 1],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "indexed".into(),
                revision: "1".into(),
                sha256: [1; 32],
                processing: "test".into(),
            },
            measure: DepthMeasure::Relative {
                near: NearDirection::Increasing,
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        },
        vec![1.],
        None,
        None,
    )
    .unwrap();
    let calibration = SavedDepthCalibration::new(
        &resource,
        PreparedDepthInput {
            rgb_size: [1, 1],
            rgb_sha256: [1; 32],
        },
        [
            DepthAnchor {
                value: 1.,
                camera_z_meters: 8.,
            },
            DepthAnchor {
                value: 2.,
                camera_z_meters: 4.,
            },
        ],
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
        20.,
    )
    .unwrap();
    VideoDepthSample::new(calibration, end).unwrap()
}

#[test]
fn intervals_are_half_open_and_gaps_never_interpolate_relative_calibration() {
    let mut track = VideoDepthTrack::default();
    track.upsert(sample(40_000, 110_000, 74)).unwrap();
    track.upsert(sample(10_000, 40_000, 74)).unwrap();
    track.upsert(sample(120_000, 150_000, 74)).unwrap();
    assert!(track.at(9_999).is_none());
    assert_eq!(track.at(10_000).unwrap().start_us(), 10_000);
    assert_eq!(track.at(39_999).unwrap().start_us(), 10_000);
    assert_eq!(track.at(40_000).unwrap().start_us(), 40_000);
    assert!(track.at(110_000).is_none());
    assert!(track.at(119_999).is_none());
    assert_eq!(track.at(149_999).unwrap().end_us(), 150_000);
    assert!(track.at(150_000).is_none());
    let json = serde_json::to_string(&track).unwrap();
    assert_eq!(
        serde_json::from_str::<VideoDepthTrack>(&json).unwrap(),
        track
    );
}

#[test]
fn invalid_upserts_and_metadata_refusals_preserve_the_prior_track() {
    let mut track = VideoDepthTrack::default();
    track.upsert(sample(10, 20, 74)).unwrap();
    track.upsert(sample(30, 40, 74)).unwrap();
    let before = track.clone();
    for bad in [sample(15, 25, 74), sample(10, 35, 74), sample(40, 50, 75)] {
        assert!(track.upsert(bad).is_err());
        assert_eq!(track, before);
    }
    assert!(VideoDepthSample::new(sample(10, 20, 74).calibration().clone(), 10).is_err());
    assert!(
        VideoDepthSample::new(sample(10, 20, 74).calibration().clone(), 3_600_000_001).is_err()
    );
    let mut json = serde_json::to_value(sample(10, 20, 74)).unwrap();
    json["calibration"]["pts"]["timescale"] = 1.into();
    assert!(serde_json::from_value::<VideoDepthSample>(json).is_err());
}

#[test]
fn deserialization_rejects_overlaps_out_of_order_and_unknown_fields() {
    for entries in [
        vec![sample(10, 20, 74), sample(15, 25, 74)],
        vec![sample(30, 40, 74), sample(10, 20, 74)],
    ] {
        assert!(
            serde_json::from_value::<VideoDepthTrack>(serde_json::to_value(entries).unwrap())
                .is_err()
        );
    }
    let mut json = serde_json::to_value(sample(10, 20, 74)).unwrap();
    json["unexpected"] = true.into();
    assert!(serde_json::from_value::<VideoDepthSample>(json).is_err());
}

#[test]
fn sample_limit_is_bounded_in_authoring_and_deserialization_and_allows_replacement() {
    let mut track = VideoDepthTrack::default();
    for index in 0..MAX_VIDEO_DEPTH_SAMPLES as u64 {
        track.upsert(sample(index * 2, index * 2 + 1, 74)).unwrap();
    }
    let before = track.clone();
    assert!(track.upsert(sample(20_000, 20_001, 74)).is_err());
    assert_eq!(track, before);
    track.upsert(sample(0, 1, 74)).unwrap();
    assert_eq!(track, before);
    let mut json = serde_json::to_value(&track).unwrap();
    json.as_array_mut()
        .unwrap()
        .push(serde_json::to_value(sample(20_000, 20_001, 74)).unwrap());
    assert!(serde_json::from_value::<VideoDepthTrack>(json).is_err());
}
