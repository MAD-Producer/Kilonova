use super::*;
use motion_core::{Keyframe, Track};

fn parameter(ease: Ease, curve: Option<Curve>) -> EffectParam {
    let mut track = Track::constant([0.25, 0., 0., 0.]);
    track.keys = vec![
        Keyframe {
            frame: 0,
            value: [0.25, 0., 0., 0.],
            ease,
            curve,
            spatial: None,
        },
        Keyframe {
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
                        |frame| sample_value(&parameter, frame, 17., speed, 30., 0).unwrap();
                    let points = graph_adaptive::sample(
                        knots(&parameter, start, end, 17.),
                        scale,
                        0.35,
                        |frame| sample_value(&parameter, frame, 17., speed, 30., 0),
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
    parameter.default=[0.;4];parameter.track.value=[0.;4];
    parameter.min = 0.;
    parameter.max = 3.;
    parameter.track.keys[0].value = [0.; 4];
    parameter.track.keys[1].value = [3., 0., 0., 0.];
    let seeds = knots(&parameter, 10., 90., 17.).collect::<Vec<_>>();
    assert!(seeds.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(seeds.contains(&77f64.next_down()));
    assert_eq!(
        sample_value(&parameter, 77f64.next_down(), 17., false, 30., 0).unwrap(),
        0.
    );
    assert_eq!(
        sample_value(&parameter, 77., 17., false, 30., 0).unwrap(),
        3.
    );
    for frame in [10., 17., 30., 77f64.next_down(), 77., 90.] {
        assert_eq!(
            sample_value(&parameter, frame, 17., true, 30., 0).unwrap(),
            0.
        );
    }
    let points = graph_adaptive::sample(seeds, [10000., 2000.], 0.35, |frame| {
        sample_value(&parameter, frame, 17., false, 30., 0)
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
            0
        ),
        Err(graph_adaptive::Failure::NonFinite { .. })
    ));
}
