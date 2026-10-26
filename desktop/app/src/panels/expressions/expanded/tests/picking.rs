use super::*;
use motion_core::ProjectExpressions;

pub(super) fn linked_fixture() -> (tempfile::TempDir, engine::Engine, Value, ExpressionTarget) {
    let (root, host, _, target) = fixture();
    drop(host);
    let mut project = motion_core::storage::load(root.path()).unwrap();
    let mut control =
        motion_core::Layer::solid(8, "Reference control", [120.; 2], [12., 24., 0.], [1.; 4]);
    control.transform.scale.value = [150., 75., 100.];
    project.layers.push(control);
    motion_core::storage::save(root.path(), &project).unwrap();
    let host = engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let state = host.state().unwrap();
    (root, host, state, target)
}

fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn press(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
fn release(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        released: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn source(panels: &Panels) -> String {
    panels
        .expressions
        .expanded
        .as_ref()
        .unwrap()
        .source
        .text
        .clone()
}
fn begin(panels: &mut Panels, state: &Value, atlas: &mut TextAtlas, floating: bool) {
    let point = center(
        panels
            .expressions
            .expanded
            .as_ref()
            .unwrap()
            .reference_button
            .unwrap(),
    );
    assert!(draw(panels, state, atlas, press(point), floating).is_empty());
    assert!(super::super::super::picking::active(
        panels,
        floating.then_some(Panel::Timeline)
    ));
}
fn destination(panels: &Panels) -> [f32; 2] {
    center(
        panels
            .expressions
            .triggers
            .iter()
            .find(|(target, _)| {
                *target
                    == ExpressionTarget::Property {
                        object: 8,
                        property: Property::Scale,
                        axis: None,
                    }
            })
            .unwrap()
            .1,
    )
}

#[test]
fn reference_drag_inserts_only_source_and_apply_uses_one_real_transaction_in_both_windows_and_locales(
) {
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let (_root, host, state, target) = linked_fixture();
            let mut panels = Panels::default();
            panels.locale = locale;
            panels.selected = Some(7);
            panels.focused = Some(Panel::Timeline);
            panels.timeline.expanded.insert(7);
            panels.timeline.expanded.insert(8);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            panels
                .expressions
                .open_expanded(&state, target, Panel::Timeline);
            draw(&mut panels, &state, &mut atlas, Input::default(), floating);
            replace(&mut panels, &state, &mut atlas, "value + [1,2,3]", floating);
            draw(&mut panels, &state, &mut atlas, key(Key::A, true), floating);
            begin(&mut panels, &state, &mut atlas, floating);
            let point = destination(&panels);
            draw(
                &mut panels,
                &state,
                &mut atlas,
                Input {
                    mouse: point,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                floating,
            );
            let mut end = release(point);
            end.keys = vec![
                (Key::Space, Default::default()),
                (Key::Delete, Default::default()),
            ];
            assert!(draw(&mut panels, &state, &mut atlas, end, floating).is_empty());
            let expected = motion_core::expression_reference_source(&ExpressionTarget::Property {
                object: 8,
                property: Property::Scale,
                axis: None,
            })
            .unwrap();
            assert_eq!(source(&panels), expected);
            assert_eq!(host.state().unwrap()["project"], state["project"]);
            // The resumed popup consumes the release; application is explicit.
            let actions = draw(
                &mut panels,
                &state,
                &mut atlas,
                key(Key::Enter, true),
                floating,
            );
            let (ticket, reply) = execute(&host, actions);
            let applied = reply.as_ref().unwrap();
            panels
                .expressions
                .expression_response(&ticket, &reply, locale);
            assert_eq!(applied["project"]["expressions"][0]["source"], expected);
            let model: motion_core::Project =
                serde_json::from_value(applied["project"].clone()).unwrap();
            assert_eq!(
                model.evaluated_at(0.).unwrap().layers[0]
                    .transform
                    .position
                    .value,
                [150., 75., 100.]
            );
            assert_eq!(host.history(0).unwrap()["project"], state["project"]);
        }
    }
}

#[test]
fn reference_gestures_cancel_on_missing_release_escape_focus_loss_and_stale_source_without_touching_drafts(
) {
    for floating in [false, true] {
        let (_root, host, state, target) = linked_fixture();
        let mut panels = Panels::default();
        panels.selected = Some(7);
        panels.focused = Some(Panel::Timeline);
        panels.timeline.expanded.insert(7);
        panels.timeline.expanded.insert(8);
        panels
            .expressions
            .open_expanded(&state, target, Panel::Timeline);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        draw(&mut panels, &state, &mut atlas, Input::default(), floating);
        replace(&mut panels, &state, &mut atlas, "value + [9,8,7]", floating);
        let draft = source(&panels);
        for input in [
            Input::default(),
            key(Key::Escape, false),
            Input {
                focus_lost: true,
                ..Default::default()
            },
            Input {
                pressed: Some((MouseButton::Right, [1., 1.])),
                ..Default::default()
            },
            release([1., 1.]),
        ] {
            begin(&mut panels, &state, &mut atlas, floating);
            assert!(draw(&mut panels, &state, &mut atlas, input, floating).is_empty());
            assert!(!super::super::super::picking::active(
                &panels,
                floating.then_some(Panel::Timeline)
            ));
            assert_eq!(source(&panels), draft);
            let point = destination(&panels);
            assert!(draw(
                &mut panels,
                &state,
                &mut atlas,
                Input {
                    mouse: point,
                    ..Default::default()
                },
                floating
            )
            .is_empty());
            assert_eq!(source(&panels), draft);
        }
        begin(&mut panels, &state, &mut atlas, floating);
        let mut stale = state.clone();
        stale["document_generation"] =
            serde_json::json!(state["document_generation"].as_u64().unwrap_or(0) + 1);
        let point = destination(&panels);
        assert!(draw(&mut panels, &stale, &mut atlas, release(point), floating).is_empty());
        assert_eq!(source(&panels), draft);
        assert_eq!(host.state().unwrap()["project"], state["project"]);
    }
}

#[test]
fn reference_pick_is_owned_by_its_native_window_and_insertion_keeps_local_history_and_byte_limits()
{
    let (_root, host, state, target) = linked_fixture();
    let mut panels = Panels::default();
    panels.selected = Some(7);
    panels.focused = Some(Panel::Timeline);
    panels.timeline.expanded.extend([7, 8]);
    panels
        .expressions
        .open_expanded(&state, target, Panel::Timeline);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    draw(&mut panels, &state, &mut atlas, Input::default(), false);
    replace(&mut panels, &state, &mut atlas, "value + [1,2,3]", false);
    let draft = source(&panels);
    draw(&mut panels, &state, &mut atlas, key(Key::A, true), false);
    begin(&mut panels, &state, &mut atlas, false);
    let point = destination(&panels);
    assert!(draw(&mut panels, &state, &mut atlas, release(point), true).is_empty());
    assert!(super::super::super::picking::active(&panels, None));
    assert_eq!(source(&panels), draft);
    assert!(draw(&mut panels, &state, &mut atlas, release(point), false).is_empty());
    assert_ne!(source(&panels), draft);
    assert!(draw(&mut panels, &state, &mut atlas, key(Key::Z, true), false).is_empty());
    assert_eq!(source(&panels), draft);
    replace(&mut panels, &state, &mut atlas, &"x".repeat(8192), false);
    // An insertion at the end cannot silently truncate or report success.
    let editor = panels.expressions.expanded.as_mut().unwrap();
    editor.source.select_bytes(8192, 8192);
    begin(&mut panels, &state, &mut atlas, false);
    let point = destination(&panels);
    assert!(draw(&mut panels, &state, &mut atlas, release(point), false).is_empty());
    assert_eq!(source(&panels), "x".repeat(8192));
    let editor = panels.expressions.expanded.as_ref().unwrap();
    assert!(editor.source.error.as_ref().unwrap().contains("8192"));
    assert!(editor.message.as_ref().unwrap().1);
    assert_eq!(host.state().unwrap()["project"], state["project"]);
}
