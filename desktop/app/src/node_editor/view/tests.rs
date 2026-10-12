use super::super::tests::{frame, press, release, simple};
use super::*;
use motion_ui::input::Modifiers;

fn socket(editor: &NodeEditor, node: &str, port: &str, output: bool) -> [f32; 2] {
    editor
        .cards(editor.last_canvas)
        .into_iter()
        .flat_map(|c| c.sockets)
        .find(|s| s.output == output && s.port == PortRef::new(node, port))
        .unwrap()
        .point
}
fn preview_click(point: [f32; 2]) -> Input {
    let mut input = press(point, MouseButton::Left);
    input.modifiers = Modifiers {
        control: true,
        shift: true,
        alt: false,
    };
    input
}

#[test]
fn invalid_wire_feedback_matches_cycle_budget_and_units_without_mutation() {
    let mut editor = simple();
    editor
        .connect_ports(PortRef::new("number", "value"), PortRef::new("math", "a"))
        .unwrap();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.snapshot();
    let source = socket(&editor, "math", "value", true);
    let target = socket(&editor, "math", "a", false);
    frame(&mut editor, &mut atlas, press(source, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: target,
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    let cards = editor.cards(editor.last_canvas);
    let (_, result) = editor
        .wire_feedback(&cards, editor.last_canvas, target)
        .unwrap();
    assert_eq!(result.unwrap_err().code, ErrorCode::Cycle);
    let mut paint = PaintList::default();
    editor.paint_canvas(
        &mut paint,
        &mut atlas,
        &cards,
        editor.last_canvas,
        &Input {
            mouse: target,
            ..Default::default()
        },
        Language::English,
    );
    assert!(paint
        .shapes
        .iter()
        .zip(&paint.colors)
        .any(
            |(shape, color)| matches!(shape, motion_ui::paint::Shape::Line { .. })
                && *color == INVALID_WIRE
        ));
    frame(&mut editor, &mut atlas, release(target, MouseButton::Left));
    assert_eq!(editor.document, before.document);
    assert_eq!(editor.revision, before.revision);
    editor.options.max_connections = editor.document.connections.len();
    let source = socket(&editor, "number", "value", true);
    let target = socket(&editor, "math", "b", false);
    frame(&mut editor, &mut atlas, press(source, MouseButton::Left));
    let cards = editor.cards(editor.last_canvas);
    assert_eq!(
        editor
            .wire_feedback(&cards, editor.last_canvas, target)
            .unwrap()
            .1
            .unwrap_err()
            .code,
        ErrorCode::Budget
    );
    frame(&mut editor, &mut atlas, release(target, MouseButton::Left));
    assert_eq!(editor.document, before.document);
    let mut editor = NodeEditor::default();
    let circle = editor
        .document
        .nodes
        .iter()
        .find(|n| n.id == "circle")
        .unwrap()
        .position;
    let uv = editor
        .add_node(NodeOp::Uv, [circle[0] - 300., circle[1]])
        .unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let source = socket(&editor, &uv, "value", true);
    let target = socket(&editor, "circle", "position", false);
    frame(&mut editor, &mut atlas, press(source, MouseButton::Left));
    let cards = editor.cards(editor.last_canvas);
    assert_eq!(
        editor
            .wire_feedback(&cards, editor.last_canvas, target)
            .unwrap()
            .1
            .unwrap_err()
            .code,
        ErrorCode::TypeMismatch
    );
}

#[test]
fn reconnect_on_wrong_direction_or_node_body_preserves_original_input() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.snapshot();
    let input = socket(&editor, "output", "color", false);
    let wrong = socket(&editor, "number", "value", true);
    frame(&mut editor, &mut atlas, press(input, MouseButton::Left));
    frame(&mut editor, &mut atlas, release(wrong, MouseButton::Left));
    assert_eq!(editor.document, before.document);
    assert_eq!(editor.revision, before.revision);
    frame(&mut editor, &mut atlas, press(input, MouseButton::Left));
    let body = editor
        .view
        .world_to_screen([420., 130.], editor.last_canvas);
    frame(&mut editor, &mut atlas, release(body, MouseButton::Left));
    assert_eq!(editor.document, before.document);
}

#[test]
fn reconnecting_same_input_is_noop_and_over_budget_replacement_keeps_old_edges() {
    let mut editor = simple();
    editor
        .connect_ports(PortRef::new("number", "value"), PortRef::new("math", "a"))
        .unwrap();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.snapshot();
    let history = editor.undo.len();
    let point = socket(&editor, "output", "color", false);
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    frame(&mut editor, &mut atlas, release(point, MouseButton::Left));
    assert_eq!(editor.document, before.document);
    assert_eq!(editor.revision, before.revision);
    assert_eq!(editor.undo.len(), history);
    editor.options.max_connections = 1;
    let error = editor
        .connect_ports(
            PortRef::new("color", "value"),
            PortRef::new("output", "color"),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Budget);
    assert_eq!(editor.document, before.document);
}

#[test]
fn alt_right_stroke_cuts_all_intersections_as_one_edit_and_cancel_keeps_edges() {
    let mut editor = simple();
    editor
        .connect_ports(PortRef::new("number", "value"), PortRef::new("math", "a"))
        .unwrap();
    editor
        .connect_ports(PortRef::new("number", "value"), PortRef::new("math", "b"))
        .unwrap();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.snapshot();
    let history = editor.undo.len();
    let start = editor
        .view
        .world_to_screen([300., 100.], editor.last_canvas);
    let end = editor
        .view
        .world_to_screen([300., 350.], editor.last_canvas);
    let mut input = press(start, MouseButton::Right);
    input.modifiers.alt = true;
    frame(&mut editor, &mut atlas, input.clone());
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: end,
            held: Some(MouseButton::Right),
            ..Default::default()
        },
    );
    assert_eq!(editor.document, before.document);
    assert!(matches!(&editor.drag,Some(Drag::Cut {inputs,..}) if inputs.len()==3));
    frame(&mut editor, &mut atlas, release(end, MouseButton::Right));
    assert!(editor.document.connections.is_empty());
    assert_eq!(editor.undo.len(), history + 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before.document);
    frame(&mut editor, &mut atlas, input);
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: end,
            held: Some(MouseButton::Right),
            ..Default::default()
        },
    );
    frame(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![(Key::Escape, Modifiers::default())],
            ..Default::default()
        },
    );
    frame(&mut editor, &mut atlas, release(end, MouseButton::Right));
    assert_eq!(editor.document, before.document);
    assert_eq!(editor.undo.len(), history);
}

#[test]
fn ctrl_shift_preview_selects_ports_and_restores_without_document_or_history_edits() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.snapshot();
    let selection = editor.selected.clone();
    let point = editor.view.world_to_screen([70., 280.], editor.last_canvas);
    let response = frame(&mut editor, &mut atlas, preview_click(point));
    assert!(matches!(
        response.actions.as_slice(),
        [Action::PreviewChanged]
    ));
    assert_eq!(
        editor.preview_target(),
        Some(&PortRef::new("number", "value"))
    );
    assert_eq!(editor.selected, selection);
    assert!(editor.drag.is_none());
    assert_eq!(editor.document, before.document);
    assert_eq!(editor.revision, before.revision);
    assert_eq!(editor.semantic_revision, before.semantic_revision);
    assert!(editor.undo.is_empty());
    let target = editor.snapshot();
    target.compile_preview().unwrap();
    frame(&mut editor, &mut atlas, release(point, MouseButton::Left));
    frame(&mut editor, &mut atlas, preview_click(point));
    assert!(editor.preview_target().is_none());
    assert_ne!(editor.stamp(), target.preview_stamp());
    let report = target.compile_preview().unwrap().report;
    assert!(
        !editor.accept_compile(&target, Ok(report)),
        "stale target cannot replace restored output"
    );
    assert_eq!(editor.document, before.document);
    let split = editor.add_node(NodeOp::SplitColor, [700., 100.]).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = editor
        .view
        .world_to_screen([710., 110.], editor.last_canvas);
    frame(&mut editor, &mut atlas, preview_click(point));
    assert!(
        matches!(&editor.context_menu,Some(ContextMenu {kind:MenuKind::Preview(id),..}) if id==&split)
    );
    frame(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![
                (Key::Down, Modifiers::default()),
                (Key::Enter, Modifiers::default()),
            ],
            ..Default::default()
        },
    );
    assert_eq!(editor.preview_target(), Some(&PortRef::new(&split, "b")));
    editor.delete_selected();
    assert!(
        editor.preview_target().is_none(),
        "deleted preview target is cleared"
    );
}

#[test]
fn cut_geometry_uses_visible_curve_and_screen_space_tolerance() {
    let area = Rect::new(100., 100., 300., 300.);
    assert_eq!(
        wires::clip_segment([50., 200.], [450., 200.], area),
        Some(([100., 200.], [400., 200.]))
    );
    assert!(wires::clip_segment([50., 50.], [450., 50.], area).is_none());
    for zoom in [0.4, 1., 2.5] {
        let mut editor = simple();
        editor.view.zoom = zoom;
        let canvas = Rect::new(0., 0., 2000., 1600.);
        let cards = editor.cards(canvas);
        let edge = &editor.document.connections[0];
        let curve = wires::wire_curve(&cards, edge);
        let p = curve[12];
        let mut hits = BTreeSet::new();
        editor.collect_cut_inputs(
            &cards,
            canvas,
            [p[0], p[1] - 8.],
            [p[0], p[1] + 8.],
            &mut hits,
        );
        assert!(hits.contains(&edge.to));
    }
}
