use super::*;

#[test]
fn malformed_layer_identity_cancels_real_numeric_preview_or_draft_once_without_retargeting() {
    for reverse in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
            for preview in [false, true] {
                for invalid in [
                    None,
                    Some(json!("7")),
                    Some(Value::Null),
                    Some(json!(-7)),
                    Some(json!(7.5)),
                    Some(json!({})),
                ] {
                    let (mut engine, catalogue) = fixture(reverse);
                    let before = engine.snapshot();
                    let could_undo = engine.can_undo();
                    let mut controls = controls(&catalogue);
                    let point =
                        start_effect(&mut controls, &mut atlas, &mut engine, locale, preview);
                    let mut broken = state(&engine, 6.5);
                    if let Some(invalid) = invalid {
                        broken["project"]["layers"][0]["id"] = invalid;
                    } else {
                        broken["project"]["layers"][0]
                            .as_object_mut()
                            .unwrap()
                            .remove("id");
                    }
                    let (_, actions) = draw(
                        &mut controls,
                        &mut atlas,
                        &broken,
                        &pointer([point[0] + 40., point[1]], false, true),
                        locale,
                    );
                    if preview {
                        assert!(matches!(actions.as_slice(), [Action::History(4)]));
                    } else {
                        assert!(
                            actions.is_empty(),
                            "a text draft must not cancel an unrelated engine transaction"
                        );
                    }
                    apply(&mut engine, actions);
                    assert_eq!(engine.snapshot(), before);
                    assert_eq!(engine.can_undo(), could_undo);
                    assert!(!engine.gesture_active() && controls.field.is_none());
                    assert!(draw(
                        &mut controls,
                        &mut atlas,
                        &broken,
                        &pointer(point, false, true),
                        locale
                    )
                    .1
                    .is_empty());
                    assert!(draw(
                        &mut controls,
                        &mut atlas,
                        &state(&engine, 6.5),
                        &pointer(point, false, true),
                        locale
                    )
                    .1
                    .is_empty());
                    assert_eq!(engine.snapshot(), before);
                }
            }
        }
    }
}
