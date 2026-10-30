use super::*;

#[test]
fn position_hints_exclude_offsets_sizes_and_radii() {
    let manifest: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/motion-effects/builtin-library/manifest.json"
    )))
    .unwrap();
    let parameter = |effect: &str, parameter: &str| {
        manifest["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == effect)
            .unwrap()["params"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == parameter)
            .unwrap()
            .clone()
    };
    for (effect, name) in [
        ("gradient_ramp", "start"),
        ("rays", "center"),
        ("corner_pin", "tl"),
    ] {
        assert!(is_position(&parameter(effect, name)), "{effect}.{name}");
    }
    for (effect, name) in [
        ("drop_shadow", "offset"),
        ("vignette", "radius"),
        ("shake", "translation"),
    ] {
        assert!(!is_position(&parameter(effect, name)), "{effect}.{name}");
    }
}

#[cfg(target_os = "windows")]
mod native;

#[test]
fn armed_pick_survives_window_transfer_but_cancels_on_escape_right_click_and_stale_source() {
    let state = serde_json::json!({"root":"pick", "document_generation":1, "revision":2,
        "frame":3., "composition":"main", "project":{"layers":[{"id":7,
            "effects":[{"id":8,"params":{"center":{}}}]}]}});
    let ticket = Ticket::new(&state, 7, 8, "center", Kind::Position);
    let mut pick = State {
        ticket: Some(ticket.clone()),
    };
    assert!(pick.prepare(
        &state,
        &Input {
            focus_lost: true,
            ..Default::default()
        }
    ));
    assert!(pick.ticket.is_some());
    for input in [
        Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        },
        Input {
            pressed: Some((MouseButton::Right, [0.; 2])),
            ..Default::default()
        },
    ] {
        pick.ticket = Some(ticket.clone());
        assert!(pick.prepare(&state, &input));
        assert!(pick.ticket.is_none());
    }
    for key in ["revision", "document_generation", "frame"] {
        let mut changed = state.clone();
        changed[key] = serde_json::json!(10);
        pick.ticket = Some(ticket.clone());
        assert!(pick.prepare(&changed, &Input::default()));
        assert!(pick.ticket.is_none(), "{key}");
    }
}

#[test]
fn effect_pick_owns_the_viewer_when_inspection_menu_was_open() {
    let state = serde_json::json!({"root":"pick", "document_generation":1, "surfaceEpoch":1,
        "revision":2, "frame":3., "composition":"main", "project":{"width":640,"height":480,
        "layers":[{"id":7,"effects":[{"id":8,"params":{"color":{}}}]}]}});
    let mut panels = Panels::default();
    panels.focused = Some(Panel::Composition);
    panels.viewer_inspection.open(false, [200., 300.], &state);
    panels.effect_pick.ticket = Some(Ticket::new(&state, 7, 8, "color", Kind::Color));
    let layout = dock::Node::dock(Panel::Composition.dock());
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let actions = super::super::chrome(&mut PaintList::default(), &mut atlas, [900., 600.],
        &layout, &mut panels, &state, &Input {
            mouse: [450., 300.], pressed: Some((MouseButton::Left, [450., 300.])), ..Default::default()
        });
    assert!(matches!(actions.as_slice(), [Action::EffectPick { .. }]));
    assert!(panels.effect_pick.ticket.is_none());
    assert_eq!(panels.viewer_inspection.frame(false, motion_render::ViewChannel::Color).region, None);
}
