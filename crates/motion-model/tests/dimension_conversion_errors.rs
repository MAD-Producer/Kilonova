use motion_model::{AxisTracks, Ease, Error, Keyframe, Track, Tween};

#[derive(Clone, Copy, Debug, PartialEq)]
struct PartialVector([f32; 3]);

impl Tween for PartialVector {
    fn mix(self, other: Self, t: f32) -> Self {
        Self(std::array::from_fn(|i| self.0[i].mix(other.0[i], t)))
    }
    fn finite(self) -> bool {
        self.0.finite()
    }
    fn bounded(self, bound: f32) -> bool {
        self.0.bounded(bound)
    }
    fn add(self, other: Self) -> Self {
        Self(self.0.add(other.0))
    }
    fn components(self) -> Option<[f32; 3]> {
        (self.0[0] >= 0.).then_some(self.0)
    }
    fn from_components(value: [f32; 3]) -> Option<Self> {
        (value[2] != 999.).then_some(Self(value))
    }
}

fn key<T>(frame: i32, value: T) -> Keyframe<T> {
    Keyframe {
        frame,
        value,
        ease: Ease::InOut,
        curve: None,
        spatial: None,
    }
}

fn axes() -> Box<AxisTracks> {
    Box::new(AxisTracks {
        x: Track::constant(1.),
        y: Track::constant(2.),
        z: Track::constant(3.),
    })
}

#[test]
fn per_key_component_failure_returns_error_without_partial_conversion() {
    let mut track = Track::constant(PartialVector([1., 2., 3.]));
    track.keys = vec![
        key(-10, PartialVector([4., 5., 6.])),
        key(20, PartialVector([-1., 8., 9.])),
    ];
    let before = format!("{track:?}");
    let error = track.separate().unwrap_err();
    assert!(matches!(&error, Error::Invalid(_)));
    assert!(error.to_string().contains("key index=1 frame=20"));
    assert_eq!(format!("{track:?}"), before);
}

#[test]
fn existing_scalar_dimensions_are_rejected_instead_of_accepted_as_idempotent() {
    let mut track = Track::constant(1.);
    track.axes = Some(axes());
    let before = format!("{track:?}");
    assert!(matches!(track.separate(), Err(Error::Invalid(_))));
    assert_eq!(format!("{track:?}"), before);
}

#[test]
fn unsorted_keys_are_rejected_before_axis_storage_is_created() {
    let mut track = Track::constant([1., 2., 3.]);
    track.keys = vec![key(20, [4., 5., 6.]), key(-10, [7., 8., 9.])];
    let before = format!("{track:?}");
    assert!(matches!(track.separate(), Err(Error::Invalid(_))));
    assert_eq!(format!("{track:?}"), before);
}

#[test]
fn unsupported_reconstruction_does_not_create_unsampleable_axes() {
    let mut track = Track::constant(PartialVector([1., 2., 999.]));
    let before = format!("{track:?}");
    assert!(matches!(track.separate(), Err(Error::Invalid(_))));
    assert_eq!(format!("{track:?}"), before);
}

#[test]
fn every_key_must_support_reconstruction_before_any_axis_is_committed() {
    let mut track = Track::constant(PartialVector([1., 2., 3.]));
    track.keys = vec![
        key(-10, PartialVector([4., 5., 6.])),
        key(20, PartialVector([7., 8., 999.])),
    ];
    let before = format!("{track:?}");
    let error = track.separate().unwrap_err();
    assert!(matches!(&error, Error::Invalid(_)));
    assert!(error.to_string().contains("key index=1 frame=20"));
    assert_eq!(format!("{track:?}"), before);
}

#[test]
fn malformed_separated_vector_storage_remains_unchanged() {
    for case in 0..3 {
        let mut track = Track::constant([1., 2., 3.]);
        track.axes = Some(axes());
        match case {
            0 => track.keys.push(key(0, [4., 5., 6.])),
            1 => track.axes.as_mut().unwrap().y.axes = Some(axes()),
            2 => track.axes.as_mut().unwrap().z.keys = vec![key(2, 4.), key(-2, 5.)],
            _ => unreachable!(),
        }
        let before = format!("{track:?}");
        assert!(matches!(track.separate(), Err(Error::Invalid(_))));
        assert_eq!(format!("{track:?}"), before);
    }
}

#[test]
fn healthy_signed_keys_preserve_axis_metadata_samples_and_idempotence() {
    let mut track = Track::constant([1., 2., 3.]);
    track.keys = vec![key(-10, [4., 5., 6.]), key(20, [7., 8., 9.])];
    let before = track.clone();
    let frames = [-20., -10., -3.25, 0., 8.75, 20., 30.];
    let expected = frames.map(|frame| before.sample(frame).unwrap());
    track.separate().unwrap();
    assert!(track.keys.is_empty());
    assert_eq!(track.static_value().unwrap(), before.value);
    let axes = track.axes.as_ref().unwrap();
    for (index, axis) in [&axes.x, &axes.y, &axes.z].into_iter().enumerate() {
        assert_eq!(axis.value, before.value[index]);
        assert!(axis.axes.is_none());
        for (key, original) in axis.keys.iter().zip(&before.keys) {
            assert_eq!(key.frame, original.frame);
            assert_eq!(key.value, original.value[index]);
            assert_eq!(key.ease, original.ease);
            assert_eq!(key.curve, original.curve);
            assert!(key.spatial.is_none());
        }
        assert_eq!(axis.keys.len(), before.keys.len());
    }
    assert_eq!(frames.map(|frame| track.sample(frame).unwrap()), expected);
    let separated = format!("{track:?}");
    track.separate().unwrap();
    assert_eq!(format!("{track:?}"), separated);
    track.merge_dimensions().unwrap();
    assert_eq!(track.value, before.value);
    assert_eq!(track.keys, before.keys);
    assert!(track.axes.is_none());
    assert_eq!(frames.map(|frame| track.sample(frame).unwrap()), expected);
}
