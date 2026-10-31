use motion_model::{
    color_curves::ColorInterpolation, CurveObject, CurveTrack, Ease, Error, Keyframe,
};

fn refused(curve: &CurveObject) {
    let before = format!("{curve:?}");
    let expected = curve.validate().unwrap_err().to_string();
    for error in [
        curve.lut().unwrap_err(),
        curve.editable().unwrap_err(),
        curve.graph().unwrap_err(),
    ] {
        assert!(matches!(error, Error::Invalid(_)));
        assert_eq!(error.to_string(), expected);
    }
    assert_eq!(format!("{curve:?}"), before);
}

#[test]
fn malformed_public_curve_inputs_return_the_validation_cause_without_mutation() {
    for length in [0, 1, 255, 257] {
        let mut curve = CurveObject::default();
        curve.sampled_lut = Some(vec![[0.; 4]; length]);
        refused(&curve);
        curve.sampled_lut = None;
        assert_eq!(curve.lut().unwrap()[255], [1.; 4]);
        for channel in 0..5 {
            curve.channel_luts[channel] = Some(vec![0.; length]);
            refused(&curve);
            curve.channel_luts[channel] = None;
            assert_eq!(curve.lut().unwrap()[255], [1.; 4]);
        }
    }
    for mode in [ColorInterpolation::Linear, ColorInterpolation::NaturalCubic] {
        for points in [vec![], vec![[0., 0.]], vec![[0., 0.], [0., 1.], [1., 1.]]] {
            let mut curve = CurveObject::default();
            curve.interpolation[0] = mode;
            curve.channels[0] = points;
            refused(&curve);
        }
    }
    let mut nonfinite = CurveObject::default();
    nonfinite.channel_luts[3] = Some(vec![f32::NAN; 256]);
    refused(&nonfinite);
    nonfinite.channel_luts[3] = None;
    nonfinite.sampled_lut = Some(vec![[f32::INFINITY; 4]; 256]);
    refused(&nonfinite);
}

fn key(frame: i32, value: CurveObject) -> Keyframe<CurveObject> {
    Keyframe {
        auto_bezier: false,
        frame,
        value,
        ease: Ease::Linear,
        curve: None,
        spatial: None,
    }
}

#[test]
fn temporal_curve_refusals_preserve_keys_and_recover_at_static_endpoint_and_intermediate_frames() {
    let mut bad = CurveObject::default();
    bad.sampled_lut = Some(vec![[0.; 4]]);
    let mut track = CurveTrack {
        value: bad.clone(),
        keys: vec![],
    };
    assert!(matches!(
        track.sample(4.),
        Err(Error::ColorCurveSampling { frame: 4., .. })
    ));
    for frame in [
        f64::NEG_INFINITY,
        -1.,
        0.,
        2.5,
        10.,
        f64::INFINITY,
        f64::NAN,
    ] {
        track.keys = vec![key(0, bad.clone()), key(10, bad.clone())];
        let before = track.clone();
        let error = track.sample(frame).unwrap_err();
        let Error::ColorCurveSampling {
            frame: returned_frame,
            source,
        } = &error
        else {
            panic!("missing native curve sampling context: {error:?}");
        };
        assert_eq!(returned_frame.to_bits(), frame.to_bits());
        assert!(matches!(source.as_ref(), Error::Invalid(_)));
        assert_eq!(source.to_string(), bad.validate().unwrap_err().to_string());
        let cause = std::error::Error::source(&error)
            .unwrap()
            .downcast_ref::<Box<Error>>()
            .unwrap();
        assert!(matches!(cause.as_ref(), Error::Invalid(_)));
        assert_eq!(track, before);
        track.value = CurveObject::default();
        for key in &mut track.keys {
            key.value = CurveObject::default();
        }
        assert_eq!(track.sample(frame).unwrap()[255], [1.; 4]);
    }
}

#[test]
fn healthy_curves_keep_exact_endpoints_interpolation_and_frozen_channel_identity() {
    let first = CurveObject::default();
    let mut second = first.clone();
    second.channels[4] = vec![[0., 0.], [1., 0.5]];
    let track = CurveTrack {
        value: first.clone(),
        keys: vec![key(0, first.clone()), key(10, second.clone())],
    };
    assert_eq!(track.sample(f64::NAN).unwrap(), first.lut().unwrap());
    assert_eq!(
        track.sample(f64::NEG_INFINITY).unwrap(),
        first.lut().unwrap()
    );
    assert_eq!(track.sample(f64::INFINITY).unwrap(), second.lut().unwrap());
    let middle = track.sample(5.).unwrap();
    let first_lut = first.lut().unwrap();
    let second_lut = second.lut().unwrap();
    for index in 0..256 {
        assert_eq!(
            middle[index][3],
            first_lut[index][3] + (second_lut[index][3] - first_lut[index][3]) * 0.5
        );
    }
    let frozen = CurveObject {
        sampled_lut: Some(middle.to_vec()),
        ..first
    };
    let editable = frozen.editable().unwrap();
    assert_eq!(editable.lut().unwrap(), middle);
    assert_eq!(frozen.lut().unwrap(), middle);
    assert_eq!(
        frozen.graph().unwrap()["outputLut"],
        serde_json::to_value(middle.to_vec()).unwrap()
    );
}
