use motion_model::{ClockRounding::*, FrameRate, RateError};

#[test]
fn canonical_rates_have_exact_identity_and_strict_validated_wire_format() {
    let rate = FrameRate::new(48_000_000, 2_002_000).unwrap();
    assert_eq!(rate.ratio(), [24_000, 1001]);
    assert_eq!(rate, FrameRate::new(24_000, 1001).unwrap());
    assert_ne!(
        FrameRate::new(2997, 100).unwrap(),
        FrameRate::new(30_000, 1001).unwrap()
    );
    assert_eq!(
        serde_json::to_value(rate).unwrap(),
        serde_json::json!({"numerator":24000,"denominator":1001})
    );
    assert_eq!(
        serde_json::from_str::<FrameRate>(r#"{"numerator":48000,"denominator":2002}"#).unwrap(),
        rate
    );
    for invalid in [
        r#"{"numerator":0,"denominator":1}"#,
        r#"{"numerator":24}"#,
        r#"{"numerator":24,"denominator":0}"#,
        r#"{"numerator":24,"denominator":1,"fps":24}"#,
        r#"{"numerator":24.5,"denominator":1}"#,
        "24",
        "null",
    ] {
        assert!(
            serde_json::from_str::<FrameRate>(invalid).is_err(),
            "{invalid}"
        );
    }
    assert_eq!(FrameRate::new(1_000_001, 1), Err(RateError::ComponentRange));
    assert_eq!(FrameRate::new(1, 1_000_001), Err(RateError::ComponentRange));
    assert_eq!(FrameRate::new(u32::MAX, u32::MAX).unwrap().ratio(), [1, 1]);
}

#[test]
fn integer_project_boundaries_preserve_existing_direct_time_values() {
    for fps in [1, 24, 25, 30, 48, 60, 120, 240] {
        let rate = FrameRate::integer(fps).unwrap();
        for frame in [-100_000_i64, -25, -1, 0, 1, 25, 100_000] {
            let numerator = i128::from(frame) * 48_000;
            assert_eq!(
                rate.ticks_at_frame(frame, 48_000, Floor).unwrap(),
                numerator.div_euclid(i128::from(fps)) as i64
            );
            assert_eq!(
                rate.rescale_frame(frame, rate, NearestTiesAwayFromZero)
                    .unwrap(),
                frame
            );
        }
    }
}

#[test]
fn ntsc_rates_map_long_sequences_and_audio_without_accumulated_drift() {
    for numerator in [24_000_u32, 30_000, 60_000] {
        let rate = FrameRate::new(numerator, 1001).unwrap();
        for hours in [1_i64, 24, 240, 24_000] {
            let frames = i64::from(numerator) * 3600 * hours;
            let expected_samples = 1001 * 3600 * hours * 48_000;
            assert_eq!(
                rate.ticks_at_frame(frames, 48_000, Floor).unwrap(),
                expected_samples
            );
            assert_eq!(
                rate.frame_at_tick(expected_samples, 48_000, Floor).unwrap(),
                frames
            );
        }
    }
    let rate = FrameRate::new(30_000, 1001).unwrap();
    assert_eq!(rate.ticks_at_frame(1, 48_000, Floor).unwrap(), 1601);
    assert_eq!(rate.ticks_at_frame(1, 48_000, Ceil).unwrap(), 1602);
    assert_eq!(rate.ticks_at_frame(5, 48_000, Floor).unwrap(), 8008);
}

#[test]
fn signed_rounding_and_cross_rate_clip_boundaries_are_explicit() {
    let two = FrameRate::integer(2).unwrap();
    for (frame, floor, ceil, nearest) in [
        (-3, -2, -1, -2),
        (-1, -1, 0, -1),
        (0, 0, 0, 0),
        (1, 0, 1, 1),
        (3, 1, 2, 2),
    ] {
        assert_eq!(two.ticks_at_frame(frame, 1, Floor).unwrap(), floor);
        assert_eq!(two.ticks_at_frame(frame, 1, Ceil).unwrap(), ceil);
        assert_eq!(
            two.ticks_at_frame(frame, 1, NearestTiesAwayFromZero)
                .unwrap(),
            nearest
        );
    }
    let parent = FrameRate::new(24_000, 1001).unwrap();
    let child = FrameRate::new(30_000, 1001).unwrap();
    assert_eq!(parent.rescale_frame(4, child, Floor).unwrap(), 5);
    assert_eq!(parent.rescale_frame(-1, child, Floor).unwrap(), -2);
    assert_eq!(parent.rescale_frame(-1, child, Ceil).unwrap(), -1);
}

#[test]
fn invalid_timebases_and_overflow_are_errors_instead_of_saturation() {
    let rate = FrameRate::integer(1).unwrap();
    for ticks in [0, 1_000_000_001] {
        assert_eq!(
            rate.ticks_at_frame(0, ticks, Floor),
            Err(RateError::Timebase)
        );
        assert_eq!(
            rate.frame_at_tick(0, ticks, Floor),
            Err(RateError::Timebase)
        );
    }
    assert_eq!(rate.ticks_at_frame(i64::MAX, 1, Floor).unwrap(), i64::MAX);
    assert_eq!(rate.ticks_at_frame(i64::MIN, 1, Ceil).unwrap(), i64::MIN);
    assert_eq!(
        rate.ticks_at_frame(i64::MAX, 2, Floor),
        Err(RateError::NumericRange)
    );
    assert_eq!(
        rate.ticks_at_frame(i64::MIN, 2, Ceil),
        Err(RateError::NumericRange)
    );
}
