use super::super::inline::{Control, Popup};
use super::super::tests::{frame, press, release, simple};
use super::*;

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}
fn input_point(editor: &NodeEditor, id: &str, port: &str) -> [f32; 2] {
    let card = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|c| c.id == id)
        .unwrap();
    let node = editor.document.nodes.iter().find(|n| n.id == id).unwrap();
    let (row, _) = controls::control_rows(node, &editor.document, Language::English)
        .into_iter()
        .find(|(_, control)| matches!(control, Control::InputDefault { port: p, .. } if p == port))
        .unwrap();
    let styles = PaintList::default().styles();
    let rect = controls::screen_rect(
        controls::input_value_rect(row, &styles),
        &card,
        editor.view.zoom,
    );
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn add(editor: &mut NodeEditor, operation: NodeOp) -> String {
    let id = editor.add_node(operation, [100., 80.]).unwrap();
    editor.inspector_open = false;
    editor.undo.clear();
    id
}

#[test]
fn saving_inside_a_group_upgrades_the_root_and_preserves_defaults_across_navigation() {
    let mut editor = simple();
    editor.select(Some("math"), false);
    editor.group_selected().unwrap();
    let group = editor.selected().unwrap().to_string();
    assert!(editor.enter_group(&group));
    let id = editor.add_node(NodeOp::CombineVec2, [300., 80.]).unwrap();
    editor
        .set_input_default(&id, "x", InputValue::Float(12.))
        .unwrap();
    let snapshot = editor.snapshot();
    assert_eq!(snapshot.document.format_version, 2);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inside-group.msgraph");
    snapshot.save(&path).unwrap();
    assert_eq!(GraphDocument::load(&path).unwrap(), snapshot.document);
    assert!(editor.leave_group());
    assert_eq!(editor.snapshot().document, snapshot.document);
    assert!(editor.enter_group(&group));
    assert_eq!(
        editor.input_default(&id, "x").unwrap(),
        &InputValue::Float(12.)
    );
}

#[test]
fn input_default_scalar_drag_scales_and_commits_one_history_step() {
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for zoom in [0.5, 1., 2.] {
        let mut editor = simple();
        editor.view.zoom = zoom;
        let id = add(
            &mut editor,
            NodeOp::Math {
                operation: MathOp::Add,
                value_type: ValueType::Float,
            },
        );
        assert_eq!(editor.document.format_version, 2);
        let original = editor.document.clone();
        let count = original.nodes.len();
        frame(&mut editor, &mut atlas, Input::default());
        let point = input_point(&editor, &id, "a");
        frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
        let moved = [point[0] + 30. * zoom, point[1]];
        frame(
            &mut editor,
            &mut atlas,
            Input {
                mouse: moved,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        );
        assert_eq!(
            editor.input_default(&id, "a").unwrap(),
            &InputValue::Float(3.)
        );
        assert!(editor.undo.is_empty());
        frame(&mut editor, &mut atlas, release(moved, MouseButton::Left));
        assert_eq!(editor.undo.len(), 1);
        assert_eq!(editor.document.nodes.len(), count);
        assert_eq!(
            editor
                .document
                .nodes
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .position,
            [100., 80.]
        );
        let changed = editor.document.clone();
        assert!(editor.undo());
        assert_eq!(editor.document, original);
        assert!(editor.redo());
        assert_eq!(editor.document, changed);
    }
}

#[test]
fn vector_default_text_validates_ime_owns_keys_and_cancels_without_history() {
    let mut editor = simple();
    let id = add(
        &mut editor,
        NodeOp::Math {
            operation: MathOp::Add,
            value_type: ValueType::Vec2,
        },
    );
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = input_point(&editor, &id, "a");
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "1, 2, 3".into(),
            ..Default::default()
        },
    );
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.field.as_ref().unwrap().error.is_some());
    assert!(editor.undo.is_empty());
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "12.34567, -8.12345x".into(),
            keys: vec![(
                Key::A,
                motion_ui::input::Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        },
    );
    frame(&mut editor, &mut atlas, key(Key::Left));
    frame(&mut editor, &mut atlas, key(Key::Delete));
    assert_eq!(editor.field.as_ref().unwrap().text, "12.34567, -8.12345");
    assert!(editor.document.nodes.iter().any(|node| node.id == id));
    frame(
        &mut editor,
        &mut atlas,
        Input {
            composing: true,
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
    );
    assert!(editor.undo.is_empty());
    assert!(editor.field.is_some());
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert_eq!(
        editor.input_default(&id, "a").unwrap(),
        &InputValue::Vec2([12.34567, -8.12345])
    );
    assert_eq!(editor.undo.len(), 1);
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "9, 9".into(),
            ..Default::default()
        },
    );
    frame(&mut editor, &mut atlas, key(Key::Escape));
    assert_eq!(
        editor.input_default(&id, "a").unwrap(),
        &InputValue::Vec2([12.34567, -8.12345])
    );
    assert_eq!(editor.undo.len(), 1);
}

#[test]
fn wired_default_is_read_only_and_disconnect_restores_it_with_undo() {
    let mut editor = simple();
    let id = add(
        &mut editor,
        NodeOp::Math {
            operation: MathOp::Add,
            value_type: ValueType::Float,
        },
    );
    editor
        .set_input_default(&id, "a", InputValue::Float(7.))
        .unwrap();
    editor
        .connect_ports(PortRef::new("number", "value"), PortRef::new(&id, "a"))
        .unwrap();
    let wired = editor.document.clone();
    assert!(editor
        .set_input_default(&id, "a", InputValue::Float(9.))
        .is_err());
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = input_point(&editor, &id, "a");
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    frame(&mut editor, &mut atlas, release(point, MouseButton::Left));
    assert!(!editor.editing());
    assert_eq!(editor.document, wired);
    editor.disconnect_input(&PortRef::new(&id, "a"));
    assert_eq!(
        editor.input_default(&id, "a").unwrap(),
        &InputValue::Float(7.)
    );
    assert!(editor.undo());
    assert_eq!(editor.document, wired);
    assert!(editor.redo());
    editor
        .set_input_default(&id, "a", InputValue::Float(9.))
        .unwrap();
}

#[test]
fn bool_and_color_defaults_use_native_controls_and_color_cancel_restores_document() {
    let mut editor = simple();
    let id = add(
        &mut editor,
        NodeOp::Select {
            value_type: ValueType::Color,
        },
    );
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = input_point(&editor, &id, "condition");
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    assert_eq!(
        editor.input_default(&id, "condition").unwrap(),
        &InputValue::Bool(true)
    );
    assert_eq!(editor.undo.len(), 1);
    let original = editor.document.clone();
    let point = input_point(&editor, &id, "true_value");
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    assert!(matches!(editor.inline.popup, Some(Popup::Color { .. })));
    editor
        .preview_binding(
            &Binding::InputDefault(id.clone(), "true_value".into(), 0),
            0.25,
        )
        .unwrap();
    frame(&mut editor, &mut atlas, key(Key::Escape));
    assert_eq!(editor.document, original);
    assert_eq!(editor.undo.len(), 1);
}

#[test]
fn changing_storage_type_replaces_incompatible_defaults_and_undo_restores_wires() {
    let mut editor = simple();
    let id = add(
        &mut editor,
        NodeOp::Math {
            operation: MathOp::Add,
            value_type: ValueType::Float,
        },
    );
    editor
        .set_input_default(&id, "a", InputValue::Float(12.))
        .unwrap();
    editor
        .connect_ports(PortRef::new("number", "value"), PortRef::new(&id, "a"))
        .unwrap();
    let before = editor.document.clone();
    editor
        .set_operation(
            &id,
            NodeOp::Math {
                operation: MathOp::Add,
                value_type: ValueType::Vec3,
            },
        )
        .unwrap();
    assert_eq!(
        editor.input_default(&id, "a").unwrap(),
        &InputValue::Vec3([0.; 3])
    );
    assert!(!editor
        .document
        .connections
        .iter()
        .any(|edge| edge.to.node == id));
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}

#[test]
fn defaults_share_the_input_row_and_outputs_do_not_overlap_the_editor() {
    let mut editor = simple();
    let id = add(
        &mut editor,
        NodeOp::Math {
            operation: MathOp::Add,
            value_type: ValueType::Vec3,
        },
    );
    let node = editor.document.nodes.iter().find(|n| n.id == id).unwrap();
    let ports = node_ports(node, &[]).unwrap();
    let rows = controls::control_rows(node, &editor.document, Language::English);
    assert_eq!(
        rows.len(),
        inline::control_count(node, &editor.document) + ports.inputs.len()
    );
    let card = editor
        .cards(Rect::new(0., 0., 1000., 800.))
        .into_iter()
        .find(|c| c.id == id)
        .unwrap();
    let output = card.sockets.iter().find(|s| s.output).unwrap();
    assert!(card
        .sockets
        .iter()
        .filter(|s| !s.output)
        .all(|s| s.point[1] < output.point[1]));
    for socket in card.sockets.iter().filter(|s| !s.output) {
        let (row, _) = rows.iter().find(|(_, c)| matches!(c, Control::InputDefault { port, .. } if *port == socket.port.port)).unwrap();
        assert!(
            (card.rect.min[1] + row.min[1] + row.height() * 0.5 - socket.point[1]).abs() < 0.01
        );
    }
}

#[test]
fn input_default_fields_stay_inside_cards_with_wide_style_columns() {
    let styles = PaintList::default()
        .styles()
        .with_overrides(
            br#"{"tokens":{"sizes":{"label.column":190,"value.min_width":80,"padding.panel":12}}}"#,
        )
        .unwrap();
    let row = Rect::new(0., 0., NODE_WIDTH, ROW);
    let field = controls::input_value_rect(row, &styles);
    assert!(field.min[0] >= row.min[0]);
    assert!(field.max[0] <= row.max[0] - 12.);
    assert!(field.width() >= 80.);
    assert_eq!(field.height(), ROW);
}
