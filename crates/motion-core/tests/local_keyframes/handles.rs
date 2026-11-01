use super::*;
use motion_core::{Command, CurveShape, Easing, KeyframeTarget, LocalKeyEdit, Property};

fn handle(frame: i32, incoming: bool, control: [f64; 2]) -> Command {
    Command::LocalKeyframes {
        target: KeyframeTarget::Property {
            object: 7,
            property: Property::Opacity,
            axis: None,
        },
        edit: LocalKeyEdit::Handle {
            frame,
            incoming,
            control,
        },
    }
}
fn controls(e: Easing) -> [[f64; 2]; 2] {
    let CurveShape::Cubic {
        control1, control2, ..
    } = e.curve.unwrap().shape
    else {
        panic!("cubic required")
    };
    [control1, control2]
}
fn animated(reverse: bool, automatic: bool) -> Engine {
    let mut p = fixture().snapshot();
    let l = &mut p.layers[0];
    l.time_mapping.as_mut().unwrap().clock = LayerClock::new(
        TimeStretch::new(3, 2).unwrap(),
        if reverse {
            TimeDirection::Reverse
        } else {
            TimeDirection::Forward
        },
        20.,
        0.,
    )
    .unwrap();
    let t = &mut l.transform.opacity;
    t.keys.clear();
    for (f, v) in [(-3, 0.1), (3, 0.2), (13, 0.8), (25, 0.9)] {
        t.upsert(f, v, Ease::Linear).unwrap();
    }
    if automatic {
        for f in [3, 13] {
            t.set_auto_bezier(f, true).unwrap();
        }
    }
    Engine::new(p).unwrap()
}

#[test]
fn local_handles_resolve_current_opposite_controls_in_both_clock_directions() {
    for reverse in [false, true] {
        let mut e = animated(reverse, false);
        let before = e.snapshot();
        for (incoming, point) in [(true, [0.77, 0.88]), (false, [0.24, 0.17])] {
            let cmd = handle(-3, incoming, point);
            assert!(!cmd.composition_transaction());
            let json = serde_json::to_value(&cmd).unwrap();
            assert_eq!(json["edit"]["frame"], -3);
            let cmd = serde_json::from_value(json).unwrap();
            e.apply(cmd).unwrap();
        }
        let t = &e.project().layers[0].transform.opacity;
        assert_eq!(
            controls(t.effective_easing(0).unwrap()),
            [[0.24, 0.17], [0.77, 0.88]]
        );
        let mut expected = before;
        expected.layers[0].transform.opacity = t.clone();
        assert_eq!(e.project(), &expected);
    }
}

#[test]
fn local_handle_freezes_only_automatic_owner_and_preserves_other_incident_interval() {
    for reverse in [false, true] {
        for incoming in [false, true] {
            let mut e = animated(reverse, true);
            let before = e.snapshot();
            let original = &before.layers[0].transform.opacity;
            let index = usize::from(incoming);
            let point = [[0.23, 0.13], [0.78, 0.92]][index];
            e.apply(handle(3, incoming, point)).unwrap();
            let t = &e.project().layers[0].transform.opacity;
            let now = controls(t.effective_easing(1).unwrap());
            assert_eq!(now[index], point);
            assert_eq!(
                now[1 - index],
                controls(original.effective_easing(1).unwrap())[1 - index]
            );
            assert!(!t.keys[1 + index].auto_bezier);
            assert!(t.keys[2 - index].auto_bezier);
            let frozen = if incoming { 2 } else { 0 };
            assert_eq!(
                t.effective_easing(frozen).unwrap(),
                original.effective_easing(frozen).unwrap()
            );
            for (a, b) in t.keys.iter().zip(&original.keys) {
                assert_eq!(
                    (a.frame, a.value, &a.spatial),
                    (b.frame, b.value, &b.spatial)
                );
            }
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
        }
    }
}

#[test]
fn local_handle_gesture_cancel_atomic_failure_undo_redo_and_save_preserve_full_project() {
    for reverse in [false, true] {
        let mut e = animated(reverse, true);
        let before = e.snapshot();
        for commit in [false, true] {
            e.begin_gesture().unwrap();
            for point in [[0.2, 0.1], [0.3, 0.2]] {
                e.apply(handle(3, false, point)).unwrap();
            }
            let preview = e.snapshot();
            for cmd in [
                handle(4, false, [0.2, 0.]),
                handle(25, false, [0.2, 0.]),
                handle(3, false, [1.1, 0.]),
                handle(3, false, [0.2, f64::NAN]),
            ] {
                assert!(e.apply(cmd).is_err());
                assert_eq!(e.project(), &preview);
            }
            e.end_gesture(commit).unwrap();
            if !commit {
                assert_eq!(e.project(), &before);
                assert!(!e.can_undo());
                continue;
            }
            let after = e.snapshot();
            let root = tempfile::tempdir().unwrap();
            motion_core::storage::save(root.path(), &after).unwrap();
            let reopened = motion_core::storage::load(root.path()).unwrap();
            assert_eq!(after, reopened);
            for n in 0..=400 {
                let f = -3. + f64::from(n) / 10.;
                assert_eq!(
                    after.layers[0].transform.opacity.sample(f).unwrap(),
                    reopened.layers[0].transform.opacity.sample(f).unwrap()
                );
            }
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
            assert!(!e.can_undo());
            e.redo().unwrap();
            assert_eq!(e.project(), &after);
        }
    }
}
