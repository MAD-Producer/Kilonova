//! Exercise real numeric providers and core transactions, not only the guard.
use super::*;

fn changed(state: &Value, case: &str) -> Value {
    let mut next = state.clone();
    match case {
        "playhead" => next["frame"] = json!(11.),
        "root" => next["root"] = json!("another-document"),
        "composition" => next["composition"] = json!("another-composition"),
        "generation" => next["document_generation"] = json!(2),
        "offset" => next["project"]["layers"][0]["timeline"]["offset_frame"] = json!(3),
        "clock" => next["project"]["layers"][0]["time_mapping"]["clock"]["local_pivot"] = json!(3.),
        "unmap" => next["project"]["layers"][0]["time_mapping"] = Value::Null,
        _ => panic!("unknown case {case}"),
    }
    next
}
const CHANGES: [&str; 7] = [
    "playhead",
    "root",
    "composition",
    "generation",
    "offset",
    "clock",
    "unmap",
];
mod faults;
mod layer_identity;
fn midpoint(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn start_effect(
    controls: &mut Controls,
    atlas: &mut TextAtlas,
    engine: &mut Engine,
    locale: Locale,
    preview: bool,
) -> [f32; 2] {
    let s = state(engine, 6.5);
    draw(controls, atlas, &s, &Input::default(), locale);
    let point = midpoint(controls.test_rects[&(7, 1, "p0001".into(), 0)]);
    apply(
        engine,
        draw(controls, atlas, &s, &pointer(point, true, false), locale).1,
    );
    if preview {
        for distance in [20., 30.] {
            let s = state(engine, 6.5);
            apply(
                engine,
                draw(
                    controls,
                    atlas,
                    &s,
                    &pointer([point[0] + distance, point[1]], false, false),
                    locale,
                )
                .1,
            );
        }
        assert!(engine.gesture_active());
    } else {
        apply(
            engine,
            draw(controls, atlas, &s, &pointer(point, false, true), locale).1,
        );
        assert!(controls.field.as_ref().unwrap().number.editor.text.active());
    }
    point
}

#[test]
fn effect_time_changes_cancel_previews_once_and_never_retarget_another_key() {
    for reverse in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            for case in CHANGES {
                let (mut engine, catalogue) = fixture(reverse);
                let before = engine.snapshot();
                let mut controls = controls(&catalogue);
                let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
                let point = start_effect(&mut controls, &mut atlas, &mut engine, locale, true);
                assert_ne!(engine.snapshot(), before);
                let stale = changed(&state(&engine, 6.5), case);
                let (_, actions) = draw(
                    &mut controls,
                    &mut atlas,
                    &stale,
                    &pointer([point[0] + 40., point[1]], false, true),
                    locale,
                );
                assert!(
                    matches!(actions.as_slice(), [Action::History(4)]),
                    "{reverse} {locale:?} {case}: {actions:?}"
                );
                apply(&mut engine, actions);
                assert_eq!(engine.snapshot(), before, "{case}");
                assert!(!engine.gesture_active() && !engine.can_undo());
                assert!(controls.field.is_none());
                assert!(draw(
                    &mut controls,
                    &mut atlas,
                    &stale,
                    &pointer(point, false, true),
                    locale
                )
                .1
                .is_empty());
            }
        }
    }
}

#[test]
fn effect_text_outside_commit_cannot_bypass_the_captured_time() {
    for reverse in [false, true] {
        for case in CHANGES {
            for owner in [None, Some(motion_ui::dock::Panel::EffectControls)] {
                let (mut engine, catalogue) = fixture(reverse);
                let before = engine.snapshot();
                let mut controls = controls(&catalogue);
                controls.context_window = owner;
                let mut atlas = TextAtlas::from_system(12.).unwrap();
                start_effect(&mut controls, &mut atlas, &mut engine, Locale::En, false);
                let mut actions = vec![];
                assert!(!controls.numeric_outside(
                    &changed(&state(&engine, 6.5), case),
                    &pointer([800., 740.], true, false),
                    owner,
                    &mut actions
                ));
                assert!(
                    actions.is_empty(),
                    "unstarted draft must not close another transaction"
                );
                assert!(controls.field.is_none());
                assert_eq!(engine.snapshot(), before);
                assert!(!engine.can_undo() && !engine.gesture_active());
            }
        }
    }
}

fn property_state(engine: &Engine) -> Value {
    let mut s = state(engine, 6.5);
    s["sampledLayers"] = json!([{"id":7,"position":[20.,32.,0.]}]);
    s
}
fn start_property(panels: &mut Panels, atlas: &mut TextAtlas, engine: &mut Engine, preview: bool) {
    let mut paint = PaintList::default();
    let mut actions = vec![];
    panels::properties::draw_value(
        &mut paint,
        atlas,
        Rect::new(90., 20., 120., 30.),
        panels,
        &property_state(engine),
        &pointer([100., 35.], true, false),
        7,
        Property::Position,
        0,
        20.,
        false,
        false,
        &mut actions,
    );
    assert!(panels.properties.field.is_some());
    assert!(actions.is_empty());
    if preview {
        panels::properties::process_field(
            panels,
            &property_state(engine),
            &pointer([130., 35.], false, false),
            &mut actions,
        );
        apply(engine, actions);
        assert!(engine.gesture_active());
    } else {
        panels::properties::process_field(
            panels,
            &property_state(engine),
            &pointer([100., 35.], false, true),
            &mut actions,
        );
        assert!(actions.is_empty());
        let field = panels.properties.field.as_mut().unwrap();
        field.text = "88".into();
        field.replace = false;
    }
}

#[test]
fn transform_time_changes_cancel_scrub_or_text_without_writing_at_the_new_playhead() {
    for reverse in [false, true] {
        for preview in [false, true] {
            for case in CHANGES {
                let (mut engine, _) = fixture(reverse);
                let before = engine.snapshot();
                let mut panels = Panels {
                    selected: Some(7),
                    ..Default::default()
                };
                let mut atlas = TextAtlas::from_system(12.).unwrap();
                start_property(&mut panels, &mut atlas, &mut engine, preview);
                let mut out = vec![];
                let s = changed(&property_state(&engine), case);
                panels::properties::process_field(
                    &mut panels,
                    &s,
                    &Input {
                        released: Some((MouseButton::Left, [160., 35.])),
                        mouse: [160., 35.],
                        keys: vec![(Key::Enter, Default::default())],
                        ..Default::default()
                    },
                    &mut out,
                );
                assert_eq!(out.len(), usize::from(preview), "{case}");
                assert!(out.iter().all(|a| matches!(a, Action::History(4))));
                apply(&mut engine, out);
                assert_eq!(engine.snapshot(), before, "{case}");
                assert!(panels.properties.field.is_none());
                assert!(!engine.gesture_active() && !engine.can_undo());
                let mut repeated = vec![];
                panels::properties::process_field(
                    &mut panels,
                    &s,
                    &Input::default(),
                    &mut repeated,
                );
                assert!(repeated.is_empty());
            }
        }
    }
}

#[test]
fn transform_preview_revisions_do_not_cancel_their_own_gesture_and_commit_undoes_once() {
    for reverse in [false, true] {
        let (mut engine, _) = fixture(reverse);
        let before = engine.snapshot();
        let mut panels = Panels {
            selected: Some(7),
            ..Default::default()
        };
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        start_property(&mut panels, &mut atlas, &mut engine, true);
        for (x, released) in [(140., false), (150., true)] {
            let mut actions = vec![];
            panels::properties::process_field(
                &mut panels,
                &property_state(&engine),
                &pointer([x, 35.], false, released),
                &mut actions,
            );
            assert!(!actions.iter().any(|a| matches!(a, Action::History(4))));
            apply(&mut engine, actions);
        }
        let mut expected = before.clone();
        expected.layers[0].transform.position.keys[1].value[0] = 45.;
        assert_eq!(engine.snapshot(), expected);
        assert!(!engine.gesture_active());
        assert!(engine.undo().unwrap());
        assert_eq!(engine.snapshot(), before);
        assert!(!engine.undo().unwrap());
    }
}

#[test]
fn numeric_time_compares_the_clock_not_only_the_resulting_local_sample() {
    let (engine, _) = fixture(false);
    let s = state(&engine, 6.5);
    let guard = property_authoring::NumericTime::read(&s, 7).unwrap();
    let mut next = s.clone();
    next["project"]["layers"][0]["timeline"]["offset_frame"] = json!(5);
    next["frame"] = json!(9.5);
    assert_eq!(
        property_authoring::local(&s, 7, 6.5).unwrap(),
        property_authoring::local(&next, 7, 9.5).unwrap()
    );
    assert!(!property_authoring::NumericTime::current(&guard, &next, 7).unwrap());
    next = s.clone();
    next["revision"] = json!(9999);
    assert!(property_authoring::NumericTime::current(&guard, &next, 7).unwrap());
    for key in ["root", "composition", "document_generation", "frame"] {
        let mut bad = s.clone();
        bad[key] = Value::Null;
        if key == "composition" {
            bad["project"]["composition_id"] = Value::Null;
        }
        assert!(
            property_authoring::NumericTime::current(&guard, &bad, 7).is_err(),
            "{key}"
        );
    }
    let mut legacy = s.clone();
    legacy["project"]["layers"][0]["time_mapping"] = Value::Null;
    let unguarded = property_authoring::NumericTime::read(&legacy, 7).unwrap();
    assert!(unguarded.is_none());
    assert!(!property_authoring::NumericTime::current(&unguarded, &s, 7).unwrap());
}
