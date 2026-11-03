use motion_model::{
    layer_time::{
        LayerClock, SourceTimeSample, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    Ease, Easing,
};

fn key(frame: f64, source_frame: f64, ease: Ease) -> TimeRemapKey {
    TimeRemapKey {
        frame,
        source_frame,
        easing: Easing { ease, curve: None },
    }
}

#[test]
fn reverse_input_selects_the_interval_before_an_exact_remap_key() {
    let remap = TimeRemap::new(vec![
        key(0., 0., Ease::Linear),
        key(10., 20., Ease::Linear),
        key(20., 0., Ease::Linear),
    ])
    .unwrap();
    for (frame, rate, expected_frame, expected_rate) in [
        (0., 1., 0., 2.),
        (0., -1., 0., 0.),
        (10., 1., 20., -2.),
        (10., -1., 20., -2.),
        (20., 1., 0., 0.),
        (20., -1., 0., 2.),
        (10., 0., 20., 0.),
    ] {
        let actual = SourceTimeSample::new(frame, rate)
            .unwrap()
            .through(LayerClock::default(), 0, Some(&remap))
            .unwrap();
        assert_eq!(actual.frame, expected_frame, "input={frame} rate={rate}");
        assert_eq!(actual.rate, expected_rate, "input={frame} rate={rate}");
    }
}

#[test]
fn negative_property_clock_selects_the_left_hold_value_without_a_fake_jump_rate() {
    let remap = TimeRemap::new(vec![
        key(0., 11., Ease::Hold),
        key(10., 51., Ease::Linear),
        key(20., 71., Ease::Linear),
    ])
    .unwrap();
    let reverse = LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 20.).unwrap();
    for (composition, expected_frame, expected_rate) in [
        (0., 71., -2.),
        (10., 11., 0.),
        (10.001, 11., 0.),
        (20., 11., 0.),
    ] {
        let actual = SourceTimeSample::new(composition, 1.)
            .unwrap()
            .through(reverse, 0, Some(&remap))
            .unwrap();
        assert_eq!(actual.frame, expected_frame);
        assert_eq!(actual.rate, expected_rate);
    }
    let still = SourceTimeSample::new(10., 0.)
        .unwrap()
        .through(LayerClock::default(), 0, Some(&remap))
        .unwrap();
    assert_eq!(
        still.frame, 51.,
        "a freeze retains the exact right-continuous value"
    );
    assert_eq!(still.rate, 0.);
}

#[test]
fn signed_cursor_membership_keeps_exact_half_open_edges_without_moving_the_coordinate() {
    for (frame, forward, reverse) in [
        (0., false, false),
        (2.25, true, false),
        (3., true, true),
        (7.75, false, true),
        (8., false, false),
    ] {
        for (rate, expected) in [(1., forward), (0., forward), (-1., reverse)] {
            let cursor = SourceTimeSample::new(frame, rate).unwrap();
            assert_eq!(cursor.in_interval(2.25, 7.75), expected);
            assert!(!cursor.in_interval(3., 3.));
            assert!(!cursor.in_interval(7., 3.));
            assert_eq!(cursor.frame.to_bits(), frame.to_bits());
        }
    }
}

#[test]
fn reverse_linear_endpoint_preserves_the_stored_double_precision_value_exactly() {
    let remap = TimeRemap::new(vec![
        key(0., 1_000_000_000., Ease::Linear),
        key(10., 0.0001, Ease::Linear),
    ])
    .unwrap();
    let sample = SourceTimeSample::new(10., -1.)
        .unwrap()
        .through(LayerClock::default(), 0, Some(&remap))
        .unwrap();
    assert_eq!(sample.frame.to_bits(), 0.0001_f64.to_bits());
    assert!(sample.rate > 0.);
}
