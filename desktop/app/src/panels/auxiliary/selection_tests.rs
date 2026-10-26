use super::*;

fn malformed_layers() -> Vec<Value> {
    vec![
        serde_json::json!({}),
        Value::Null,
        serde_json::json!({"id": null}),
        serde_json::json!({"id": "7"}),
        serde_json::json!({"id": -1}),
        serde_json::json!({"id": 7.0}),
        serde_json::json!({"id": false}),
        serde_json::json!({"id": []}),
    ]
}

fn state(layers: Vec<Value>) -> Value {
    serde_json::json!({
        "root": "empty-selection-fixture",
        "composition": motion_core::composition::MAIN_COMPOSITION,
        "document_generation": 1,
        "revision": 0,
        "frame": 0,
        "project": {"layers": layers},
    })
}

#[test]
fn empty_selection_never_matches_missing_or_malformed_layer_ids() {
    let panels = Panels::default();
    assert!(panels.selected.is_none());
    for malformed in malformed_layers() {
        let snapshot = state(vec![malformed, serde_json::json!({"id": 7})]);
        assert!(selected(&panels, &snapshot).is_none(), "{snapshot}");
    }
}

#[test]
fn explicit_selection_keeps_first_valid_identity_and_missing_selection_stays_empty() {
    let mut panels = Panels::default();
    panels.select_only(Some(7));
    for malformed in malformed_layers() {
        let snapshot = state(vec![
            malformed.clone(),
            serde_json::json!({"id": 9}),
            serde_json::json!({"id": 7, "name": "first"}),
            serde_json::json!({"id": 7, "name": "second"}),
        ]);
        assert_eq!(selected(&panels, &snapshot).unwrap()["name"], "first");
        assert!(selected(&panels, &state(vec![malformed])).is_none());
    }
    assert!(selected(&panels, &Value::Null).is_none());
}

#[test]
fn effect_controls_with_no_selection_render_the_empty_state_for_malformed_layers() {
    for locale in [Locale::En, Locale::Zh] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        let mut draw = |snapshot: &Value| {
            let mut panels = Panels { locale, ..Default::default() };
            let mut paint = PaintList::default();
            paint.stylesheet = panels.styles.clone();
            let mut actions = Vec::new();
            super::super::effects::render(
                Panel::EffectControls,
                &mut paint,
                &mut atlas,
                Rect::new(20., 20., 360., 560.),
                &mut panels,
                snapshot,
                &Input::default(),
                &mut actions,
            );
            assert!(actions.is_empty(), "unselected content emitted {actions:?}");
            assert!(panels.selected.is_none());
            assert!(!paint.glyphs.is_empty(), "empty selection needs a visible prompt");
            format!("{:?}", (paint.shapes, paint.colors, paint.glyphs, paint.order))
        };
        let expected = draw(&state(Vec::new()));
        for malformed in malformed_layers() {
            let snapshot = state(vec![malformed, serde_json::json!({"id": 7})]);
            assert_eq!(draw(&snapshot), expected);
        }
    }
}
