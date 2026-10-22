use motion_model::{Axis, AxisTracks, Ease, Easing, Error, Keyframe, Track, Tween};

#[derive(Clone, Copy, Debug, PartialEq)]
struct DerivedVector([f32; 3]);

impl Tween for DerivedVector {
    fn mix(self, other: Self, t: f32) -> Self {
        Self(self.0.mix(other.0, t))
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
        Some(if self.0[0] == 42. {
            [42., f32::NAN, self.0[2]]
        } else {
            self.0
        })
    }
    fn from_components(value: [f32; 3]) -> Option<Self> {
        Some(Self(value))
    }
}

fn axes() -> Box<AxisTracks> {
    Box::new(AxisTracks {
        x: Track::constant(1.),
        y: Track::constant(2.),
        z: Track::constant(3.),
    })
}

fn nested() -> Track<[f32; 3]> {
    let mut track = Track::constant([1., 2., 3.]);
    track.axes = Some(axes());
    track.axes.as_mut().unwrap().y.axes = Some(axes());
    track
}

#[test]
fn upsert_rejects_nested_storage_without_leaving_the_first_axis_edited() {
    let mut track = nested();
    let before = format!("{track:?}");
    assert!(matches!(
        track.upsert(8, [4., 5., 6.], Ease::Out),
        Err(Error::Invalid(_))
    ));
    assert_eq!(format!("{track:?}"), before);
}

#[test]
fn set_at_rejects_nested_storage_without_leaving_the_first_axis_edited() {
    let mut track = nested();
    let before = format!("{track:?}");
    assert!(matches!(
        track.set_at(8, [4., 5., 6.]),
        Err(Error::Invalid(_))
    ));
    assert_eq!(format!("{track:?}"), before);
}

#[test]
fn whole_axis_edits_reject_scalar_owners_instead_of_mutating_invalid_storage() {
    let mut unexpected = Vec::new();
    for operation in 0..7 {
        let mut track = Track::constant(1.);
        track.axes = Some(axes());
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            let scalar = track.axes.as_mut().unwrap().get_mut(axis);
            scalar.keys = [1, 5]
                .map(|frame| Keyframe {
                    frame,
                    value: frame as f32,
                    ease: Ease::Linear,
                    curve: None,
                    spatial: None,
                })
                .into();
        }
        let before = format!("{track:?}");
        let result = match operation {
            0 => track.move_key(1, 3),
            1 => track.copy_key(1, 3),
            2 => track.delete_key(1),
            3 => track.set_ease(1, Ease::Out),
            4 => track.set_curve(
                1,
                Easing {
                    ease: Ease::InOut,
                    curve: None,
                },
            ),
            5 => track.set_animated(3, true),
            6 => track.set_animated(3, false),
            _ => unreachable!(),
        };
        if !matches!(result, Err(Error::Invalid(_))) || format!("{track:?}") != before {
            unexpected.push(operation);
        }
    }
    assert!(
        unexpected.is_empty(),
        "scalar owner accepted or modified by operations {unexpected:?}"
    );
}

#[test]
fn axis_mut_rejects_invalid_storage_before_exposing_an_editable_axis() {
    let mut track = Track::constant(1.);
    track.axes = Some(axes());
    let before = format!("{track:?}");
    assert!(matches!(track.axis_mut(Axis::X), Err(Error::Invalid(_))));
    assert_eq!(format!("{track:?}"), before);
}

#[test]
fn invalid_derived_components_do_not_partially_upsert_a_valid_separated_track() {
    let mut track = Track::constant(DerivedVector([1., 2., 3.]));
    track.separate().unwrap();
    let before = format!("{track:?}");
    assert!(matches!(
        track.upsert(8, DerivedVector([42., 5., 6.]), Ease::Out),
        Err(Error::Invalid(_))
    ));
    assert_eq!(format!("{track:?}"), before);
}

#[test]
fn invalid_derived_components_do_not_partially_set_a_valid_separated_track() {
    let mut track = Track::constant(DerivedVector([1., 2., 3.]));
    track.separate().unwrap();
    let before = format!("{track:?}");
    assert!(matches!(
        track.set_at(8, DerivedVector([42., 5., 6.])),
        Err(Error::Invalid(_))
    ));
    assert_eq!(format!("{track:?}"), before);
}

#[test]
fn invalid_derived_components_do_not_create_nonfinite_axis_storage() {
    let mut track = Track::constant(DerivedVector([42., 5., 6.]));
    let before = format!("{track:?}");
    assert!(matches!(track.separate(), Err(Error::Invalid(_))));
    assert_eq!(format!("{track:?}"), before);
}
