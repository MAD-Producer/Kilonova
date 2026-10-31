use super::*;
use motion_core::{CurveClipboard, CurveSpace};
use motion_model::keyframe_clipboard::curve_clipboard::MAX_CURVE_CLIPBOARD_BYTES;

fn payload() -> CurveClipboard {
    CurveClipboard::capture(
        fixture().project(),
        &selection(property(7, Property::Position, None), &[10]),
    )
    .unwrap()
}
fn command(clipboard: CurveClipboard, selections: Vec<KeyframeSelection>) -> Command {
    Command::PasteEasing {
        clipboard: Box::new(clipboard),
        selections,
    }
}

#[test]
fn versioned_easing_survives_process_independent_text_and_rejects_bad_units() {
    let clipboard = payload();
    let text = clipboard.to_text().unwrap();
    assert_eq!(CurveClipboard::from_text(&text).unwrap(), clipboard);
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["time_unit"], "normalized_interval");
    assert_eq!(json["ordinate_unit"], "normalized_progress");
    assert!(json.get("frames").is_none());
    assert!(json.get("values").is_none());
    for invalid in [
        "text".to_owned(),
        text.replace("\"version\":1", "\"version\":0"),
        text.replace("normalized_progress", "progress_per_normalized_time"),
        text.replacen('{', "{\"unexpected\":true,", 1),
        " ".repeat(MAX_CURVE_CLIPBOARD_BYTES + 1),
    ] {
        assert!(
            CurveClipboard::from_text(&invalid).is_err(),
            "invalid payload accepted"
        );
    }
    let mut velocity = clipboard.easing;
    velocity.curve.as_mut().unwrap().space = CurveSpace::Velocity;
    let velocity = CurveClipboard::new(velocity).unwrap();
    assert!(velocity
        .to_text()
        .unwrap()
        .contains("progress_per_normalized_time"));
    assert_eq!(
        CurveClipboard::from_text(&velocity.to_text().unwrap()).unwrap(),
        velocity
    );
}

#[test]
fn easing_paste_crosses_property_units_with_one_undo_and_unchanged_key_values_and_times() {
    let mut project = fixture().snapshot();
    project.layers[0].transform.opacity.keys.push(Keyframe {
        auto_bezier: false,
        frame: 18,
        value: 0.9,
        ease: Ease::Linear,
        curve: None,
        spatial: None,
    });
    let mut engine = Engine::new(project).unwrap();
    let before = engine.snapshot();
    engine
        .apply(command(
            payload(),
            vec![
                selection(property(7, Property::Opacity, None), &[15, 25]),
                selection(property(7, Property::Position, None), &[10, 20]),
            ],
        ))
        .unwrap();
    let after = engine.snapshot();
    let source = payload().easing;
    let opacity = &after.layers[0].transform.opacity.keys;
    assert_eq!(opacity[0].curve, source.curve);
    assert_eq!(opacity[0].ease, source.ease);
    assert_eq!(opacity[1], before.layers[0].transform.opacity.keys[1]);
    // Every field except the outgoing timing metadata must be byte-identical.
    let mut restored = after.clone();
    restored.layers[0].transform.opacity.keys[0].ease =
        before.layers[0].transform.opacity.keys[0].ease;
    restored.layers[0].transform.opacity.keys[0].curve =
        before.layers[0].transform.opacity.keys[0].curve;
    assert_eq!(restored, before);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
    engine.redo().unwrap();
    assert_eq!(engine.project(), &after);
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), engine.project()).unwrap();
    assert_eq!(motion_core::storage::load(root.path()).unwrap(), after);
}

#[test]
fn easing_paste_rejects_stale_terminal_or_locked_targets_without_partial_edits() {
    let mut engine = fixture();
    for (target, frame, cause) in [
        (property(7, Property::Opacity, None), 15, "no outgoing"),
        (property(9, Property::Position, None), 10, "does not exist"),
        (property(7, Property::Position, None), 11, "does not exist"),
    ] {
        rejected(
            &mut engine,
            command(
                payload(),
                vec![
                    selection(property(7, Property::Position, None), &[10]),
                    selection(target, &[frame]),
                ],
            ),
            if frame == 11 { "duplicate" } else { cause },
        );
    }
    let mut project = engine.snapshot();
    project.layers[1].locked = true;
    let mut engine = Engine::new(project).unwrap();
    rejected(
        &mut engine,
        command(
            payload(),
            vec![selection(property(9, Property::Position, None), &[40])],
        ),
        "locked",
    );
    assert!(CurveClipboard::capture(
        engine.project(),
        &selection(property(7, Property::Position, None), &[20])
    )
    .is_err());
    let mut locked_source = fixture().snapshot();
    locked_source.layers[0].locked = true;
    assert_eq!(
        CurveClipboard::capture(
            &locked_source,
            &selection(property(7, Property::Position, None), &[10])
        )
        .unwrap(),
        payload()
    );
}

#[test]
fn easing_paste_requires_an_explicit_separated_axis_and_preserves_other_axes() {
    let mut project = fixture().snapshot();
    for key in &mut project.layers[0].transform.position.keys {
        key.spatial = None;
    }
    project.layers[0].transform.position.separate().unwrap();
    let mut engine = Engine::new(project).unwrap();
    rejected(
        &mut engine,
        command(
            payload(),
            vec![selection(property(7, Property::Position, None), &[10])],
        ),
        "separated",
    );
    let before = engine.snapshot();
    let timing = CurveClipboard::new(motion_core::Easing {
        ease: Ease::Hold,
        curve: None,
    })
    .unwrap();
    engine
        .apply(command(
            timing,
            vec![selection(
                property(7, Property::Position, Some(Axis::Y)),
                &[10],
            )],
        ))
        .unwrap();
    let mut restored = engine.snapshot();
    let axes = restored.layers[0].transform.position.axes.as_mut().unwrap();
    assert_eq!(axes.y.keys[0].ease, Ease::Hold);
    axes.y.keys[0] = before.layers[0]
        .transform
        .position
        .axes
        .as_ref()
        .unwrap()
        .y
        .keys[0]
        .clone();
    assert_eq!(restored, before);
}
