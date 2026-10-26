//! Actual f32 track samples are the oracle, independently of profile readouts.
use super::*;

fn both(speed: f64) -> TimingSettings {
    let side = Some(TimingSide {
        speed: Some(speed),
        influence: Some(25.),
    });
    TimingSettings {
        interpolation: TimingInterpolation::Bezier,
        units: VelocityUnits::PropertyPerSecond,
        incoming: side,
        outgoing: side,
        match_speed: true,
    }
}

#[test]
fn matching_scalar_rates_reaches_actual_samples_even_when_the_next_interval_reverses() {
    let mut p = scalar_fixture().snapshot();
    p.layers[0].transform.opacity.keys[2].value = 0.25;
    let mut engine = Engine::new(p).unwrap();
    let original = engine.snapshot();
    engine
        .apply(command(
            vec![selection(property(7, Property::Opacity, None), &[40])],
            both(10.),
        ))
        .unwrap();
    let edited = engine.snapshot();
    let samples = |p: &Project| {
        let track = &p.layers[0].transform.opacity;
        let h = 0.01;
        let value = f64::from(track.sample(33.).unwrap());
        let seconds = h / f64::from(p.fps);
        [
            (value - f64::from(track.sample(33. - h).unwrap())) * 100. / seconds,
            (f64::from(track.sample(33. + h).unwrap()) - value) * 100. / seconds,
        ]
    };
    let rates = samples(&edited);
    // 1/3000 second finite differences over f32 values; this bounds both
    // truncation and f32 quantization, not authored data or stored handles.
    for speed in rates {
        assert!((speed - 10.).abs() < 0.1, "actual rate={speed}");
    }
    let old = samples(&original);
    assert!(old[0] > 40. && old[1] < -20., "negative control={old:?}");
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), &edited).unwrap();
    let reopened = motion_core::storage::load(directory.path()).unwrap();
    assert_eq!(reopened, edited);
    assert_eq!(samples(&reopened), rates);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &original);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &edited);
    assert_eq!(samples(engine.project()), rates);
}

#[test]
fn equal_vector_rates_prove_c1_only_when_spatial_directions_also_match() {
    for corner in [false, true] {
        let mut p = fixture().snapshot();
        p.layers[0].transform.position.keys = [
            (3, [0., 0., 0.]),
            (33, [60., 0., 0.]),
            (
                93,
                if corner {
                    [60., 60., 0.]
                } else {
                    [120., 0., 0.]
                },
            ),
        ]
        .into_iter()
        .map(|(frame, value)| Keyframe {
            frame,
            value,
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        })
        .collect();
        let mut engine = Engine::new(p).unwrap();
        engine
            .apply(command(
                vec![selection(property(7, Property::Position, None), &[40])],
                both(12.),
            ))
            .unwrap();
        let track = &engine.project().layers[0].transform.position;
        let h = 0.01;
        let seconds = h / 30.;
        let center = track.sample(33.).unwrap();
        let left = track.sample(33. - h).unwrap();
        let right = track.sample(33. + h).unwrap();
        let incoming: [f64; 3] =
            std::array::from_fn(|i| (f64::from(center[i]) - f64::from(left[i])) / seconds);
        let outgoing: [f64; 3] =
            std::array::from_fn(|i| (f64::from(right[i]) - f64::from(center[i])) / seconds);
        let norm = |v: [f64; 3]| v.into_iter().fold(0f64, f64::hypot);
        for rate in [norm(incoming), norm(outgoing)] {
            assert!((rate - 12.).abs() < 0.2, "corner={corner} rate={rate}");
        }
        let difference = norm(std::array::from_fn(|i| incoming[i] - outgoing[i]));
        if corner {
            assert!(
                difference > 16.,
                "same rates are not C1 across a corner: {incoming:?} {outgoing:?}"
            );
            assert!(outgoing[0].abs() < 0.01 && outgoing[1] > 11.8);
        } else {
            assert!(
                difference < 0.2,
                "straight path must be C1: {incoming:?} {outgoing:?}"
            );
        }
    }
}

#[test]
fn equal_scalar_values_and_hold_or_elastic_require_explicit_conversion_and_cannot_invent_speed() {
    for elastic in [false, true] {
        let mut p = scalar_fixture().snapshot();
        for key in &mut p.layers[0].transform.opacity.keys {
            key.value = 0.5;
        }
        let key = &mut p.layers[0].transform.opacity.keys[0];
        key.ease = Ease::Hold;
        key.curve = elastic.then_some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Elastic {
                oscillations: 2.,
                damping: 5.,
            },
        });
        let mut engine = Engine::new(p).unwrap();
        let selected = vec![selection(property(7, Property::Opacity, None), &[10])];
        rejected(
            &mut engine,
            command(selected.clone(), settings(0.)),
            "choose Bezier explicitly",
        );
        rejected(
            &mut engine,
            command(
                selected.clone(),
                TimingSettings {
                    interpolation: TimingInterpolation::Bezier,
                    ..settings(10.)
                },
            ),
            "stationary keyframe endpoint cannot have nonzero speed",
        );
        engine
            .apply(command(
                selected,
                TimingSettings {
                    interpolation: TimingInterpolation::Bezier,
                    ..settings(0.)
                },
            ))
            .unwrap();
        for frame in [3., 3.01, 18., 32.99, 33., 63.] {
            assert_eq!(
                engine.project().layers[0]
                    .transform
                    .opacity
                    .sample(frame)
                    .unwrap(),
                0.5
            );
        }
    }
}

#[test]
fn equal_endpoint_spatial_loops_can_have_physical_speed_but_have_no_displacement_average() {
    let mut p = fixture().snapshot();
    let track = &mut p.layers[0].transform.position;
    track.keys[0].frame = 3;
    track.keys[1].frame = 33;
    for key in &mut track.keys {
        key.value = [0.; 3];
        key.curve = None;
        key.ease = Ease::Linear;
    }
    track.keys[0].spatial = Some(SpatialTangents {
        incoming: None,
        outgoing: Some([20., 0., 0.]),
    });
    track.keys[1].spatial = Some(SpatialTangents {
        incoming: Some([-20., 0., 0.]),
        outgoing: None,
    });
    let mut engine = Engine::new(p).unwrap();
    let relative = timing::profile(
        engine.project(),
        &selected(),
        VelocityUnits::AverageMultiple,
    )
    .unwrap();
    assert_eq!(relative.outgoing.speed, None);
    engine.apply(command(selected(), settings(15.))).unwrap();
    let track = &engine.project().layers[0].transform.position;
    let h = 0.01;
    let velocity = f64::from(track.sample(3. + h).unwrap()[0]) / (h / 30.);
    assert!(
        (velocity - 15.).abs() < 0.2,
        "actual loop endpoint speed={velocity}"
    );
    assert_eq!(track.sample(3.).unwrap(), [0.; 3]);
    assert_eq!(track.sample(33.).unwrap(), [0.; 3]);
}
