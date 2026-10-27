use super::*;
use motion_core::{Engine, Layer, Project};

#[test]
fn adaptive_normalized_curves_cover_high_zoom_and_one_sided_hold_against_dense_samples() {
    let easings = [
        Easing::default(),
        Easing {
            ease: Ease::Hold,
            curve: None,
        },
        Easing {
            ease: Ease::Linear,
            curve: Some(Curve {
                space: CurveSpace::Progress,
                shape: CurveShape::Cubic {
                    control1: [0.001, -1.],
                    control2: [0.999, 2.],
                    start: 0.,
                    end: 1.,
                },
            }),
        },
        Easing {
            ease: Ease::Linear,
            curve: Some(Curve {
                space: CurveSpace::Progress,
                shape: CurveShape::Elastic {
                    oscillations: 8.,
                    damping: 0.5,
                },
            }),
        },
        Easing {
            ease: Ease::Linear,
            curve: Some(Curve {
                space: CurveSpace::Velocity,
                shape: CurveShape::Quadratic {
                    control: [0.3, 2.],
                    start: 0.,
                    end: 1.,
                },
            }),
        },
    ];
    for easing in easings {
        easing.validate().unwrap();
        for [start, end] in [[0., 1.], [0.49, 0.51], [0., 0.0001], [0.9999, 1.]] {
            let bounds = Bounds {
                x: Range::new(start, end).unwrap(),
                y: Range::new(-1., 2.).unwrap(),
            };
            let size = [800., 500.];
            let points = sample(easing, bounds, size, 0.35).unwrap();
            let scale = [
                f64::from(size[0]) / bounds.x.span(),
                f64::from(size[1]) / bounds.y.span(),
            ];
            for pair in points.windows(2) {
                let delta = [
                    (pair[1].0 - pair[0].0) * scale[0],
                    (pair[1].1 - pair[0].1) * scale[1],
                ];
                let length = delta[0].hypot(delta[1]);
                for i in 0..=128 {
                    let t = pair[0].0 + (pair[1].0 - pair[0].0) * f64::from(i) / 128.;
                    let p = [
                        (t - pair[0].0) * scale[0],
                        (normalized(easing, t).unwrap() - pair[0].1) * scale[1],
                    ];
                    let u = if length == 0. {
                        0.
                    } else {
                        ((p[0] * delta[0] + p[1] * delta[1]) / length / length).clamp(0., 1.)
                    };
                    let error = (p[0] - u * delta[0]).hypot(p[1] - u * delta[1]);
                    assert!(
                        error <= 0.36,
                        "easing={easing:?} t={t} range={start}..{end} error={error}"
                    );
                }
            }
        }
    }
}

#[test]
fn automatic_ranges_include_authored_controls_and_actual_extrema_without_clipping() {
    let easing = Easing {
        ease: Ease::Linear,
        curve: Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Cubic {
                control1: [0.1, -6.],
                control2: [0.8, 7.],
                start: 0.,
                end: 1.,
            },
        }),
    };
    easing.validate().unwrap();
    let bounds = automatic(easing, [800., 130.], 0.35).unwrap();
    assert!(bounds.y.low < -6. && bounds.y.high > 7.);
    for i in 0..=10000 {
        let value = normalized(easing, f64::from(i) / 10000.).unwrap();
        assert!((bounds.y.low..=bounds.y.high).contains(&value));
    }
}

#[test]
fn outgoing_interval_maps_composition_time_once_and_never_invents_last_key_time() {
    assert_eq!(interval_frame(17, Some(77), 0.25), Some(32.));
    for (start, end) in [(17, None), (-1, Some(17)), (17, Some(17)), (77, Some(17))] {
        assert_eq!(interval_frame(start, end, 0.5), None);
    }
    for t in [-0.1, 1.1, f64::NAN] {
        assert_eq!(interval_frame(17, Some(77), t), None);
    }
}

fn scalar_state(a: f32, b: f32, span: i32) -> Value {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Scalar", [100., 50.], [0.; 3], [1.; 4]);
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 17,
        out_frame: 120,
        offset_frame: 17,
    });
    layer.transform.opacity.upsert(0, a, Ease::Linear).unwrap();
    layer
        .transform
        .opacity
        .upsert(span, b, Ease::Linear)
        .unwrap();
    project.layers.push(layer);
    let engine = Engine::new(project).unwrap();
    crate::panels::curve_comparison::tests::state(&engine)
}

#[test]
fn actual_readouts_preserve_zero_and_negative_amplitude_and_duration_in_physical_units() {
    let target = KeyframeTarget::Property {
        object: 7,
        property: Property::Opacity,
        axis: None,
    };
    for (a, b) in [(0.2, 0.8), (0.8, 0.2), (0.5, 0.5)] {
        for span in [15, 60] {
            let state = scalar_state(a, b, span);
            let mut view = View::default();
            for speed in [false, true] {
                let (title, lines) = view.actual_readout(
                    &state,
                    Locale::En,
                    &target,
                    17,
                    Some(17 + i64::from(span)),
                    0.5,
                    speed,
                    "normalized t=0.5".into(),
                    vec![],
                );
                assert!(title.ends_with(&format!("{:.2} f", 17. + f64::from(span) * 0.5)));
                let expected = if speed {
                    f64::from(b - a) * 100. * 30. / f64::from(span)
                } else {
                    f64::from((a + b) * 0.5) * 100.
                };
                let data = view.readout.as_ref().unwrap().data.as_ref().unwrap();
                let actual = data
                    .sample(17. + f64::from(span) * 0.5, 0, 30., speed)
                    .unwrap();
                assert!((f64::from(actual) - expected).abs() < 0.0001);
                assert_eq!(lines.len(), 1);
                assert!(lines[0].ends_with(if speed { "%/s" } else { "%" }));
                if a > b && speed {
                    assert!(lines[0].contains('-'));
                }
            }
        }
    }
}

#[test]
fn last_key_readout_remains_normalized_without_fabricated_frames_or_physical_units() {
    let state = scalar_state(0.2, 0.8, 60);
    let target = KeyframeTarget::Property {
        object: 7,
        property: Property::Opacity,
        axis: None,
    };
    let mut view = View::default();
    let expected = (
        "t=0.5".to_owned(),
        vec!["0.5000 normalized progress".to_owned()],
    );
    assert_eq!(
        view.actual_readout(
            &state,
            Locale::En,
            &target,
            77,
            None,
            0.5,
            false,
            expected.0.clone(),
            expected.1.clone()
        ),
        expected
    );
    assert!(view.readout.is_none());
}

#[test]
fn coupled_readout_keeps_each_visible_component_and_source_revision_invalidates_cache() {
    let engine = crate::panels::curve_comparison::tests::fixture();
    let mut state = crate::panels::curve_comparison::tests::state(&engine);
    let target = KeyframeTarget::Property {
        object: 7,
        property: Property::Position,
        axis: None,
    };
    let mut view = View::default();
    let (_, lines) = view.actual_readout(
        &state,
        Locale::En,
        &target,
        15,
        Some(45),
        0.5,
        false,
        "t=0.5".into(),
        vec![],
    );
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("Position X: 50.0000 px"));
    assert!(lines[1].contains("Position Y: 10.0000 px"));
    let old = view
        .readout
        .as_ref()
        .unwrap()
        .data
        .as_ref()
        .unwrap()
        .clone();
    state["revision"] = 100.into();
    view.actual_readout(
        &state,
        Locale::Zh,
        &target,
        15,
        Some(45),
        0.5,
        false,
        "t=0.5".into(),
        vec![],
    );
    assert!(!Arc::ptr_eq(
        &old,
        view.readout.as_ref().unwrap().data.as_ref().unwrap()
    ));
}
