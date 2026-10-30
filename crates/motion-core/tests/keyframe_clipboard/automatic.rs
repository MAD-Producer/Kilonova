use super::*;

fn animated() -> Engine {
    let mut p = fixture().snapshot();
    let track = &mut p.layers[0].transform.opacity;
    track.keys.clear();
    for (frame, value) in [(0, 0.), (10, 0.2), (40, 0.9), (70, 1.)] {
        track.upsert(frame, value, Ease::Linear).unwrap();
    }
    Engine::new(p).unwrap()
}

fn selected(frames: &[u32]) -> KeyframeSelection {
    selection(property(7, Property::Opacity, None), frames)
}

#[test]
fn automatic_type_is_local_offset_aware_undoable_and_persisted_without_replacing_keys() {
    let mut e = animated();
    let before = e.snapshot();
    let samples: Vec<_> = (0..701)
        .map(|i| before.layers[0].transform.opacity.sample(i as f64 / 10.).unwrap())
        .collect();
    e.apply(Command::KeyframeAutomatic { selections: vec![selected(&[17])], enabled: true }).unwrap();
    let after = e.snapshot();
    let keys = &after.layers[0].transform.opacity.keys;
    assert_eq!(keys.iter().filter(|k| k.auto_bezier).map(|k| k.frame).collect::<Vec<_>>(), [10]);
    let mut expected = before.clone();
    expected.layers[0].transform.opacity.keys[1].auto_bezier = true;
    assert_eq!(after, expected);
    assert_ne!(samples[45], after.layers[0].transform.opacity.sample(4.5).unwrap());
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
    e.redo().unwrap();
    assert_eq!(e.project(), &after);
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), &after).unwrap();
    let reopened = motion_core::storage::load(root.path()).unwrap();
    assert_eq!(reopened, after);
    for i in 0..701 {
        assert_eq!(reopened.layers[0].transform.opacity.sample(i as f64 / 10.).unwrap(),
            after.layers[0].transform.opacity.sample(i as f64 / 10.).unwrap());
    }
    e.apply(Command::KeyframeAutomatic { selections: vec![selected(&[17])], enabled: false }).unwrap();
    for i in 0..701 {
        assert_eq!(e.project().layers[0].transform.opacity.sample(i as f64 / 10.).unwrap(),
            after.layers[0].transform.opacity.sample(i as f64 / 10.).unwrap());
    }
}

#[test]
fn later_unsupported_or_locked_target_rejects_the_whole_command_without_history() {
    let mut e = animated();
    let before = e.snapshot();
    // This vector carries spatial handles; the preceding scalar is supported.
    let error = e.apply(Command::KeyframeAutomatic {
        selections: vec![selected(&[17]), selection(property(7, Property::Position, None), &[10])],
        enabled: true,
    }).unwrap_err();
    assert!(error.to_string().contains("manual spatial"), "{error}");
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
    let mut p = before.clone();
    p.layers[1].locked = true;
    let mut e = Engine::new(p).unwrap();
    let before = e.snapshot();
    assert!(e.apply(Command::KeyframeAutomatic {
        selections: vec![selected(&[17]), selection(property(9, Property::Position, None), &[40])],
        enabled: true,
    }).is_err());
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
}

#[test]
fn quick_ease_converts_only_selected_key_and_preserves_automatic_neighbour() {
    let mut e = animated();
    e.apply(Command::KeyframeAutomatic { selections: vec![selected(&[17,47])], enabled: true }).unwrap();
    let before = e.snapshot();
    e.apply(Command::EaseKeyframes { selections: vec![selected(&[17])], incoming: true, outgoing: true }).unwrap();
    let track = &e.project().layers[0].transform.opacity;
    assert!(!track.keys[1].auto_bezier);
    assert!(track.keys[2].auto_bezier);
    assert_eq!(track.effective_easing(0).unwrap().sample(1.).velocity, 0.);
    assert_eq!(track.effective_easing(1).unwrap().sample(0.).velocity, 0.);
    let outgoing = track.effective_easing(2).unwrap();
    let mut changed = track.clone();
    changed.set_at(70, 0.95).unwrap();
    assert_ne!(outgoing, changed.effective_easing(2).unwrap());
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
}

#[test]
fn speed_profile_and_portable_easing_copy_use_the_current_derived_curve() {
    use motion_core::{CurveClipboard, timing::{self, VelocityUnits}};
    let mut e = animated();
    e.apply(Command::KeyframeAutomatic { selections: vec![selected(&[17])], enabled: true }).unwrap();
    let track = &e.project().layers[0].transform.opacity;
    let expected = track.effective_easing(1).unwrap();
    let copied = CurveClipboard::capture(e.project(), &selected(&[17])).unwrap();
    assert_eq!(copied.easing, expected);
    let profile = timing::profile(e.project(), &[selected(&[17])], VelocityUnits::AverageMultiple).unwrap();
    assert_eq!(profile.outgoing.speed, Some(expected.sample(0.).velocity));
    assert_eq!(profile.outgoing.influence, Some((1. / 3.) * 100.));
}

#[test]
fn a_single_automatic_key_can_be_copied_and_cut_without_inventing_neighbours() {
    let mut e = animated();
    e.apply(Command::KeyframeAutomatic { selections: vec![selected(&[17])], enabled: true }).unwrap();
    let original = e.snapshot();
    let clipboard = KeyframeClipboard::capture(e.project(), &[selected(&[17])]).unwrap();
    let KeyframeData::Scalar(keys) = &clipboard.tracks[0].data else { panic!() };
    assert_eq!(keys.len(), 1);
    assert!(keys[0].auto_bezier);
    e.apply(paste(clipboard.clone(), vec![selected(&[]).target], Some(67))).unwrap();
    let track = &e.project().layers[0].transform.opacity;
    assert!(track.keys.iter().find(|k| k.frame == 60).unwrap().auto_bezier);
    e.undo().unwrap();
    assert_eq!(e.project(), &original);
    e.apply(Command::DeleteKeyframes { selections: vec![selected(&[7,47,77])] }).unwrap();
    let track = &e.project().layers[0].transform.opacity;
    assert_eq!(track.keys.len(), 1);
    assert_eq!(track.keys[0].frame, 10);
    assert!(!track.keys[0].auto_bezier);
    assert_eq!(track.sample(40.).unwrap(), 0.2);
    e.undo().unwrap();
    assert_eq!(e.project(), &original);
    e.apply(Command::RemoveKeyframes { clipboard: Box::new(clipboard) }).unwrap();
    assert!(e.project().layers[0].transform.opacity.keys.iter().all(|k| k.frame != 10));
    e.undo().unwrap();
    assert_eq!(e.project(), &original);
}
