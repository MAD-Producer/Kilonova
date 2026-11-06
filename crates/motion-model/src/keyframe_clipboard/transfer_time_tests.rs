use super::*;
use crate::{layer_time::TimeStretch, Ease, SpatialTangents};

fn reversed() -> KeyframeTimeSpace {
    KeyframeTimeSpace::Local {
        clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 10.).unwrap(),
        offset_frame: 0,
    }
}
fn key(frame: i32, ease: Ease, auto_bezier: bool) -> Keyframe<[f32; 2]> {
    Keyframe {
        frame,
        value: [frame as f32, 17.],
        ease,
        auto_bezier,
        curve: None,
        spatial: Some(SpatialTangents {
            incoming: Some([1., 2.]),
            outgoing: Some([3., 4.]),
        }),
    }
}

#[test]
fn reverse_transfer_keeps_empty_and_single_key_terminal_metadata() {
    let empty: Vec<Keyframe<[f32; 2]>> = vec![];
    assert!(move_keys(
        &empty,
        KeyframeTimeSpace::Composition,
        reversed(),
        0.,
        None,
        12
    )
    .unwrap()
    .is_empty());
    let original = vec![key(2, Ease::Hold, true)];
    let before = original.clone();
    let moved = move_keys(
        &original,
        KeyframeTimeSpace::Composition,
        reversed(),
        0.,
        None,
        12,
    )
    .unwrap();
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].frame, 8);
    assert_eq!(moved[0].value, [2., 17.]);
    assert_eq!(moved[0].ease, Ease::Hold);
    assert!(moved[0].auto_bezier);
    let tangents = moved[0].spatial.as_ref().unwrap();
    assert_eq!(tangents.incoming, Some([3., 4.]));
    assert_eq!(tangents.outgoing, Some([1., 2.]));
    assert_eq!(original, before);
}

#[test]
fn reverse_transfer_uses_preceding_segment_policy_and_preserves_rejection() {
    let original = vec![
        key(1, Ease::In, false),
        key(4, Ease::Out, true),
        key(8, Ease::Linear, false),
    ];
    let before = original.clone();
    let moved = move_keys(
        &original,
        KeyframeTimeSpace::Composition,
        reversed(),
        0.,
        None,
        12,
    )
    .unwrap();
    assert_eq!(moved.iter().map(|k| k.frame).collect::<Vec<_>>(), [2, 6, 9]);
    assert_eq!(
        moved.iter().map(|k| k.value).collect::<Vec<_>>(),
        [[8., 17.], [4., 17.], [1., 17.]]
    );
    assert_eq!(
        moved.iter().map(|k| k.ease).collect::<Vec<_>>(),
        [Ease::In, Ease::Out, Ease::Linear]
    );
    assert_eq!(
        moved.iter().map(|k| k.auto_bezier).collect::<Vec<_>>(),
        [true, false, false]
    );
    assert_eq!(original, before);
    let unsupported = vec![key(1, Ease::Hold, false), key(4, Ease::Linear, false)];
    let unchanged = unsupported.clone();
    let error = move_keys(
        &unsupported,
        KeyframeTimeSpace::Composition,
        reversed(),
        0.,
        None,
        12,
    )
    .unwrap_err();
    assert!(matches!(error, Error::Invalid(ref cause)
        if cause == "reversed clipboard hold interpolation requires an opposite-sided step"));
    assert_eq!(unsupported, unchanged);
}
