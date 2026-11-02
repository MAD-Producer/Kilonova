use super::*;
use motion_core::{Keyframe, Track};

#[test]
fn stretched_and_reversed_effect_samples_fit_signed_speed_and_keep_jump_neighbors() {
    use motion_core::layer_time::{TimeDirection, TimeStretch};
    for reverse in [false, true] {
        for ease in [Ease::Linear, Ease::InOut, Ease::Hold] {
            let clock = LayerClock::new(
                TimeStretch::new(3, 2).unwrap(),
                if reverse {
                    TimeDirection::Reverse
                } else {
                    TimeDirection::Forward
                },
                if reverse { 40. } else { 0. },
                0.,
            )
            .unwrap();
            let mut p = parameter(ease, None);
            p.track.keys[0].frame = 3;
            p.track.keys[1].frame = 13;
            let seeds = knots(&p, 0., 60., 2., clock).unwrap().collect::<Vec<_>>();
            assert!(seeds.windows(2).all(|p| p[0] < p[1]));
            for local in [3., 13.] {
                let time = clock.projected_composition_frame(local, 2).unwrap();
                for adjacent in [time.next_down(), time, time.next_up()] {
                    assert!(seeds.contains(&adjacent));
                }
            }
            for speed in [false, true] {
                let fit =
                    fitted(&p, [0., 60.], 2., [800., 400.], 0.35, speed, 30., 0, clock).unwrap();
                let points = graph_adaptive::sample(seeds.clone(), [20., 1000.], 0.35, |t| {
                    sample_value(&p, t, 2., speed, 30., 0, clock)
                })
                .unwrap();
                assert!(points.len() < 65536);
                for n in 0..=960 {
                    let local = f64::from(n) / 64.;
                    let time = clock.projected_composition_frame(local, 2).unwrap();
                    let expected =
                        sample_value(&p, local + 2., 2., speed, 30., 0, LayerClock::default())
                            .unwrap()
                            * if speed { clock.rate() } else { 1. };
                    let actual = sample_value(&p, time, 2., speed, 30., 0, clock).unwrap();
                    assert!(
                        (actual - expected).abs() < 0.00001,
                        "reverse={reverse} ease={ease:?} speed={speed} local={local}"
                    );
                    assert!(actual >= fit.y.low && actual <= fit.y.high);
                }
            }
        }
    }
}

fn parameter(ease: Ease, curve: Option<Curve>) -> EffectParam {
    let mut track = Track::constant([0.25, 0., 0., 0.]);
    track.keys = vec![
        Keyframe {
            auto_bezier: false,
            frame: 0,
            value: [0.25, 0., 0., 0.],
            ease,
            curve,
            spatial: None,
        },
        Keyframe {
            auto_bezier: false,
            frame: 60,
            value: [0.75, 0., 0., 0.],
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        },
    ];
    track.validate_local().unwrap();
    EffectParam {
        kind: motion_effects::ParamKind::Float,
        animatable: true,
        min: 0.,
        max: 1.,
        implemented: true,
        default: [0.25, 0., 0., 0.],
        track,
        curve: None,
    }
}

#[test]
fn effect_adaptive_values_and_bounded_physical_speed_match_dense_model_oracles_at_high_zoom() {
    let shapes = [
        None,
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Quadratic {
                control: [0.2, 2.],
                start: 0.,
                end: 1.,
            },
        }),
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Cubic {
                control1: [0.001, -1.],
                control2: [0.999, 2.],
                start: 0.,
                end: 1.,
            },
        }),
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Elastic {
                oscillations: 8.,
                damping: 0.5,
            },
        }),
    ];
    for (ease, curve) in shapes
        .into_iter()
        .map(|curve| (Ease::InOut, curve))
        .chain(std::iter::once((Ease::Hold, None)))
    {
        let parameter = parameter(ease, curve);
        for speed in [false, true] {
            for [start, end] in [[10., 90.], [39., 40.], [17., 17.1], [76.99, 77.01]] {
                for scale_y in [500., 32000.] {
                    let scale = [800. / (end - start), scale_y];
                    let value =
                        |frame| {
                        sample_value(&parameter, frame, 17., speed, 30., 0, LayerClock::default()).unwrap()
                    };
                    let points = graph_adaptive::sample(
                        knots(&parameter, start, end, 17., LayerClock::default()).unwrap(),
                        scale,
                        0.35,
                        |frame| {
                            sample_value(&parameter, frame, 17., speed, 30., 0,
                                LayerClock::default(),
                            )
                        },
                    )
                    .unwrap();
                    assert!(points.len() >= 257 && points.len() < 65536);
                    for segment in points.windows(2) {
                        let delta = [
                            (segment[1].0 - segment[0].0) * scale[0],
                            (segment[1].1 - segment[0].1) * scale[1],
                        ];
                        let squared = delta[0] * delta[0] + delta[1] * delta[1];
                        for i in 0..=128 {
                            let frame =
                                segment[0].0 + (segment[1].0 - segment[0].0) * f64::from(i) / 128.;
                            let probe = [
                                (frame - segment[0].0) * scale[0],
                                (value(frame) - segment[0].1) * scale[1],
                            ];
                            let t = if squared > 0. {
                                ((probe[0] * delta[0] + probe[1] * delta[1]) / squared)
                                    .clamp(0., 1.)
                            } else {
                                0.
                            };
                            let error = (probe[0] - t * delta[0]).hypot(probe[1] - t * delta[1]);
                            assert!(error<=0.36,"ease={ease:?} curve={curve:?} speed={speed} range={start}..{end} scale_y={scale_y} frame={frame} error={error}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn discrete_effect_steps_keep_the_left_limit_and_zero_speed_without_changing_easing() {
    let mut parameter = parameter(Ease::Linear, None);
    parameter.kind = motion_effects::ParamKind::Enum;
    parameter.default = [0.; 4];
    parameter.track.value = [0.; 4];
    parameter.min = 0.;
    parameter.max = 3.;
    parameter.track.keys[0].value = [0.; 4];
    parameter.track.keys[1].value = [3., 0., 0., 0.];
    let seeds = knots(&parameter, 10., 90., 17., LayerClock::default())
        .unwrap().collect::<Vec<_>>();
    assert!(seeds.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(seeds.contains(&77f64.next_down()));
    assert_eq!(
        sample_value(&parameter, 77f64.next_down(), 17., false, 30., 0,
            LayerClock::default()
        ).unwrap(),
        0.
    );
    assert_eq!(
        sample_value(&parameter, 77., 17., false, 30., 0, LayerClock::default()).unwrap(),
        3.
    );
    for frame in [10., 17., 30., 77f64.next_down(), 77., 90.] {
        assert_eq!(
            sample_value(&parameter, frame, 17., true, 30., 0, LayerClock::default()).unwrap(),
            0.
        );
    }
    let points = graph_adaptive::sample(seeds, [10000., 2000.], 0.35, |frame| {
        sample_value(&parameter, frame, 17., false, 30., 0, LayerClock::default())
    })
    .unwrap();
    let jump = points
        .windows(2)
        .find(|pair| pair[0].1 == 0. && pair[1].1 == 3.)
        .unwrap();
    assert!((jump[1].0 - jump[0].0) * 10000. < 0.36);
    assert_eq!(parameter.track.keys[0].ease, Ease::Linear);
}

#[test]
fn fit_preserves_actual_sampler_errors_and_rejects_nonfinite_values_without_partial_ranges() {
    let mut parameter = parameter(Ease::Linear, None);
    parameter.track.axes = Some(Box::new(motion_core::AxisTracks {
        x: Track::constant(0.),
        y: Track::constant(0.),
        z: Track::constant(0.),
    }));
    let error = fitted(
        &parameter,
        [17., 77.],
        17.,
        [800., 320.],
        0.35,
        false,
        30.,
        0,
        LayerClock::default(),
    )
    .unwrap_err();
    match error {
        graph_adaptive::Failure::Sample { frame, cause } => {
            assert_eq!(frame, 17.);
            assert!(cause
                .to_string()
                .contains("separated tracks cannot contain vector keys"));
        }
        other => panic!("{other:?}"),
    }
    parameter.track.axes = None;
    parameter.track.keys.clear();
    parameter.track.value = [f32::NAN, 0., 0., 0.];
    assert!(matches!(
        fitted(
            &parameter,
            [17., 77.],
            17.,
            [800., 320.],
            0.35,
            false,
            30.,
            0,
            LayerClock::default()
        ),
        Err(graph_adaptive::Failure::NonFinite { .. })
    ));
}
