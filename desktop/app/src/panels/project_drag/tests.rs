use super::*;
use motion_core::media_assets::AssetKind;

fn state() -> Value {
    serde_json::json!({"root":"project-drag","document_generation":7,"revision":3,"composition":"main"})
}
fn asset() -> AssetRef {
    AssetRef {
        kind: AssetKind::Image,
        id: 5,
    }
}
fn finish_drag(controls: Controls, state: &Value) -> (Controls, Vec<Action>) {
    let mut panels = Panels::default();
    panels.project_drag = controls;
    let mut list = PaintList::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut out = vec![];
    finish(
        &mut list,
        &mut atlas,
        [900., 600.],
        &mut panels,
        state,
        &mut out,
    );
    (panels.project_drag, out)
}
#[test]
fn asset_drag_uses_zoomed_scrolled_time_mapping_and_commits_only_once_in_destination_window() {
    for (from, to) in [
        (None, None),
        (None, Some(Panel::Timeline)),
        (Some(Panel::Project), None),
        (Some(Panel::Project), Some(Panel::Timeline)),
    ] {
        let state = state();
        let mut c = Controls::default();
        c.prepare(from, &state, &Input::default());
        c.begin(asset(), [30., 40.], &state);
        c.pointer(to, [600., 210.], true, 3.);
        c.prepare(to, &state, &Input::default());
        c.target(Rect::new(400., 100., 400., 300.), 120., 5., 300);
        let (c, out) = finish_drag(c, &state);
        assert!(
            matches!(out.as_slice(),[Action::ProjectPlace{source,target,at_frame:160}] if *source==KeyframeSource::of(&state)&&*target==asset()),
            "{out:?}"
        );
        assert!(!c.pending());
        let (_, out) = finish_drag(c, &state);
        assert!(out.is_empty());
    }
}
#[test]
fn asset_drag_click_escape_focus_stale_and_invalid_release_never_create_a_layer() {
    let state = state();
    for kind in ["click", "escape", "focus", "stale", "outside", "passive"] {
        let mut c = Controls::default();
        c.prepare(None, &state, &Input::default());
        c.begin(asset(), [30., 40.], &state);
        c.pointer(
            None,
            if kind == "click" {
                [30., 40.]
            } else {
                [50., 60.]
            },
            kind != "passive",
            3.,
        );
        let mut input = Input::default();
        let mut current = state.clone();
        match kind {
            "escape" => input.keys.push((Key::Escape, Default::default())),
            "focus" => input.focus_lost = true,
            "stale" => current["revision"] = 4.into(),
            _ => {}
        }
        c.prepare(None, &current, &input);
        if !matches!(kind, "outside" | "passive") {
            c.target(Rect::new(20., 20., 300., 200.), 0., 5., 300);
        }
        let (_, out) = finish_drag(c, &current);
        assert!(out.is_empty(), "{kind}: {out:?}");
    }
}

#[test]
fn model_drag_uses_existing_time_mapping_and_creates_only_one_placement_action() {
    let state = state();
    let mut c = Controls::default();
    c.prepare(Some(Panel::Project), &state, &Input::default());
    c.begin_model(19, [30., 40.], &state);
    c.pointer(Some(Panel::Timeline), [600., 210.], true, 3.);
    c.prepare(Some(Panel::Timeline), &state, &Input::default());
    c.target(Rect::new(400., 100., 400., 300.), 120., 5., 300);
    let (c, out) = finish_drag(c, &state);
    assert!(matches!(out.as_slice(), [Action::ProjectPlaceModel { source, asset: 19, at_frame: 160 }]
        if *source == KeyframeSource::of(&state)), "{out:?}");
    assert!(!c.pending());
    assert!(finish_drag(c, &state).1.is_empty());
}

#[test]
fn model_drag_cancellation_and_stale_document_never_create_a_layer() {
    let state = state();
    for kind in ["escape", "focus", "stale", "outside", "passive"] {
        let mut c = Controls::default();
        c.prepare(None, &state, &Input::default());
        c.begin_model(19, [30., 40.], &state);
        c.pointer(None, [50., 60.], kind != "passive", 3.);
        let mut input = Input::default();
        let mut current = state.clone();
        match kind {
            "escape" => input.keys.push((Key::Escape, Default::default())),
            "focus" => input.focus_lost = true,
            "stale" => current["revision"] = 4.into(),
            _ => {}
        }
        c.prepare(None, &current, &input);
        if !matches!(kind, "outside" | "passive") {
            c.target(Rect::new(20., 20., 300., 200.), 0., 5., 300);
        }
        assert!(finish_drag(c, &current).1.is_empty(), "{kind}");
    }
}
