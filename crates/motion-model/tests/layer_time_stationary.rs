use motion_model::{
    layer_time::{LayerClock, SourceTimeSample, TimeRemap, TimeRemapKey},
    Curve, CurveShape, CurveSpace, Ease, Easing, VideoClip,
};

fn key(frame: f64, source_frame: f64, easing: Easing) -> TimeRemapKey {
    TimeRemapKey {
        frame,
        source_frame,
        easing,
    }
}
fn decreasing(easing: Easing) -> TimeRemap {
    TimeRemap::new(vec![key(0., 24., easing), key(10., 0., Easing::default())]).unwrap()
}

#[test]
fn zero_speed_reverse_ease_keeps_the_last_native_sample_instead_of_becoming_a_freeze() {
    for ease in [Ease::In, Ease::InOut] {
        let remap = decreasing(Easing { ease, curve: None });
        let sample = SourceTimeSample::new(0., 1.)
            .unwrap()
            .through(LayerClock::default(), 0, Some(&remap))
            .unwrap();
        assert_eq!(sample.frame, 24.);
        assert_eq!(sample.rate, 0.);
        assert_eq!(
            sample.video_time_us(&VideoClip::new(1), 24).unwrap(),
            999_999
        );
        assert_eq!(sample.pcm_position(24, 48_000, 0).unwrap(), Some(47_999.));
        assert!(sample.in_interval(0., 24.));
    }
}

#[test]
fn a_real_freeze_retains_its_exact_point_and_silence_through_later_clocks() {
    let freeze = TimeRemap::new(vec![key(0., 24., Easing::default())]).unwrap();
    let sample = SourceTimeSample::new(0., 1.)
        .unwrap()
        .through(LayerClock::default(), 0, Some(&freeze))
        .unwrap();
    assert_eq!(sample.rate, 0.);
    assert_eq!(
        sample.video_time_us(&VideoClip::new(1), 24).unwrap(),
        1_000_000
    );
    assert_eq!(sample.pcm_position(24, 48_000, 0).unwrap(), None);
    assert!(!sample.in_interval(0., 24.));
    let child = TimeRemap::new(vec![
        key(
            0.,
            3.,
            Easing {
                ease: Ease::Hold,
                curve: None,
            },
        ),
        key(24., 11., Easing::default()),
        key(30., 17., Easing::default()),
    ])
    .unwrap();
    let held = sample
        .through(LayerClock::default(), 0, Some(&child))
        .unwrap();
    assert_eq!(held.frame, 11.);
    assert_eq!(held.pcm_position(24, 48_000, 0).unwrap(), None);
}

#[test]
fn a_stationary_parent_enters_the_left_hold_interval_without_losing_its_direction() {
    let parent = SourceTimeSample::new(0., 1.)
        .unwrap()
        .through(
            LayerClock::default(),
            0,
            Some(&decreasing(Easing {
                ease: Ease::In,
                curve: None,
            })),
        )
        .unwrap();
    let child = TimeRemap::new(vec![
        key(
            0.,
            3.,
            Easing {
                ease: Ease::Hold,
                curve: None,
            },
        ),
        key(24., 11., Easing::default()),
        key(30., 17., Easing::default()),
    ])
    .unwrap();
    let sample = parent
        .through(LayerClock::default(), 0, Some(&child))
        .unwrap();
    assert_eq!(sample.frame, 3.);
    assert_eq!(sample.rate, 0.);
    assert_eq!(sample.pcm_position(24, 48_000, 0).unwrap(), None);
}

#[test]
fn stationary_endpoints_keep_their_side_through_fps_conversion_and_two_reversals() {
    use motion_model::{
        layer_time::{TimeDirection, TimeStretch},
        CompositionClip,
    };
    for ease in [Ease::Out, Ease::InOut] {
        let sample = SourceTimeSample::new(10., -1.)
            .unwrap()
            .through(
                LayerClock::default(),
                0,
                Some(&decreasing(Easing { ease, curve: None })),
            )
            .unwrap();
        assert_eq!(sample.frame, 0.);
        assert_eq!(sample.rate, 0.);
        assert_eq!(sample.pcm_position(24, 48_000, 0).unwrap(), Some(0.));
        assert!(sample.in_interval(0., 24.));
    }
    let sample = SourceTimeSample::new(0., 1.)
        .unwrap()
        .through(
            LayerClock::default(),
            0,
            Some(&decreasing(Easing {
                ease: Ease::In,
                curve: None,
            })),
        )
        .unwrap();
    let child = sample
        .through_composition(&CompositionClip::new("child".into()), 24, 48)
        .unwrap();
    assert_eq!(child.frame, 48.);
    assert_eq!(child.rate, 0.);
    assert_eq!(
        child.video_time_us(&VideoClip::new(1), 48).unwrap(),
        999_999
    );
    let reverse = LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 48.).unwrap();
    let forward = child.through(reverse, 0, None).unwrap();
    assert_eq!(forward.frame, 0.);
    assert_eq!(forward.pcm_position(48, 48_000, 0).unwrap(), Some(0.));
    assert!(forward.in_interval(0., 48.));
    let backward = forward.through(reverse, 0, None).unwrap();
    assert_eq!(backward, child);
}

#[test]
fn bezier_and_elastic_stationary_points_keep_their_analytic_one_sided_traversal() {
    let curves = [
        (
            Curve {
                space: CurveSpace::Progress,
                shape: CurveShape::Quadratic {
                    control: [0.5, 1.5],
                    start: 0.,
                    end: 1.,
                },
            },
            0.75,
            [-1, -1],
        ),
        (
            Curve {
                space: CurveSpace::Progress,
                shape: CurveShape::Cubic {
                    control1: [1. / 3., 1.],
                    control2: [2. / 3., 0.],
                    start: 0.,
                    end: 1.,
                },
            },
            0.5,
            [1, -1],
        ),
        (
            Curve {
                space: CurveSpace::Velocity,
                shape: CurveShape::Quadratic {
                    control: [0.5, -1.],
                    start: 1.,
                    end: 1.,
                },
            },
            0.5,
            [1, -1],
        ),
        (
            Curve {
                space: CurveSpace::Progress,
                shape: CurveShape::Elastic {
                    oscillations: 1.,
                    damping: 1.,
                },
            },
            0.5,
            [-1, -1],
        ),
        (
            Curve {
                space: CurveSpace::Progress,
                shape: CurveShape::Elastic {
                    oscillations: 2.,
                    damping: 1.,
                },
            },
            0.5,
            [1, 1],
        ),
    ];
    for (curve, t, directions) in curves {
        curve.validate().unwrap();
        let remap = TimeRemap::new(vec![
            key(
                0.,
                0.,
                Easing {
                    ease: Ease::Linear,
                    curve: Some(curve),
                },
            ),
            key(1., 24., Easing::default()),
        ])
        .unwrap();
        let expected_frame = 24. * curve.sample(t).progress;
        for (incoming_rate, direction) in [(1., directions[0]), (-1., directions[1])] {
            let sample = SourceTimeSample::new(t, incoming_rate)
                .unwrap()
                .through(LayerClock::default(), 0, Some(&remap))
                .unwrap();
            assert_eq!(sample.frame.to_bits(), expected_frame.to_bits());
            let raw = VideoClip::new(1).source_time_us(expected_frame, 24);
            assert_eq!(
                sample.video_time_us(&VideoClip::new(1), 24).unwrap(),
                raw - i64::from(direction < 0),
                "{curve:?} incoming={incoming_rate}"
            );
            assert!(
                sample.pcm_position(24, 48_000, 0).unwrap().is_some(),
                "moving stationary point"
            );
            assert_eq!(
                sample.in_interval(expected_frame - 1., expected_frame),
                direction < 0
            );
        }
    }
}
