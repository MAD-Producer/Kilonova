use motion_model::{Ease, Keyframe, Track, Tween};

#[derive(Clone, Copy, Debug, PartialEq)]
struct IntegralVector([f32; 3]);
impl Tween for IntegralVector {
    fn mix(self, other: Self, t: f32) -> Self {
        Self(std::array::from_fn(|i| {
            (self.0[i] * (1.0 - t) + other.0[i] * t).round()
        }))
    }
    fn finite(self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }
    fn bounded(self, bound: f32) -> bool {
        self.0.iter().all(|v| v.is_finite() && v.abs() <= bound)
    }
    fn add(self, other: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + other.0[i]))
    }
    fn components(self) -> Option<[f32; 3]> {
        Some(self.0)
    }
    fn from_components(value: [f32; 3]) -> Option<Self> {
        value
            .iter()
            .all(|v| v.is_finite() && v.fract() == 0.0)
            .then_some(Self(value))
    }
}

fn integral_track() -> Track<IntegralVector> {
    let mut track = Track::constant(IntegralVector([0.0; 3]));
    for (frame, value) in [(0, 0.0), (10, 10.0)] {
        track.keys.push(Keyframe {
            frame,
            value: IntegralVector([value; 3]),
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        });
    }
    track
}

#[test]
fn validated_generic_separation_returns_native_error_for_unrepresentable_intermediate_value() {
    let mut track = integral_track();
    track.validate(20).unwrap();
    assert_eq!(track.sample(5.5).unwrap(), IntegralVector([6.0; 3]));
    track.separate().unwrap();
    track.validate(20).unwrap();
    let before = track.clone();
    let error = track.sample(5.5).unwrap_err();
    assert!(matches!(error, motion_model::Error::Invalid(_)));
    let message = error.to_string();
    for context in [
        "track sample",
        "phase=reconstruct",
        "frame=5.5",
        "IntegralVector",
        "cannot reconstruct sampled dimensions",
    ] {
        assert!(message.contains(context), "{context}: {message}");
    }
    assert_eq!(track, before);
    assert_eq!(track.sample(5.0).unwrap(), IntegralVector([5.0; 3]));
    assert_eq!(track.sample(10.0).unwrap(), IntegralVector([10.0; 3]));
    assert!(track.sample(5.5).is_err());
    assert_eq!(track, before);
}

#[test]
fn malformed_separated_storage_returns_errors_without_recursing_or_ignoring_vector_keys() {
    let mut vector = Track::constant([1.0, 2.0, 3.0]);
    vector.separate().unwrap();
    let mut scalar = Track::constant(7.0_f32);
    scalar.axes = vector.axes.clone();
    assert!(scalar
        .sample(0.0)
        .unwrap_err()
        .to_string()
        .contains("phase=reconstruct"));
    let mut nested = vector.clone();
    nested.axes.as_mut().unwrap().x.axes = vector.axes.clone();
    let before = nested.clone();
    assert!(nested
        .sample(0.0)
        .unwrap_err()
        .to_string()
        .contains("scalar axis tracks cannot contain separated dimensions"));
    assert_eq!(nested, before);
    assert_eq!(format!("{nested:?}"), format!("{before:?}"));
    vector.keys.push(Keyframe {
        frame: 0,
        value: [9.0; 3],
        ease: Ease::Linear,
        curve: None,
        spatial: None,
    });
    assert!(vector
        .sample(0.0)
        .unwrap_err()
        .to_string()
        .contains("separated tracks cannot contain vector keys"));
}

#[test]
fn valid_sampling_keeps_scalar_vector_easing_and_time_boundary_results() {
    let mut scalar = Track::constant(9.0_f32);
    scalar.upsert(0, 1.0, Ease::In).unwrap();
    scalar.upsert(10, 5.0, Ease::Linear).unwrap();
    for (frame, expected) in [
        (f64::NAN, 9.0),
        (f64::NEG_INFINITY, 1.0),
        (2.5, 1.25),
        (10.0, 5.0),
        (f64::INFINITY, 5.0),
    ] {
        assert_eq!(scalar.sample(frame).unwrap(), expected);
    }
    let mut vector = Track::constant([1.0, 2.0, 3.0]);
    vector.upsert(0, [10.0, 20.0, 30.0], Ease::Linear).unwrap();
    vector.upsert(10, [20.0, 30.0, 40.0], Ease::Linear).unwrap();
    let frames = [f64::NEG_INFINITY, -1.0, 0.0, 2.5, 5.5, 10.0, f64::INFINITY];
    let expected: Vec<_> = frames.iter().map(|&f| vector.sample(f).unwrap()).collect();
    vector.separate().unwrap();
    for (&frame, expected) in frames.iter().zip(expected) {
        assert_eq!(vector.sample(frame).unwrap(), expected);
    }
    assert_eq!(vector.sample(f64::NAN).unwrap(), [1.0, 2.0, 3.0]);
}

#[test]
fn raw_track_rejects_invalid_component_and_parameter_dimensions() {
    use motion_model::expressions::RawTrack;
    let raw = RawTrack::Component(Track::constant([1.0, 2.0, 3.0]), 3);
    assert!(raw
        .sample(5.0)
        .unwrap_err()
        .to_string()
        .contains("component=3"));
    let raw = RawTrack::Param(Track::constant([1.0, 2.0, 3.0, 4.0]), 5);
    assert!(raw
        .sample(5.0)
        .unwrap_err()
        .to_string()
        .contains("dimensions=5"));
    let raw = RawTrack::Component(Track::constant([1.0, 2.0, 3.0]), 1);
    assert_eq!(raw.sample(5.0).unwrap(), vec![2.0]);
}
