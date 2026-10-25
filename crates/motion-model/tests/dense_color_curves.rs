use motion_model::{
    color_curves::ColorInterpolation, CurveObject, CurveTrack, Ease, Error, Keyframe,
    HIGH_PRECISION_CURVE_SAMPLES as N,
};

const PROBE: usize = 2049;

fn narrow() -> CurveObject {
    let mut curve = CurveObject::default();
    let x = PROBE as f32 / (N - 1) as f32;
    curve.interpolation[1] = ColorInterpolation::Linear;
    curve.channels[1] = vec![
        [0., 0.],
        [x - 0.0001, 0.],
        [x, 1.],
        [x + 0.0001, 0.],
        [1., 0.],
    ];
    curve
}

fn key(frame: i32, value: CurveObject) -> Keyframe<CurveObject> {
    Keyframe {
        frame,
        value,
        ease: Ease::Linear,
        curve: None,
        spatial: None,
    }
}

#[test]
fn dense_authored_sampling_retains_features_and_values_absent_from_the_preview() {
    let track = CurveTrack {
        value: narrow(),
        keys: vec![],
    };
    let captured = track.sampled_curve(0.).unwrap();
    assert!(captured.preview_lut().iter().all(|row| row[0] == 0.));
    let dense = captured.high_precision_lut();
    assert_eq!(dense.len(), N);
    assert_eq!(dense[PROBE][0], 1.);
    assert_eq!(dense[PROBE - 1][0], 0.);
    assert_eq!(dense[PROBE + 1][0], 0.);
    assert_eq!(dense[PROBE][3], PROBE as f32 / (N - 1) as f32);
    let mut fine = CurveObject::default();
    fine.channels[1] = vec![[0., 0.20001], [1., 0.20002]];
    let dense = CurveTrack {
        value: fine,
        keys: vec![],
    }
    .sampled_curve(0.)
    .unwrap()
    .high_precision_lut();
    assert_eq!(dense[0][0], 0.20001);
    assert_eq!(dense[N - 1][0], 0.20002);
    assert_ne!(dense[0][0], dense[N - 1][0]);
}

#[test]
fn temporal_dense_sampling_is_deterministic_and_keeps_endpoint_selection() {
    let a = narrow();
    let mut b = a.clone();
    b.channels[1].iter_mut().for_each(|p| p[1] *= 0.5);
    let track = CurveTrack {
        value: a.clone(),
        keys: vec![key(0, a), key(10, b)],
    };
    for frame in [0., 10., 5., 0., 5.] {
        let captured = track.sampled_curve(frame).unwrap();
        assert_eq!(captured.preview_lut(), track.sample(frame).unwrap());
        assert_eq!(
            captured.high_precision_lut()[PROBE][0],
            1. - frame as f32 / 20.
        );
    }
    assert_eq!(
        track.sampled_curve(f64::NAN).unwrap().high_precision_lut()[PROBE][0],
        1.
    );
    assert_eq!(
        track
            .sampled_curve(f64::INFINITY)
            .unwrap()
            .high_precision_lut()[PROBE][0],
        0.5
    );
}

#[test]
fn dense_frozen_data_survives_serialization_editing_and_another_dense_sample() {
    let rows = CurveTrack {
        value: narrow(),
        keys: vec![],
    }
    .sampled_curve(0.)
    .unwrap()
    .high_precision_lut();
    let frozen = CurveObject {
        sampled_lut: Some(rows.clone()),
        ..Default::default()
    };
    let saved = serde_json::to_vec(&frozen).unwrap();
    let restored: CurveObject = serde_json::from_slice(&saved).unwrap();
    let editable = restored.editable().unwrap();
    assert_eq!(editable.channel_luts[1].as_ref().unwrap().len(), N);
    assert!(editable.channels.iter().all(|channel| channel.len() <= 64));
    for value in [restored, editable] {
        let dense = CurveTrack {
            value,
            keys: vec![],
        }
        .sampled_curve(0.)
        .unwrap()
        .high_precision_lut();
        assert_eq!(dense, rows);
    }
}

#[test]
fn snapshot_budget_and_malformed_sources_fail_without_mutating_or_weakening_causes() {
    let mut track = CurveTrack::default();
    let healthy = track.sampled_curve(0.).unwrap();
    let before = track.clone();
    let error = track
        .sampled_curve_with_budget(2., Some(&healthy), healthy.source_bytes() - 1)
        .unwrap_err();
    assert!(matches!(error, Error::ColorCurveSampling { frame: 2., .. }));
    assert!(error.to_string().contains("CPU budget"));
    assert_eq!(track, before);
    assert_eq!(
        track
            .sampled_curve_with_budget(2., Some(&healthy), healthy.source_bytes())
            .unwrap(),
        healthy
    );
    for length in [0, 1, 255, 257, N - 1, N + 1] {
        track.value.sampled_lut = Some(vec![[0.; 4]; length]);
        let expected = track.value.validate().unwrap_err().to_string();
        let error = track.sampled_curve(3.).unwrap_err();
        let Error::ColorCurveSampling { frame: 3., source } = error else {
            panic!("missing sampling cause")
        };
        assert_eq!(source.to_string(), expected);
    }
}
