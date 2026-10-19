use super::super::tests::{frame, press, simple};
use super::*;

fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn area() -> Rect {
    Rect::new(0., 0., 1440., 900.)
}
fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}
fn open(editor: &mut NodeEditor, atlas: &mut TextAtlas) {
    frame(editor, atlas, Input::default());
    let styles = PaintList::default().styles();
    let point = center(editor.diagnostic_toggle(editor.layout(area()).footer, &styles));
    frame(editor, atlas, press(point, MouseButton::Left));
    assert!(editor.diagnostic_drawer.open);
}

#[test]
fn drawer_click_locates_offscreen_node_and_preserves_source_history_and_preview() {
    let mut editor = simple();
    assert!(editor.disconnect_input(&PortRef::new("output", "color")));
    editor.view.pan = [-6000., -4000.];
    let before = editor.snapshot();
    let history = editor.undo.len();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    open(&mut editor, &mut atlas);
    let styles = PaintList::default().styles();
    let drawer = editor.diagnostic_drawer_layout(area(), &styles).unwrap();
    let response = frame(
        &mut editor,
        &mut atlas,
        press(center(drawer.locate), MouseButton::Left),
    );
    assert_eq!(editor.selected(), Some("output"));
    let point = editor.port_position("output", "color", false).unwrap();
    assert!(point[1] < drawer.area.min[1]);
    assert!(!response.changed && !response.semantic_changed);
    assert!(response.actions.is_empty());
    assert_eq!(editor.snapshot().document, before.document);
    assert_eq!(editor.stamp(), before.preview_stamp());
    assert_eq!(editor.undo.len(), history);
    frame(&mut editor, &mut atlas, key(Key::Delete));
    assert_eq!(
        editor.snapshot().document,
        before.document,
        "drawer owns destructive keys"
    );
    editor.select(Some("number"), false);
    editor.cancel_interaction();
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert_eq!(
        editor.selected(),
        Some("number"),
        "unfocused drawer cannot navigate"
    );
    frame(
        &mut editor,
        &mut atlas,
        press(center(drawer.content), MouseButton::Left),
    );
    frame(&mut editor, &mut atlas, key(Key::Escape));
    assert!(!editor.diagnostic_drawer.open);
    assert!(editor.undo());
    assert!(editor.diagnostic().is_none());
    frame(&mut editor, &mut atlas, Input::default());
    assert!(!editor.diagnostic_drawer.open);
    assert_eq!(
        editor.snapshot().compile().unwrap().source,
        simple().snapshot().compile().unwrap().source
    );
}

#[test]
fn drawer_wraps_full_unicode_cause_scrolls_and_resets_on_new_failure_and_recovery() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let message = format!(
        "{}\n末尾 original cause",
        "不可截断/very-long-path/".repeat(70)
    );
    let lines = diagnostics::wrapped_lines(&mut atlas, &message, 160.);
    assert_eq!(lines.concat(), message.replace('\n', ""));
    assert!(lines.iter().all(|line| atlas.measure(line) <= 160.));
    let snapshot = editor.snapshot();
    assert!(editor.accept_validation(
        &snapshot,
        Err(Diagnostic {
            code: ErrorCode::Io,
            node: None,
            port: None,
            message
        })
    ));
    open(&mut editor, &mut atlas);
    assert!(editor.diagnostic_drawer.scroll.max > 0.);
    frame(&mut editor, &mut atlas, key(Key::End));
    assert_eq!(
        editor.diagnostic_drawer.scroll.offset,
        editor.diagnostic_drawer.scroll.max
    );
    frame(&mut editor, &mut atlas, key(Key::Home));
    assert_eq!(editor.diagnostic_drawer.scroll.offset, 0.);
    let styles = PaintList::default().styles();
    let point = center(
        editor
            .diagnostic_drawer_layout(area(), &styles)
            .unwrap()
            .content,
    );
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: point,
            wheel: [0., -200.],
            ..Default::default()
        },
    );
    assert!(editor.diagnostic_drawer.scroll.offset > 0.);
    editor.accept_validation(
        &snapshot,
        Err(Diagnostic {
            code: ErrorCode::Io,
            node: None,
            port: None,
            message: "new failure".into(),
        }),
    );
    frame(&mut editor, &mut atlas, Input::default());
    assert_eq!(editor.diagnostic_drawer.scroll.offset, 0.);
    assert!(editor.diagnostic_drawer.open);
    assert!(editor.diagnostic_target().is_none());
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.parents.is_empty());
    editor.accept_validation(&snapshot, Ok(snapshot.validate().unwrap()));
    frame(&mut editor, &mut atlas, Input::default());
    assert!(!editor.diagnostic_drawer.open);
}

#[test]
fn drawer_navigation_resolves_nested_scopes_without_losing_edits_or_collapsed_state() {
    let mut editor = NodeEditor::default();
    editor.library_open = false;
    editor.inspector_open = false;
    editor.select(Some("circle"), false);
    editor.select(Some("fill"), true);
    assert!(editor.group_selected().unwrap());
    let outer = editor.selected().unwrap().to_owned();
    assert!(editor.enter_group(&outer));
    editor.select(Some("circle"), false);
    assert!(editor.group_selected().unwrap());
    let inner = editor.selected().unwrap().to_owned();
    assert!(editor.enter_group(&inner));
    assert!(editor.disconnect_input(&PortRef::new("circle", "radius")));
    assert!(editor.toggle_collapsed("circle"));
    assert!(editor.leave_group());
    assert!(editor.leave_group());
    assert_eq!(
        editor.diagnostic().unwrap().node.as_deref(),
        Some(format!("{outer}/{inner}/circle").as_str())
    );
    let before = editor.snapshot();
    let history = editor.undo.len();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    open(&mut editor, &mut atlas);
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert_eq!(editor.selected(), Some("circle"));
    assert_eq!(
        editor
            .parents
            .iter()
            .map(|p| p.group.as_str())
            .collect::<Vec<_>>(),
        vec![outer.as_str(), inner.as_str()]
    );
    assert_eq!(editor.snapshot().document, before.document);
    assert_eq!(editor.revision, before.revision);
    assert_eq!(editor.undo.len(), history);
    let circle = editor
        .document
        .nodes
        .iter()
        .find(|n| n.id == "circle")
        .unwrap();
    assert!(circle.collapsed);
    assert!(editor.diagnostic_missing_inputs(circle).contains("radius"));
}

#[test]
fn diagnostic_rings_cover_all_missing_inputs_but_exclude_defaults_and_connections() {
    let mut editor = simple();
    editor.diagnostic = Some(Diagnostic {
        code: ErrorCode::MissingInput,
        node: Some("math".into()),
        port: Some("a".into()),
        message: "required input is not connected".into(),
    });
    let math = editor
        .document
        .nodes
        .iter()
        .find(|n| n.id == "math")
        .unwrap();
    assert_eq!(
        editor.diagnostic_missing_inputs(math),
        BTreeSet::from(["a".into(), "b".into()])
    );
    editor
        .document
        .nodes
        .iter_mut()
        .find(|n| n.id == "math")
        .unwrap()
        .input_defaults
        .insert("b".into(), InputValue::Float(2.));
    let math = editor
        .document
        .nodes
        .iter()
        .find(|n| n.id == "math")
        .unwrap();
    assert_eq!(
        editor.diagnostic_missing_inputs(math),
        BTreeSet::from(["a".into()])
    );
    editor
        .document
        .connections
        .push(Connection::new("number", "value", "math", "a"));
    let math = editor
        .document
        .nodes
        .iter()
        .find(|n| n.id == "math")
        .unwrap();
    assert!(editor.diagnostic_missing_inputs(math).is_empty());
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    editor.document.connections.pop();
    frame(&mut editor, &mut atlas, Input::default());
    let cards = editor.cards(editor.last_canvas);
    let mut paint = PaintList::default();
    editor.paint_canvas(
        &mut paint,
        &mut atlas,
        &cards,
        editor.last_canvas,
        &Input::default(),
        Language::English,
    );
    let error = paint.styles().color("error");
    assert!(paint.colors.iter().any(|color| *color == error));
}

#[test]
fn narrow_drawer_keeps_controls_inside_and_does_not_steal_drag_or_menu_input() {
    let mut editor = simple();
    editor.disconnect_input(&PortRef::new("output", "color"));
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let styles = PaintList::default().styles();
    open(&mut editor, &mut atlas);
    for width in [320., 360., 600.] {
        for language in [Language::Chinese, Language::English] {
            let area = Rect::new(0., 0., width, 480.);
            editor.draw(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &Input::default(),
                language,
            );
            let drawer = editor.diagnostic_drawer_layout(area, &styles).unwrap();
            assert!(drawer.locate.min[0] >= area.min[0]);
            assert!(drawer.locate.max[0] <= area.max[0]);
            assert!(drawer.content.height() > 0.);
            assert!(
                editor
                    .diagnostic_toggle(editor.layout(area).footer, &styles)
                    .min[0]
                    >= area.min[0]
            );
        }
    }
    editor.diagnostic_drawer.focused = false;
    editor.drag = Some(Drag::WireInput {
        target: PortRef::new("output", "color"),
    });
    assert!(!editor.diagnostics_input(area(), &key(Key::Enter), &styles));
    assert!(!editor.locate_diagnostic(area(), &styles));
    editor.drag = None;
    editor.context_menu = Some(ContextMenu {
        position: [30., 40.],
        kind: MenuKind::Canvas,
        selected: 0,
    });
    assert!(!editor.diagnostics_input(area(), &key(Key::Enter), &styles));
    assert!(!editor.locate_diagnostic(area(), &styles));
}

#[test]
fn stale_or_absent_diagnostic_targets_do_not_move_selection_or_scopes() {
    let mut editor = simple();
    editor.select(Some("number"), false);
    let styles = PaintList::default().styles();
    let view = editor.view;
    for node in [None, Some("missing/group/node".into())] {
        editor.diagnostic = Some(Diagnostic {
            code: ErrorCode::UnknownNode,
            node,
            port: None,
            message: "not found".into(),
        });
        assert!(!editor.locate_diagnostic(area(), &styles));
        assert_eq!(editor.selected(), Some("number"));
        assert_eq!(editor.view.pan, view.pan);
        assert!(editor.parents.is_empty());
    }
}

#[test]
fn releasing_a_wire_over_the_drawer_cancels_without_connecting_through_it() {
    let mut editor = simple();
    editor.disconnect_input(&PortRef::new("output", "color"));
    let before = editor.snapshot();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    open(&mut editor, &mut atlas);
    let styles = PaintList::default().styles();
    let point = center(
        editor
            .diagnostic_drawer_layout(area(), &styles)
            .unwrap()
            .content,
    );
    editor.drag = Some(Drag::Wire {
        source: PortRef::new("color", "value"),
        old_input: None,
    });
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: point,
            ..key(Key::Escape)
        },
    );
    assert!(
        editor.drag.is_none(),
        "active wire retains Escape over the drawer"
    );
    assert!(
        editor.diagnostic_drawer.open,
        "first Escape cancels the wire"
    );
    editor.drag = Some(Drag::Wire {
        source: PortRef::new("color", "value"),
        old_input: None,
    });
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: point,
            released: Some((MouseButton::Left, point)),
            ..Default::default()
        },
    );
    assert!(editor.drag.is_none());
    assert_eq!(editor.snapshot().document, before.document);
    assert_eq!(editor.revision, before.revision);
}

#[test]
fn numeric_drag_released_over_drawer_rolls_back_instead_of_changing_a_hidden_value() {
    let mut editor = simple();
    editor.disconnect_input(&PortRef::new("output", "color"));
    let before = editor.snapshot();
    let history = editor.undo.len();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    open(&mut editor, &mut atlas);
    let card = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|card| card.id == "number")
        .unwrap();
    let node = editor
        .document
        .nodes
        .iter()
        .find(|node| node.id == "number")
        .unwrap();
    let (row, _) = controls::control_rows(node, &editor.document, Language::English).remove(0);
    let point = center(controls::screen_rect(
        controls::number_rect(row, false),
        &card,
        editor.view.zoom,
    ));
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    assert!(editor.inline.number.is_some());
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: [point[0] + 25., point[1]],
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    assert_ne!(editor.document, before.document);
    let styles = PaintList::default().styles();
    let covered = center(
        editor
            .diagnostic_drawer_layout(area(), &styles)
            .unwrap()
            .content,
    );
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: covered,
            released: Some((MouseButton::Left, covered)),
            ..Default::default()
        },
    );
    assert!(editor.inline.number.is_none());
    assert_eq!(editor.snapshot().document, before.document);
    assert_eq!(editor.undo.len(), history);
}
