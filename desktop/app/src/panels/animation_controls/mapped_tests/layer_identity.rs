use super::*;

#[test]
fn malformed_layer_identity_cancels_real_easing_drag_once_and_preserves_undo_state() {
    for reverse in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
            for invalid in [None, Some(json!("7")), Some(json!(7.5))] {
                for focus_lost in [false, true] {
                    let mut engine = fixture(reverse);
                    seed_curve(&mut engine);
                    let before = engine.snapshot();
                    let could_undo = engine.can_undo();
                    let frame = projected(&engine, 3.);
                    let mut c = controls();
                    let s = state(&engine, frame);
                    assert!(draw(&mut c, &mut atlas, &s, &Input::default(), locale).is_empty());
                    let (_, graph, bounds) = c.mini_test_geometry().unwrap();
                    let point = [
                        graph.min[0] + bounds.x.fraction(0.5) as f32 * graph.width(),
                        graph.max[1] - bounds.y.fraction(0.25) as f32 * graph.height(),
                    ];
                    apply(
                        &mut engine,
                        draw(&mut c, &mut atlas, &s, &pointer(point, true, false), locale),
                    );
                    assert!(engine.gesture_active());
                    let moved = [point[0] + 25., point[1] - 15.];
                    let s = state(&engine, frame);
                    apply(
                        &mut engine,
                        draw(
                            &mut c,
                            &mut atlas,
                            &s,
                            &pointer(moved, false, false),
                            locale,
                        ),
                    );
                    assert_ne!(engine.snapshot(), before);
                    let mut broken = state(&engine, frame);
                    if let Some(invalid) = invalid.clone() {
                        broken["project"]["layers"][0]["id"] = invalid;
                    } else {
                        broken["project"]["layers"][0]
                            .as_object_mut()
                            .unwrap()
                            .remove("id");
                    }
                    let mut input = pointer(moved, false, true);
                    input.focus_lost = focus_lost;
                    let actions = draw(&mut c, &mut atlas, &broken, &input, locale);
                    assert!(matches!(actions.as_slice(), [Action::History(4)]));
                    apply(&mut engine, actions);
                    assert_eq!(engine.snapshot(), before);
                    assert_eq!(engine.can_undo(), could_undo);
                    assert!(!c.editing() && !engine.gesture_active());
                    assert!(c.user_key().is_none());
                    assert!(draw(
                        &mut c,
                        &mut atlas,
                        &broken,
                        &pointer(moved, false, true),
                        locale
                    )
                    .is_empty());
                    assert!(draw(
                        &mut c,
                        &mut atlas,
                        &state(&engine, frame),
                        &pointer(moved, false, true),
                        locale
                    )
                    .is_empty());
                    assert_eq!(engine.snapshot(), before);
                }
            }
        }
    }
}
