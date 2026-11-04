use motion_model::{
    FrameRate, RateError, TimecodeError,
    TimecodeMode::{Drop, NonDrop},
};

#[test]
fn rate_entry_keeps_exact_decimals_distinct_from_rational_broadcast_clocks() {
    for (text, ratio) in [
        ("24", [24, 1]),
        (" 24000/1001 ", [24000, 1001]),
        ("29.97", [2997, 100]),
        ("29.970000000", [2997, 100]),
        ("60000000000/1001000000", [60000, 1001]),
        ("1.00000000000000000000", [1, 1]),
    ] {
        let rate: FrameRate = text.parse().unwrap();
        assert_eq!(rate.ratio(), ratio, "{text}");
        assert_eq!(rate.to_string().parse::<FrameRate>().unwrap(), rate);
    }
    assert_ne!(
        "29.97".parse::<FrameRate>().unwrap(),
        "30000/1001".parse().unwrap()
    );
    for text in [
        "", "+24", "-24", "1e3", "NaN", ".5", "24.", "24/", "24/1/2", "2 4", "1/.5",
    ] {
        assert_eq!(text.parse::<FrameRate>(), Err(RateError::Syntax), "{text}");
    }
    for text in ["0", "0/1", "1/0", "0.000"] {
        assert_eq!(text.parse::<FrameRate>(), Err(RateError::Zero));
    }
    assert_eq!(
        "1000001/1000000".parse::<FrameRate>(),
        Err(RateError::ComponentRange)
    );
    assert!("9".repeat(128).parse::<FrameRate>().is_err());
    assert_eq!(
        "9".repeat(39).parse::<FrameRate>(),
        Err(RateError::NumericRange)
    );
}

#[test]
fn non_drop_labels_preserve_integer_count_and_explicit_fractional_rate_semantics() {
    for fps in [1, 24, 25, 30, 48, 60, 120, 240] {
        let rate = FrameRate::integer(fps).unwrap();
        assert_eq!(rate.parse_timecode("0:01:10:15", NonDrop).is_ok(), fps > 15);
        assert_eq!(
            rate.format_timecode(i64::from(fps) * 3600, NonDrop)
                .unwrap(),
            "01:00:00:00"
        );
        for frame in [i64::MIN, -108000, -1, 0, 65, 108000, i64::MAX] {
            let label = rate.format_timecode(frame, NonDrop).unwrap();
            assert_eq!(
                rate.parse_timecode(&label, NonDrop).unwrap(),
                frame,
                "{fps} {label}"
            );
        }
    }
    let rate = FrameRate::new(24000, 1001).unwrap();
    assert_eq!(rate.format_timecode(86400, NonDrop).unwrap(), "01:00:00:00");
    assert_eq!(
        FrameRate::new(30000, 1001)
            .unwrap()
            .format_timecode(108000, NonDrop)
            .unwrap(),
        "01:00:00:00"
    );
    assert_eq!(
        FrameRate::integer(30)
            .unwrap()
            .parse_timecode("0:01:10:15", NonDrop)
            .unwrap(),
        2115
    );
    assert_eq!(
        FrameRate::new(2997, 100)
            .unwrap()
            .format_timecode(1, NonDrop),
        Err(TimecodeError::UnsupportedRate)
    );
}

#[test]
fn drop_labels_cross_minute_ten_minute_hour_day_and_signed_boundaries() {
    for (num, scale) in [(30000, 1i64), (60000, 2)] {
        let rate = FrameRate::new(num, 1001).unwrap();
        let first_label = if scale == 1 {
            "00:01:00;02"
        } else {
            "00:01:00;04"
        };
        for (frame, label) in [
            (1800 * scale, first_label),
            (17982 * scale, "00:10:00;00"),
            (107892 * scale, "01:00:00;00"),
            (2589408 * scale, "24:00:00;00"),
        ] {
            assert_eq!(rate.format_timecode(frame, Drop).unwrap(), label);
            assert_eq!(rate.parse_timecode(label, Drop).unwrap(), frame);
        }
        assert_eq!(
            rate.format_timecode(1800 * scale - 1, Drop).unwrap(),
            if scale == 1 {
                "00:00:59;29"
            } else {
                "00:00:59;59"
            }
        );
        for frame in [i64::MIN, -107892, -1800, -1, 0, 17981, i64::MAX] {
            assert_eq!(
                rate.parse_timecode(&rate.format_timecode(frame, Drop).unwrap(), Drop)
                    .unwrap(),
                frame
            );
        }
        for minute in [1, 2, 9, 11, 59] {
            for frame in 0..2 * scale {
                assert_eq!(
                    rate.parse_timecode(&format!("00:{minute:02}:00;{frame:02}"), Drop),
                    Err(TimecodeError::DroppedLabel)
                );
            }
        }
    }
}

#[test]
fn every_drop_label_in_one_hour_matches_an_independent_forward_label_counter() {
    for (numerator, nominal, drop, expected_count) in
        [(30000, 30, 2, 107892), (60000, 60, 4, 215784)]
    {
        let rate = FrameRate::new(numerator, 1001).unwrap();
        let mut frame = 0i64;
        // Enumerate permitted clock labels directly instead of using the
        // implementation's ten-minute-block conversion or inverse arithmetic.
        for minute in 0..60 {
            for second in 0..60 {
                for number in 0..nominal {
                    if minute % 10 != 0 && second == 0 && number < drop {
                        continue;
                    }
                    let label = format!("00:{minute:02}:{second:02};{number:02}");
                    assert_eq!(rate.format_timecode(frame, Drop).unwrap(), label);
                    assert_eq!(rate.parse_timecode(&label, Drop).unwrap(), frame);
                    frame += 1;
                }
            }
        }
        assert_eq!(frame, expected_count);
    }
}

#[test]
fn timecode_rejects_wrong_modes_missing_labels_malformed_fields_and_overflow() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    for label in [
        "00:00:00:00",
        "0:0;0",
        "00:00:00;",
        "+00:00:00;00",
        " 00:00:00;00",
        "00:00:00;00;00",
    ] {
        assert_eq!(
            rate.parse_timecode(label, Drop),
            Err(TimecodeError::Syntax),
            "{label}"
        );
    }
    for label in ["00:60:00;00", "00:00:60;00", "00:00:00;30"] {
        assert_eq!(
            rate.parse_timecode(label, Drop),
            Err(TimecodeError::FieldRange)
        );
    }
    assert_eq!(
        rate.parse_timecode("99999999999999999999999:00:00;00", Drop),
        Err(TimecodeError::NumericRange)
    );
    assert_eq!(
        rate.parse_timecode("00:00:00;00", NonDrop),
        Err(TimecodeError::Syntax)
    );
    for rate in [
        FrameRate::integer(30).unwrap(),
        FrameRate::new(24000, 1001).unwrap(),
        FrameRate::new(2997, 100).unwrap(),
        FrameRate::new(120000, 1001).unwrap(),
    ] {
        assert_eq!(
            rate.format_timecode(0, Drop),
            Err(TimecodeError::UnsupportedRate)
        );
    }
}
