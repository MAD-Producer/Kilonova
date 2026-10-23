use super::*;
use serde_json::json;
fn catalogue() -> Value {
    json!({"packages":[{"hash":"hash-a", "enabled":true, "manifest":{"id":"test.effects","version":"1.0.0",
        "effects":[{"id":"blur","name":"模糊","english_name":"Blur","renderer":"image"}]}}]})
}
fn state() -> Value {
    json!({"root":"project-a","composition":"main", "project":{"layers":[
        {"id":1,"name":"One","content":{"kind":"solid"}},
        {"id":2,"name":"Two","content":{"kind":"vector"}},
        {"id":3,"name":"Locked","locked":true,"content":{"kind":"solid"}},
        {"id":4,"name":"Audio","content":{"kind":"audio"}}
    ]}})
}
fn start(owner: Option<Panel>) -> Controls {
    let mut c = Controls::default();
    c.prepare(owner, &state(), &Input::default());
    c.begin(
        super::super::effect_controls::entries(&catalogue(), Locale::En).remove(0),
        [10., 10.],
        &state(),
    );
    c
}
fn frame(
    c: &mut Controls,
    owner: Option<Panel>,
    object: Option<u64>,
    ids: &[u64],
    state: &Value,
    catalogue: &Value,
    locale: Locale,
) -> (Vec<Action>, Option<String>) {
    c.prepare(owner, state, &Input::default());
    c.target(Rect::new(50., 50., 200., 22.), object);
    let mut out = vec![];
    let mut message = None;
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    c.finish(
        &mut PaintList::default(),
        &mut atlas,
        [400., 300.],
        state,
        ids,
        catalogue,
        locale,
        &mut out,
        &mut message,
        &mut None,
    );
    (out, message)
}
#[test]
fn click_threshold_and_passive_other_window_do_not_apply_or_finish_drag() {
    let mut c = start(None);
    c.pointer(None, [11., 11.], true, 3.);
    assert!(frame(
        &mut c,
        None,
        Some(1),
        &[1],
        &state(),
        &catalogue(),
        Locale::En
    )
    .0
    .is_empty());
    assert!(!c.pending());
    let mut c = start(None);
    c.pointer(Some(Panel::Timeline), [75., 60.], false, 3.);
    assert!(frame(
        &mut c,
        None,
        Some(1),
        &[1],
        &state(),
        &catalogue(),
        Locale::En
    )
    .0
    .is_empty());
    assert!(c.pending());
    c.pointer(Some(Panel::Timeline), [75., 60.], true, 3.);
    assert!(frame(
        &mut c,
        None,
        Some(1),
        &[1],
        &state(),
        &catalogue(),
        Locale::En
    )
    .0
    .is_empty());
    let (out, _) = frame(
        &mut c,
        Some(Panel::Timeline),
        Some(1),
        &[1, 2],
        &state(),
        &catalogue(),
        Locale::En,
    );
    assert!(
        matches!(out.as_slice(),[Action::Plugin(v)] if v["objects"]==json!([1,2]) && v["hash"]=="hash-a" && v["composition"]=="main")
    );
    assert!(!c.pending());
    assert!(frame(
        &mut c,
        Some(Panel::Timeline),
        Some(1),
        &[1, 2],
        &state(),
        &catalogue(),
        Locale::En
    )
    .0
    .is_empty());
}
#[test]
fn explicit_unselected_row_and_viewer_selection_have_distinct_targets() {
    for (target, expected) in [(Some(2), json!([2])), (None, json!([1]))] {
        let mut c = start(Some(Panel::Effects));
        c.pointer(None, [75., 60.], true, 3.);
        let (out, _) = frame(
            &mut c,
            None,
            target,
            &[1],
            &state(),
            &catalogue(),
            Locale::Zh,
        );
        assert!(matches!(out.as_slice(),[Action::Plugin(v)] if v["objects"]==expected));
    }
}
#[test]
fn locked_incompatible_empty_missing_and_changed_package_cancel_atomically() {
    for locale in [Locale::Zh, Locale::En] {
        for (ids, catalogue) in [
            (vec![1, 3], catalogue()),
            (vec![1, 4], catalogue()),
            (vec![], catalogue()),
            (vec![99], catalogue()),
            (vec![1], json!({"packages":[]})),
        ] {
            let mut c = start(None);
            c.pointer(None, [75., 60.], true, 3.);
            let (out, message) = frame(&mut c, None, None, &ids, &state(), &catalogue, locale);
            assert!(out.is_empty());
            assert!(message.is_some());
            assert!(!c.pending());
        }
        let mut c = start(None);
        c.pointer(None, [300., 200.], true, 3.);
        let (out, message) = frame(&mut c, None, Some(1), &[1], &state(), &catalogue(), locale);
        assert!(out.is_empty());
        assert!(message.is_some());
        assert!(!c.pending());
    }
}
#[test]
fn escape_focus_loss_and_project_composition_change_consume_without_edit() {
    for input in [
        Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        },
        Input {
            focus_lost: true,
            ..Default::default()
        },
    ] {
        let mut c = start(None);
        c.pointer(None, [75., 60.], false, 3.);
        assert!(c.prepare(None, &state(), &input));
        assert!(!c.pending());
    }
    for key in ["root", "composition"] {
        let mut c = start(None);
        let mut value = state();
        value[key] = json!("changed");
        assert!(c.prepare(None, &value, &Input::default()));
        assert!(!c.pending());
    }
}
