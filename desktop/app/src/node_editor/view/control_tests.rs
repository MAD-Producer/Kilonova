use super::super::inline::{Control, Popup};
use super::super::tests::{frame, press, release, simple};
use super::*;
use motion_ui::input::Modifiers;

fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn row(editor: &NodeEditor, id: &str, index: usize, slider: bool) -> Rect {
    let card = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|c| c.id == id)
        .unwrap();
    let node = editor.document.nodes.iter().find(|n| n.id == id).unwrap();
    let (rect, control) =
        controls::control_rows(node, &editor.document, Language::English).remove(index);
    let rect = match control {
        Control::Number {
            slider: bounded, ..
        } => {
            if slider {
                controls::slider_rect(rect)
            } else {
                controls::number_rect(rect, bounded)
            }
        }
        _ => rect,
    };
    controls::screen_rect(rect, &card, editor.view.zoom)
}
fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}
fn value(editor: &NodeEditor) -> f32 {
    editor
        .binding_value(&Binding::Constant("number".into(), 0))
        .unwrap()
}
fn held(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}

#[test]
fn choice_popup_attaches_to_field_and_flips_inside_canvas() {
    let area = Rect::new(20., 30., 400., 300.);
    let anchor = Rect::new(60., 80., 90., 18.);
    let down = control_popup::choice_popup_rect(anchor, area, [120., 94.]);
    assert_eq!(down.min, [anchor.min[0], anchor.max[1]]);
    assert_eq!([down.width(), down.height()], [120., 94.]);
    let low = Rect::new(375., 290., 40., 18.);
    let up = control_popup::choice_popup_rect(low, area, [120., 94.]);
    assert_eq!(up.max, [area.max[0], low.min[1]]);
    let tall = control_popup::choice_popup_rect(anchor, area, [1000., 1000.]);
    assert_eq!(tall.min, [area.min[0], anchor.max[1]]);
    assert_eq!(tall.max, area.max);
}

#[test]
fn choice_popup_tracks_the_clicked_field_on_multi_choice_nodes() {
    let mut editor = simple();
    editor.inspector_open = false;
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let rows = controls::control_rows(
        &editor.document.nodes[3],
        &editor.document,
        Language::English,
    );
    assert_eq!(rows.len(), 2);
    for (index, (local, _)) in rows.into_iter().enumerate() {
        let point = center(row(&editor, "math", index, false));
        frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
        let Some(Popup::Choice { field, .. }) = &editor.inline.popup else {
            panic!("choice did not open")
        };
        assert_eq!(*field, controls::number_rect(local, false));
        frame(&mut editor, &mut atlas, key(Key::Escape));
        assert!(editor.inline.popup.is_none());
        assert!(editor.undo.is_empty());
    }
}

#[test]
fn choice_popup_pointer_selection_and_cancel_work_at_canvas_zooms() {
    for zoom in [0.48, 1., 1.75] {
        let graph = GraphDocument::from_json(include_str!(
            "../../../../../crates/motion-graph/examples/compact-gradient.msgraph"
        ))
        .unwrap();
        let mut editor = NodeEditor::new(graph);
        editor.first_frame = false;
        editor.inspector_open = false;
        editor.library_open = false;
        editor.view.zoom = zoom;
        editor.view.pan = [20.; 2];
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        frame(&mut editor, &mut atlas, Input::default());
        let original = editor.snapshot().document;
        let field = editor.choice_bounds("gradient").unwrap();
        frame(
            &mut editor,
            &mut atlas,
            press(center(field), MouseButton::Left),
        );
        assert!(matches!(editor.inline.popup, Some(Popup::Choice { .. })));
        frame(
            &mut editor,
            &mut atlas,
            release(center(field), MouseButton::Left),
        );
        let list = PaintList::default();
        let styles = list.styles();
        let padding = styles.metric("padding", 5.) * zoom;
        let base_row_height = styles.metric("node.dropdown.row.height", 28.);
        assert!(
            base_row_height >= 27.,
            "retain the logical pointer row height"
        );
        let row_height = base_row_height * zoom;
        let mut painted = PaintList::default();
        let canvas = editor.last_canvas;
        editor.paint_control_popup(
            &mut painted,
            &mut atlas,
            canvas,
            &Input::default(),
            Language::English,
        );
        let rect = painted
            .shapes
            .iter()
            .find_map(|shape| match shape {
                motion_ui::paint::Shape::Procedural {
                    area,
                    kind: motion_ui::paint::Pattern::Box { .. },
                    ..
                } => Some(*area),
                _ => None,
            })
            .expect("painted choice menu");
        assert!(
            (rect.height() - (row_height * 3. + padding * 2.)).abs() < 0.01,
            "menu rows and padding must scale with the card: {zoom}"
        );
        assert!((rect.width() - field.width()).abs() < 0.01);
        assert!(!painted.glyphs.is_empty());
        let max_glyph_height = painted
            .glyphs
            .iter()
            .map(|glyph| glyph.size[1])
            .fold(0., f32::max);
        assert!(max_glyph_height <= styles.metric("font.body", 12.) * zoom + 1.,
            "menu text stayed at screen size instead of canvas zoom: {zoom}, glyph={max_glyph_height}");
        // Click the last row with real pointer input, including at reduced node zoom.
        let point = [
            rect.min[0] + padding * 2.,
            rect.min[1] + padding + row_height * 2.5,
        ];
        frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
        assert!(editor.inline.popup.is_none());
        assert_eq!(
            editor.document.nodes[1].operation,
            NodeOp::Gradient {
                mode: GradientMode::Conic
            }
        );
        assert_eq!(editor.document.connections, original.connections);
        assert_eq!(
            editor.document.nodes[1].input_defaults,
            original.nodes[1].input_defaults
        );
        assert_eq!(editor.undo.len(), 1);
        let committed = editor.snapshot().document;
        frame(&mut editor, &mut atlas, release(point, MouseButton::Left));
        frame(
            &mut editor,
            &mut atlas,
            press(center(field), MouseButton::Left),
        );
        frame(&mut editor, &mut atlas, key(Key::Down));
        frame(&mut editor, &mut atlas, key(Key::Escape));
        assert!(editor.inline.popup.is_none());
        assert_eq!(editor.snapshot().document, committed);
        assert_eq!(editor.undo.len(), 1);
        frame(
            &mut editor,
            &mut atlas,
            press(center(field), MouseButton::Left),
        );
        let outside = editor.last_canvas.min;
        frame(&mut editor, &mut atlas, press(outside, MouseButton::Left));
        assert!(editor.inline.popup.is_none());
        assert_eq!(editor.snapshot().document, committed);
        assert!(editor.undo());
        assert_eq!(editor.snapshot().document, original);
    }
}

#[test]
fn gradient_mode_dropdown_preserves_defaults_and_wires_and_is_undoable() {
    for language in [Language::Chinese, Language::English] {
        let graph = GraphDocument::from_json(include_str!(
            "../../../../../crates/motion-graph/examples/compact-gradient.msgraph"
        ))
        .unwrap();
        let mut editor = NodeEditor::new(graph);
        editor.inspector_open = false;
        editor.view.zoom = 0.85;
        editor.view.pan = [20.; 2];
        let before = editor.snapshot().document;
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        frame(&mut editor, &mut atlas, Input::default());
        let node = editor
            .document
            .nodes
            .iter()
            .find(|n| n.id == "gradient")
            .unwrap();
        let index = controls::control_rows(node, &editor.document, language)
            .iter()
            .position(|(_, control)| matches!(control, Control::Choice { .. }))
            .unwrap();
        let point = center(row(&editor, "gradient", index, false));
        frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
        assert!(matches!(editor.inline.popup, Some(Popup::Choice { .. })));
        frame(&mut editor, &mut atlas, key(Key::Down));
        frame(&mut editor, &mut atlas, key(Key::Enter));
        assert!(matches!(
            editor
                .document
                .nodes
                .iter()
                .find(|n| n.id == "gradient")
                .unwrap()
                .operation,
            NodeOp::Gradient {
                mode: GradientMode::Radial
            }
        ));
        assert_eq!(editor.document.connections, before.connections);
        assert_eq!(
            editor.document.nodes[1].input_defaults,
            before.nodes[1].input_defaults
        );
        assert!(editor.diagnostic().is_none());
        assert_eq!(editor.undo.len(), 1);
        assert!(editor.undo());
        assert_eq!(editor.snapshot().document, before);
        assert!(editor.redo());
        editor
            .set_input_default("gradient", "falloff", InputValue::Float(2.))
            .unwrap();
        let snapshot = editor.snapshot();
        assert_eq!(
            GraphDocument::from_json(&snapshot.document.to_json().unwrap()).unwrap(),
            snapshot.document
        );
        snapshot.compile().unwrap();
        assert!(editor.undo());
        assert_eq!(
            editor.document.nodes[1].input_defaults["falloff"],
            InputValue::Float(1.)
        );
    }
}

#[test]
fn gradient_catalog_creation_needs_only_uv_and_keeps_wired_fallbacks() {
    let mut editor = simple();
    let operation = catalog()
        .into_iter()
        .find(|item| matches!(item.operation, NodeOp::Gradient { .. }))
        .unwrap()
        .operation;
    let gradient = editor.add_node(operation, [120., 100.]).unwrap();
    let uv = editor.add_node(NodeOp::Uv, [0.; 2]).unwrap();
    editor
        .connect_ports(
            PortRef::new(&gradient, "color"),
            PortRef::new("output", "color"),
        )
        .unwrap();
    assert_eq!(
        editor.diagnostic().unwrap().port.as_deref(),
        Some("coordinate")
    );
    editor
        .connect_ports(
            PortRef::new(&uv, "value"),
            PortRef::new(&gradient, "coordinate"),
        )
        .unwrap();
    assert!(editor.diagnostic().is_none());
    editor
        .connect_ports(
            PortRef::new("number", "value"),
            PortRef::new(&gradient, "falloff"),
        )
        .unwrap();
    assert!(editor
        .set_input_default(&gradient, "falloff", InputValue::Float(2.))
        .is_err());
    assert_eq!(
        editor.input_default(&gradient, "falloff").unwrap(),
        &InputValue::Float(1.)
    );
    assert!(editor.undo());
    editor
        .set_input_default(&gradient, "falloff", InputValue::Float(2.))
        .unwrap();
    assert_eq!(
        editor.input_default(&gradient, "falloff").unwrap(),
        &InputValue::Float(2.)
    );
}

#[test]
fn numeric_live_drag_preserves_positions_multiselection_and_one_history_step() {
    for zoom in [0.5, 1., 2.] {
        let mut editor = simple();
        editor.view.zoom = zoom;
        editor.inspector_open = false;
        editor.select(Some("color"), false);
        editor.select(Some("number"), true);
        let selected = editor.selected.clone();
        let before = editor.document.clone();
        let stamp = editor.stamp();
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        frame(&mut editor, &mut atlas, Input::default());
        let point = center(row(&editor, "number", 0, false));
        frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
        for delta in [10., 20., 30.] {
            let result = frame(
                &mut editor,
                &mut atlas,
                held([point[0] + delta * zoom, point[1]]),
            );
            assert!(result.semantic_changed);
            assert!(editor.undo.is_empty());
        }
        assert!((value(&editor) - 4.).abs() < 1e-5);
        assert_ne!(editor.stamp(), stamp);
        frame(
            &mut editor,
            &mut atlas,
            release([point[0] + 30. * zoom, point[1]], MouseButton::Left),
        );
        assert_eq!(editor.selected, selected);
        assert!(editor.drag.is_none());
        assert_eq!(editor.undo.len(), 1);
        assert!(editor
            .document
            .nodes
            .iter()
            .zip(&before.nodes)
            .all(|(a, b)| a.position == b.position));
        let changed = editor.document.clone();
        assert!(editor.undo());
        assert_eq!(editor.document, before);
        assert!(editor.redo());
        assert_eq!(editor.document, changed);
    }
}

#[test]
fn canceled_numeric_drag_restores_preview_without_losing_existing_redo() {
    for cancel in [
        key(Key::Escape),
        Input {
            focus_lost: true,
            ..Default::default()
        },
    ] {
        let mut editor = simple();
        editor.set_constant("number", 0, 2.).unwrap();
        editor.undo();
        let before = editor.document.clone();
        let redo = editor.redo.len();
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        frame(&mut editor, &mut atlas, Input::default());
        let point = center(row(&editor, "number", 0, false));
        frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
        frame(&mut editor, &mut atlas, held([point[0] + 50., point[1]]));
        let pending = editor.snapshot();
        assert_ne!(editor.document, before);
        let result = frame(&mut editor, &mut atlas, cancel);
        assert!(result.semantic_changed);
        assert_eq!(editor.document, before);
        assert_eq!(editor.redo.len(), redo);
        assert_ne!(pending.preview_stamp(), editor.stamp());
        assert!(!editor.editing());
        assert!(editor.undo.is_empty());
    }
}

#[test]
fn numeric_text_validates_and_owns_delete_arrows_and_ime_confirmation() {
    let mut editor = simple();
    editor.select(Some("number"), false);
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = center(row(&editor, "number", 0, false));
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    frame(&mut editor, &mut atlas, release(point, MouseButton::Left));
    let original = editor.document.clone();
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "NaN".into(),
            ..Default::default()
        },
    );
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert_eq!(editor.document, original);
    assert!(editor
        .inline
        .number
        .as_ref()
        .unwrap()
        .editor
        .text
        .error
        .is_some());
    frame(&mut editor, &mut atlas, key(Key::Delete));
    assert_eq!(editor.document, original);
    frame(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![(
                Key::A,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            text: "12.3456".into(),
            ..Default::default()
        },
    );
    frame(
        &mut editor,
        &mut atlas,
        Input {
            composing: true,
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
    );
    assert_eq!(editor.document, original);
    assert!(editor.inline.number.is_some());
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!((value(&editor) - 12.3456).abs() < 1e-5);
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, original);
}

#[test]
fn out_and_back_drag_is_not_a_text_click_and_shift_starts_fine_scrubbing() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = center(row(&editor, "number", 0, false));
    let mut input = press(point, MouseButton::Left);
    input.released = Some((MouseButton::Left, point));
    input.held = None;
    input.pointer_travel = 30.;
    frame(&mut editor, &mut atlas, input);
    assert!(!editor.editing());
    assert!(editor.undo.is_empty());
    assert_eq!(value(&editor), 1.);
    let mut input = press(point, MouseButton::Left);
    input.modifiers.shift = true;
    frame(&mut editor, &mut atlas, input);
    let mut input = held([point[0] + 30., point[1]]);
    input.modifiers.shift = true;
    frame(&mut editor, &mut atlas, input);
    assert!((value(&editor) - 1.3).abs() < 1e-5);
    frame(&mut editor, &mut atlas, key(Key::Escape));
    assert_eq!(value(&editor), 1.);
}

#[test]
fn parameter_slider_uses_shared_default_range_and_step_without_moving_nodes() {
    let mut editor = simple();
    let id = editor
        .add_public_parameter(ValueType::Float, [20., 400.])
        .unwrap();
    let param = editor.document.public_parameters.last_mut().unwrap();
    param.min = -2.;
    param.max = 2.;
    param.step = 0.25;
    param.default[0] = 0.;
    let pid = param.id.clone();
    editor
        .add_node(
            NodeOp::Parameter {
                parameter: pid.clone(),
            },
            [400., 400.],
        )
        .unwrap();
    editor.undo.clear();
    let before = editor.document.clone();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let rect = row(&editor, &id, 0, true);
    let point = [rect.min[0] + rect.width() * 0.8, center(rect)[1]];
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    assert!((editor.document.public_parameters[0].default[0] - 1.25).abs() < 1e-6);
    frame(
        &mut editor,
        &mut atlas,
        held([rect.max[0] + 1000., point[1]]),
    );
    assert_eq!(editor.document.public_parameters[0].default[0], 2.);
    frame(
        &mut editor,
        &mut atlas,
        release([rect.max[0] + 1000., point[1]], MouseButton::Left),
    );
    assert_eq!(editor.undo.len(), 1);
    assert_eq!(editor.document.nodes, before.nodes);
    assert_eq!(editor.document.public_parameters[0].id, pid);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}

#[test]
fn dropdown_explicit_type_change_undo_restores_removed_connections() {
    let mut editor = simple();
    editor
        .connect_ports(PortRef::new("number", "value"), PortRef::new("math", "a"))
        .unwrap();
    editor.undo.clear();
    let before = editor.document.clone();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = center(row(&editor, "math", 1, false));
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    assert_eq!(editor.document, before);
    assert!(matches!(editor.inline.popup, Some(Popup::Choice { .. })));
    frame(&mut editor, &mut atlas, key(Key::Down));
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(matches!(
        editor
            .document
            .nodes
            .iter()
            .find(|n| n.id == "math")
            .unwrap()
            .operation,
        NodeOp::Math {
            value_type: ValueType::Vec2,
            ..
        }
    ));
    assert_eq!(
        editor.document.connections.len(),
        before.connections.len() - 1
    );
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.notice.as_ref().unwrap().contains("incompatible"));
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}

#[test]
fn color_picker_live_hsv_alpha_apply_and_cancel_share_one_transaction() {
    let mut editor = simple();
    editor
        .document
        .nodes
        .retain(|node| node.id == "color" || node.id == "output");
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.document.clone();
    let point = center(row(&editor, "color", 0, false));
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    let position = match &editor.inline.popup {
        Some(Popup::Color { position, .. }) => *position,
        _ => panic!(),
    };
    let rect = control_popup::popup_rect(position, Rect::new(0., 0., 1440., 900.), [284., 290.]);
    let square = [rect.min[0] + 100., rect.min[1] + 70.];
    frame(&mut editor, &mut atlas, press(square, MouseButton::Left));
    frame(&mut editor, &mut atlas, release(square, MouseButton::Left));
    assert_ne!(editor.document, before);
    assert!(editor.undo.is_empty());
    assert_eq!(
        editor.binding_value(&Binding::Constant("color".into(), 3)),
        Some(1.)
    );
    let alpha = [
        rect.min[0] + 48. + (rect.width() - 60.) * 0.3,
        rect.min[1] + 138. + 3. * 28. + 22.,
    ];
    frame(&mut editor, &mut atlas, press(alpha, MouseButton::Left));
    frame(&mut editor, &mut atlas, release(alpha, MouseButton::Left));
    assert!(
        editor
            .binding_value(&Binding::Constant("color".into(), 3))
            .unwrap()
            < 0.5
    );
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(!editor.editing());
    assert_eq!(editor.undo.len(), 1);
    motion_graph::compile(&editor.document, &editor.options).unwrap();
    let applied = editor.document.clone();
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    assert!(editor.redo());
    assert_eq!(editor.document, applied);
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    frame(&mut editor, &mut atlas, press(square, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        Input {
            focus_lost: true,
            ..Default::default()
        },
    );
    assert_eq!(editor.document, applied);
    assert_eq!(editor.undo.len(), 1);
    assert!(!editor.editing());
}

#[test]
fn popup_close_consumes_click_and_boolean_toggle_does_not_select_or_drag() {
    let mut editor = simple();
    let id = editor
        .add_node(NodeOp::Bool { value: false }, [700., 100.])
        .unwrap();
    editor.undo.clear();
    editor.select(Some("number"), false);
    let selected = editor.selected.clone();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = center(row(&editor, &id, 0, false));
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    assert_eq!(editor.selected, selected);
    assert!(editor.drag.is_none());
    assert_eq!(editor.undo.len(), 1);
    assert!(matches!(
        editor
            .document
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .operation,
        NodeOp::Bool { value: true }
    ));
    let point = center(row(&editor, "math", 0, false));
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        press([1200., 700.], MouseButton::Left),
    );
    assert!(!editor.popup_open());
    assert!(editor.drag.is_none());
    assert_eq!(editor.selected, selected);
}

#[test]
fn initial_layout_leaves_room_for_taller_controls_and_choices_filter_unsupported_types() {
    let editor = NodeEditor::default();
    for a in &editor.document.nodes {
        for b in &editor.document.nodes {
            if a.id != b.id && a.position[0] == b.position[0] {
                let a_end = a.position[1] + node_height(a, &editor.document);
                assert!(
                    a_end <= b.position[1]
                        || b.position[1] + node_height(b, &editor.document) <= a.position[1]
                );
            }
        }
    }
    let node = Node::new(
        "op",
        NodeOp::Math {
            operation: MathOp::Add,
            value_type: ValueType::Float,
        },
    );
    let choices = inline::controls(&node, &editor.document, Language::English);
    if let Control::Choice { items, .. } = &choices[0] {
        assert!(items.iter().all(|item| !matches!(
            item.operation,
            NodeOp::Math {
                operation: MathOp::Dot | MathOp::Length | MathOp::Normalize,
                ..
            }
        )));
    } else {
        panic!();
    }
}

#[test]
fn focus_cancellation_notifies_host_and_blocks_external_inspector_edits() {
    let mut editor = simple();
    editor.select(Some("number"), false);
    let before = editor.document.clone();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = center(row(&editor, "number", 0, false));
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    frame(&mut editor, &mut atlas, held([point[0] + 40., point[1]]));
    editor.draw_inspector(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 0., 252., 500.),
        &press([100., 103.], MouseButton::Left),
        Language::English,
    );
    assert!(editor.document.nodes.iter().any(|node| node.id == "number"));
    assert_ne!(editor.document, before);
    let mut panels = crate::panels::Panels::default();
    panels.node_editor = editor;
    let actions = crate::panels::cancel_interaction(&mut panels);
    assert!(matches!(
        actions.as_slice(),
        [crate::panels::Action::Node(Action::PreviewChanged)]
    ));
    assert_eq!(panels.node_editor.document, before);
    assert!(!panels.node_editor.editing());
}

#[test]
fn compact_color_popup_scrolls_to_alpha_and_tiny_panel_cancels_preview() {
    let mut editor = simple();
    let original = editor.document.clone();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = center(row(&editor, "color", 0, false));
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    let position = match &editor.inline.popup {
        Some(Popup::Color { position, .. }) => *position,
        _ => panic!(),
    };
    let area = Rect::new(0., 0., 1440., 160.);
    let rect = control_popup::popup_rect(position, area, [284., 180.]);
    editor.draw(
        &mut PaintList::default(),
        &mut atlas,
        area,
        &Input {
            mouse: [rect.min[0] + 100., rect.min[1] + 80.],
            wheel: [0., 100.],
            ..Default::default()
        },
        Language::English,
    );
    let scroll = match &editor.inline.popup {
        Some(Popup::Color { scroll, .. }) => *scroll,
        _ => panic!(),
    };
    assert!(scroll > 0.);
    let alpha = [rect.min[0] + 100., rect.min[1] + 32. + 84. - scroll + 22.];
    editor.draw(
        &mut PaintList::default(),
        &mut atlas,
        area,
        &press(alpha, MouseButton::Left),
        Language::English,
    );
    assert!(
        editor
            .binding_value(&Binding::Constant("color".into(), 3))
            .unwrap()
            < 1.
    );
    let response = editor.draw(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 0., 200., 100.),
        &Input::default(),
        Language::English,
    );
    assert!(response.semantic_changed);
    assert_eq!(editor.document, original);
    assert!(!editor.editing());
}

#[test]
fn numeric_save_commits_valid_text_and_invalid_text_blocks_the_request() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = center(row(&editor, "number", 0, false));
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    frame(&mut editor, &mut atlas, release(point, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "no number".into(),
            ..Default::default()
        },
    );
    let save = Input {
        keys: vec![(
            Key::S,
            Modifiers {
                control: true,
                ..Default::default()
            },
        )],
        ..Default::default()
    };
    let response = frame(&mut editor, &mut atlas, save.clone());
    assert!(response.actions.is_empty());
    assert!(editor.inline.number.is_some());
    frame(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![(
                Key::A,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            text: "8.5".into(),
            ..Default::default()
        },
    );
    let response = frame(&mut editor, &mut atlas, save);
    assert_eq!(value(&editor), 8.5);
    assert!(!editor.editing());
    assert_eq!(editor.undo.len(), 1);
    assert!(response.actions.iter().any(
        |action| matches!(action,Action::Save(snapshot) if snapshot.document == editor.document)
    ));
}
