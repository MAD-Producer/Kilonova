use super::*;
use motion_ui::input::{Key, Modifiers, MouseButton};

#[test]
fn incomplete_unused_nodes_keep_live_validation_and_history_reversible() {
    let mut editor = simple();
    assert!(editor.diagnostic().is_none());
    let original = editor.snapshot().compile().unwrap();
    editor
        .connect_ports(PortRef::new("number", "value"), PortRef::new("math", "a"))
        .unwrap();
    assert!(editor.diagnostic().is_none());
    assert_eq!(editor.snapshot().compile().unwrap().source, original.source);
    let invert = editor
        .add_node(
            NodeOp::ColorMath {
                operation: ColorOp::Invert,
            },
            [650., 260.],
        )
        .unwrap();
    assert!(editor.diagnostic().is_none());
    editor
        .connect_ports(
            PortRef::new(&invert, "color"),
            PortRef::new("output", "color"),
        )
        .unwrap();
    let diagnostic = editor.diagnostic().unwrap();
    assert_eq!(diagnostic.code, ErrorCode::MissingInput);
    assert_eq!(diagnostic.node.as_deref(), Some(invert.as_str()));
    assert_eq!(diagnostic.port.as_deref(), Some("color"));
    assert!(editor.undo());
    assert!(editor.diagnostic().is_none());
    assert_eq!(editor.snapshot().compile().unwrap().source, original.source);
    assert!(editor.redo());
    assert_eq!(editor.diagnostic().unwrap().code, ErrorCode::MissingInput);
    editor
        .connect_ports(
            PortRef::new("color", "value"),
            PortRef::new(&invert, "color"),
        )
        .unwrap();
    assert!(editor.diagnostic().is_none());
    assert_ne!(editor.snapshot().compile().unwrap().source, original.source);
}

pub(super) fn simple() -> NodeEditor {
    let mut graph = demo_graph();
    graph.public_parameters.clear();
    graph.nodes = vec![
        Node::new(
            "color",
            NodeOp::Color {
                value: [0.1, 0.6, 1., 1.],
            },
        ),
        Node::new("output", NodeOp::ColorOutput),
        Node::new("number", NodeOp::Float { value: 1. }),
        Node::new(
            "math",
            NodeOp::Math {
                operation: MathOp::Add,
                value_type: ValueType::Float,
            },
        ),
    ];
    for (node, position) in
        graph
            .nodes
            .iter_mut()
            .zip([[20., 80.], [380., 80.], [20., 260.], [380., 260.]])
    {
        node.position = position;
    }
    graph.connections = vec![Connection::new("color", "value", "output", "color")];
    let mut editor = NodeEditor::new(graph);
    editor.first_frame = false;
    editor.library_open = false;
    editor.view = CanvasView {
        pan: [30., 30.],
        zoom: 1.,
    };
    editor
}

#[test]
fn duplicate_subgraph_copies_only_internal_edges_and_undo_restores_it_once() {
    let mut editor = simple();
    editor
        .document
        .connections
        .push(Connection::new("number", "value", "math", "a"));
    editor.document.nodes.push(Node::new(
        "outside",
        NodeOp::Math {
            operation: MathOp::Add,
            value_type: ValueType::Float,
        },
    ));
    editor
        .document
        .connections
        .push(Connection::new("math", "value", "outside", "a"));
    editor.selected = [String::from("number"), String::from("math")].into();
    editor.primary = Some("number".into());
    editor.focus_panel();
    let before = editor.snapshot();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let input = Input {
        keys: vec![(
            Key::D,
            Modifiers {
                control: true,
                ..Default::default()
            },
        )],
        ..Default::default()
    };
    let result = frame(&mut editor, &mut atlas, input.clone());
    assert!(result.changed && result.semantic_changed);
    assert_eq!(editor.document.nodes.len(), before.document.nodes.len() + 2);
    assert_eq!(
        editor.document.connections.len(),
        before.document.connections.len() + 1
    );
    assert_eq!(editor.selected.len(), 2);
    assert!(editor
        .selected
        .iter()
        .all(|id| !before.document.nodes.iter().any(|n| &n.id == id)));
    let copied_edge = editor.document.connections.last().unwrap();
    assert!(
        editor.selected.contains(&copied_edge.from.node)
            && editor.selected.contains(&copied_edge.to.node)
    );
    for node in editor
        .document
        .nodes
        .iter()
        .filter(|n| editor.selected.contains(&n.id))
    {
        let original = before
            .document
            .nodes
            .iter()
            .find(|n| n.operation == node.operation)
            .unwrap();
        assert_eq!(node.position, original.position.map(|v| v + 32.));
    }
    let copied = editor.snapshot().document;
    let copied_selection = editor.selected.clone();
    let mut repeated = input;
    repeated.key_repeats = vec![true];
    frame(&mut editor, &mut atlas, repeated);
    assert_eq!(editor.document, copied);
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before.document);
    assert!(editor.redo());
    assert_eq!(editor.document, copied);
    editor.selected = copied_selection;
    editor.delete_selected();
    assert_eq!(editor.document, before.document);
}

#[test]
fn duplication_rejects_budget_and_unique_output_without_partial_edits() {
    let mut editor = simple();
    editor.select(Some("output"), false);
    let before = editor.snapshot();
    assert_eq!(
        editor.duplicate_selected().unwrap_err().code,
        ErrorCode::MultipleOutputs
    );
    assert_eq!(editor.document, before.document);
    editor.select(Some("number"), false);
    editor.options.max_nodes = editor.document.nodes.len();
    assert_eq!(
        editor.duplicate_selected().unwrap_err().code,
        ErrorCode::Budget
    );
    assert_eq!(editor.document, before.document);
    assert_eq!(editor.revision, before.revision);
    assert!(editor.undo.is_empty());
}

#[test]
fn node_body_selects_without_dragging_and_header_moves_the_selected_group() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.snapshot();
    let body = editor.view.world_to_screen([60., 130.], editor.last_canvas);
    frame(&mut editor, &mut atlas, press(body, MouseButton::Left));
    assert!(editor.drag.is_none());
    frame(
        &mut editor,
        &mut atlas,
        release([body[0] + 40., body[1] + 30.], MouseButton::Left),
    );
    assert_eq!(editor.document, before.document);
    assert_eq!(editor.selected, [String::from("color")].into());
    editor.selected.insert("output".into());
    let header = editor.view.world_to_screen([60., 90.], editor.last_canvas);
    frame(&mut editor, &mut atlas, press(header, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        release([header[0] + 20., header[1] - 10.], MouseButton::Left),
    );
    assert_eq!(editor.document.nodes[0].position, [40., 70.]);
    assert_eq!(editor.document.nodes[1].position, [400., 70.]);
    assert_eq!(editor.undo.len(), 1);
    assert_eq!(editor.semantic_revision, before.semantic_revision);
}

#[test]
fn add_menu_latches_pointer_coordinates_and_consumes_enter_and_outside_click() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    editor.focus_panel();
    frame(&mut editor, &mut atlas, Input::default());
    let mouse = [600., 420.];
    let world = editor.view.screen_to_world(mouse, editor.last_canvas);
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse,
            text: "A".into(),
            keys: vec![(
                Key::A,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        },
    );
    assert!(editor.add_menu.as_ref().unwrap().query.is_empty());
    let node_count = editor.document.nodes.len();
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "Float".into(),
            ..Default::default()
        },
    );
    assert!(editor.add_menu.is_some());
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: [700., 520.],
            keys: vec![(Key::Enter, Modifiers::default())],
            ..Default::default()
        },
    );
    assert!(editor.add_menu.is_none());
    assert_eq!(editor.document.nodes.len(), node_count + 1);
    assert_eq!(editor.document.nodes.last().unwrap().position, world);
    let before = editor.snapshot();
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse,
            keys: vec![(
                Key::A,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        },
    );
    let selection = editor.selected.clone();
    frame(
        &mut editor,
        &mut atlas,
        press([50., 700.], MouseButton::Left),
    );
    assert!(editor.add_menu.is_none());
    assert!(editor.drag.is_none());
    assert_eq!(editor.selected, selection);
    assert_eq!(editor.document, before.document);
}

#[test]
fn node_navigation_is_layout_only_and_text_keeps_ime_commits() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    editor.focus_panel();
    editor.select(Some("number"), false);
    let before = editor.snapshot();
    frame(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![
                (Key::Right, Modifiers::default()),
                (
                    Key::Down,
                    Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                ),
            ],
            ..Default::default()
        },
    );
    assert_eq!(editor.document.nodes[2].position, [21., 270.]);
    assert_eq!(editor.semantic_revision, before.semantic_revision);
    editor.field = Some(active_field(Binding::GraphName(true), ""));
    let mut input = Input::default();
    motion_ui::ui::accumulate(
        &mut input,
        motion_ui::input::Event::TextCommit("新图".into()),
    );
    frame(&mut editor, &mut atlas, input);
    assert_eq!(editor.field.as_ref().unwrap().text, "新图");
    frame(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![(Key::X, Modifiers::default())],
            ..Default::default()
        },
    );
    assert_eq!(editor.document.nodes.len(), before.document.nodes.len());
}

#[test]
fn f_frames_selection_and_arrange_preserves_external_nodes_and_edges() {
    let mut editor = simple();
    editor
        .document
        .connections
        .push(Connection::new("number", "value", "math", "a"));
    editor.select(Some("number"), false);
    let canvas = Rect::new(0., 0., 500., 600.);
    editor.fit_all(canvas);
    let all_zoom = editor.view.zoom;
    editor.fit_selected(canvas);
    assert!(editor.view.zoom > all_zoom);
    editor.selected.insert("math".into());
    let before = editor.snapshot();
    assert!(editor.arrange_selected());
    assert!(editor.document.nodes[3].position[0] > editor.document.nodes[2].position[0]);
    assert_eq!(editor.document.nodes[..2], before.document.nodes[..2]);
    assert_eq!(editor.document.connections, before.document.connections);
    assert_eq!(editor.semantic_revision, before.semantic_revision);
    assert!(editor.undo());
    assert_eq!(editor.document, before.document);
}

fn output(id: &str) -> PortRef {
    PortRef::new(id, "value")
}

#[test]
fn home_fits_distant_nodes_and_ctrl_wheel_retains_the_anchor_at_fit_scale() {
    let mut editor = simple();
    editor.document.nodes[0].position = [-1.0e6, -1.0e6];
    editor.document.nodes[1].position = [1.0e6, 1.0e6];
    let canvas = Rect::new(100., 70., 720., 600.);
    editor.fit_all(canvas);
    assert!(editor.view.zoom < 0.25);
    for node in &editor.document.nodes {
        let start = editor.view.world_to_screen(node.position, canvas);
        let end = editor.view.world_to_screen(
            [
                node.position[0] + view::NODE_WIDTH,
                node.position[1] + view::node_height(node, &editor.document),
            ],
            canvas,
        );
        assert!(canvas.contains(start) && canvas.contains(end));
    }
    let mouse = [430., 240.];
    let anchor = editor.view.screen_to_world(mouse, canvas);
    editor.view.zoom_at(editor.view.zoom * 1.12, mouse, canvas);
    let after = editor.view.screen_to_world(mouse, canvas);
    assert!(anchor.iter().zip(after).all(|(a, b)| (*a - b).abs() < 0.25));
    assert!(editor.undo.is_empty());
}
fn active_field(binding: Binding, text: &str) -> Field {
    Field {
        binding,
        text: text.into(),
        cursor: text.len(),
        replace: false,
        editor: Default::default(),
        error: None,
    }
}

#[test]
fn demo_validates_and_compiles() {
    let editor = NodeEditor::default();
    editor.snapshot().validate().unwrap();
    editor.snapshot().compile().unwrap();
}

#[test]
fn semantic_coordinate_mismatch_preserves_existing_wire_and_document() {
    let mut editor = NodeEditor::default();
    let uv = editor.add_node(NodeOp::Uv, [0., 0.]).unwrap();
    let before = editor.snapshot();
    let error = editor
        .connect_ports(
            PortRef::new(&uv, "value"),
            PortRef::new("circle", "position"),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::TypeMismatch);
    assert_eq!(editor.snapshot().document, before.document);
    assert_eq!(editor.revision, before.revision);
}

#[test]
fn catalog_uses_only_n1_ports() {
    let mut editor = NodeEditor::new(demo_graph());
    editor.document.nodes.clear();
    editor.document.connections.clear();
    editor.document.public_parameters.clear();
    let items = catalog();
    assert!(items.len() > 50);
    for item in items {
        let id = if let Some(ty) = item.parameter_type {
            editor.add_public_parameter(ty, [0., 0.]).unwrap()
        } else {
            editor.add_node(item.operation, [0., 0.]).unwrap()
        };
        let node = editor.document.nodes.iter().find(|n| n.id == id).unwrap();
        node_ports(node, &editor.document.public_parameters).unwrap();
    }
}

#[test]
fn connections_reject_types_and_cycles_without_losing_old_input() {
    let mut editor = simple();
    let before = editor.document.clone();
    assert_eq!(
        editor
            .connect_ports(output("number"), PortRef::new("output", "color"))
            .unwrap_err()
            .code,
        ErrorCode::TypeMismatch
    );
    assert_eq!(editor.document, before);
    editor
        .connect_ports(output("number"), PortRef::new("math", "a"))
        .unwrap();
    let second = editor
        .add_node(
            NodeOp::Math {
                operation: MathOp::Add,
                value_type: ValueType::Float,
            },
            [700., 260.],
        )
        .unwrap();
    editor
        .connect_ports(output("math"), PortRef::new(&second, "a"))
        .unwrap();
    let before = editor.document.clone();
    assert_eq!(
        editor
            .connect_ports(output(&second), PortRef::new("math", "a"))
            .unwrap_err()
            .code,
        ErrorCode::Cycle
    );
    assert_eq!(editor.document, before);
    assert_eq!(
        editor
            .connect_ports(output("math"), PortRef::new("math", "b"))
            .unwrap_err()
            .code,
        ErrorCode::Cycle
    );
    assert_eq!(
        editor
            .connect_ports(PortRef::new("number", "missing"), PortRef::new("math", "b"))
            .unwrap_err()
            .code,
        ErrorCode::UnknownPort
    );
}

#[test]
fn type_change_removes_incompatible_wires_and_undo_restores() {
    let mut editor = simple();
    editor
        .connect_ports(output("number"), PortRef::new("math", "a"))
        .unwrap();
    let before = editor.document.clone();
    editor
        .set_operation(
            "math",
            NodeOp::Math {
                operation: MathOp::Add,
                value_type: ValueType::Vec2,
            },
        )
        .unwrap();
    assert!(!editor
        .document
        .connections
        .iter()
        .any(|c| c.to.node == "math"));
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    assert!(editor.redo());
    assert!(!editor
        .document
        .connections
        .iter()
        .any(|c| c.to.node == "math"));
}

#[test]
fn deleting_parameter_node_preserves_parameter_slots() {
    let mut editor = NodeEditor::default();
    let before = editor.document.clone();
    let id = editor
        .document
        .nodes
        .iter()
        .find(|n| matches!(n.operation, NodeOp::Parameter { .. }))
        .unwrap()
        .id
        .clone();
    editor.select(Some(&id), false);
    assert!(editor.delete_selected());
    assert_eq!(editor.document.public_parameters, before.public_parameters);
    assert!(editor
        .document
        .connections
        .iter()
        .all(|c| c.from.node != id && c.to.node != id));
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}

#[test]
fn numeric_editing_is_finite_typed_and_atomic() {
    let mut editor = simple();
    editor.set_constant("number", 0, 12.5).unwrap();
    let before = editor.document.clone();
    for value in [f32::NAN, f32::INFINITY, 1.1e6] {
        assert!(editor.set_constant("number", 0, value).is_err());
    }
    assert!(editor.set_constant("color", 3, 2.).is_err());
    assert!(editor
        .add_node(NodeOp::Float { value: f32::NAN }, [0., 0.])
        .is_err());
    assert!(editor
        .add_public_parameter(ValueType::Float, [f32::NAN, 0.])
        .is_err());
    assert_eq!(editor.document, before);
    let node = editor
        .add_public_parameter(ValueType::Color, [0., 0.])
        .unwrap();
    let parameter = match &editor
        .document
        .nodes
        .iter()
        .find(|n| n.id == node)
        .unwrap()
        .operation
    {
        NodeOp::Parameter { parameter } => parameter.clone(),
        _ => unreachable!(),
    };
    assert!(editor
        .apply_field(Binding::ParamMin(parameter.clone()), "2")
        .is_err());
    assert!(editor
        .apply_field(Binding::ParamStep(parameter.clone()), "0")
        .is_err());
    assert!(editor.set_parameter_default(&parameter, 3, 1.5).is_err());
    editor.set_parameter_default(&parameter, 3, 0.25).unwrap();
    assert_eq!(
        editor.document.public_parameters.last().unwrap().default[3],
        0.25
    );
}

#[test]
fn action_snapshots_commit_valid_fields_and_are_owned() {
    let mut editor = simple();
    editor.field = Some(active_field(Binding::Constant("number".into(), 0), "9"));
    let Action::Save(snapshot) = editor.request(Request::Save).unwrap() else {
        panic!()
    };
    assert!(editor.field.is_none());
    editor.set_constant("number", 0, 10.).unwrap();
    assert!(matches!(
        snapshot.document.nodes[2].operation,
        NodeOp::Float { value: 9. }
    ));
    for request in [
        Request::Open,
        Request::SaveAs,
        Request::Validate,
        Request::Compile,
        Request::ExportMsfx,
    ] {
        let action = editor.request(request).unwrap();
        let snapshot = match action {
            Action::Open(s)
            | Action::SaveAs(s)
            | Action::Validate(s)
            | Action::Compile(s)
            | Action::ExportMsfx(s) => s,
            _ => panic!(),
        };
        assert_eq!(snapshot.document, editor.document);
    }
    editor.field = Some(active_field(Binding::Constant("number".into(), 0), "bad"));
    assert!(editor.request(Request::Save).is_err());
    assert!(editor.field.as_ref().unwrap().error.is_some());
}

#[test]
fn stale_worker_results_and_save_completion_order_are_safe() {
    let mut editor = NodeEditor::default();
    let initial = editor.snapshot();
    let report = initial.compile().unwrap().report;
    assert!(editor.accept_compile(&initial, Ok(report.clone())));
    editor
        .add_node(NodeOp::Float { value: 2. }, [0., 0.])
        .unwrap();
    let current = editor.snapshot();
    assert!(!editor.accept_compile(&initial, Ok(report.clone())));
    assert!(!editor.accept_validation(&initial, initial.validate().map_err(|e| e.diagnostic())));
    assert!(!editor.accept_open(&initial, demo_graph(), "old.msgraph".into()));
    assert!(editor.compiled.is_some());
    editor.request(Request::Compile).unwrap();
    editor.undo();
    assert!(editor.pending_compile.is_none());
    assert!(editor.accept_saved(&current, "new.msgraph".into()));
    assert!(!editor.accept_saved(&initial, "old.msgraph".into()));
    assert_eq!(
        editor.source_path.as_deref(),
        Some(std::path::Path::new("new.msgraph"))
    );
    editor.replace_document(demo_graph(), Some("opened.msgraph".into()));
    assert!(!editor.accept_saved(&current, "new.msgraph".into()));
    assert!(!editor.accept_compile(&current, Ok(report)));
    assert!(!editor.dirty());
}

#[test]
fn save_reopen_and_real_msfx_export_round_trip() {
    let mut editor = NodeEditor::default();
    let snapshot = editor.snapshot();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("motion-node-n2-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let source = directory.join("circle.msgraph");
    snapshot.save(&source).unwrap();
    assert!(editor.accept_saved(&snapshot, source.clone()));
    assert!(!editor.dirty());
    let reopened = GraphDocument::load(&source).unwrap();
    assert_eq!(reopened, snapshot.document);
    editor.replace_document(reopened, Some(source.clone()));
    let package = directory.join("circle.msfx");
    let report = editor.snapshot().export_msfx(&package).unwrap();
    assert_eq!(report.compile.passes, 1);
    assert!(std::fs::read(&package).unwrap().starts_with(b"PK"));
    assert_eq!(GraphDocument::load(&source).unwrap(), editor.document);
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(package).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

pub(super) fn frame(editor: &mut NodeEditor, atlas: &mut TextAtlas, input: Input) -> FrameResponse {
    editor.draw(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., 1440., 900.),
        &input,
        Language::English,
    )
}
pub(super) fn press(point: [f32; 2], button: MouseButton) -> Input {
    Input {
        mouse: point,
        pressed: Some((button, point)),
        held: Some(button),
        ..Default::default()
    }
}
pub(super) fn release(point: [f32; 2], button: MouseButton) -> Input {
    Input {
        mouse: point,
        released: Some((button, point)),
        ..Default::default()
    }
}

#[test]
fn real_pointer_events_drag_undo_zoom_pan_and_connect_both_directions() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let canvas = editor.last_canvas;
    let start = editor.view.world_to_screen([55., 95.], canvas);
    frame(&mut editor, &mut atlas, press(start, MouseButton::Left));
    let end = [start[0] + 52., start[1] + 31.];
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: end,
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    let response = frame(&mut editor, &mut atlas, release(end, MouseButton::Left));
    assert!(response.changed && !response.semantic_changed);
    assert_eq!(editor.document.nodes[0].position, [72., 111.]);
    assert!(editor.undo());
    assert_eq!(editor.document.nodes[0].position, [20., 80.]);
    let anchor = [600., 450.];
    let world = editor.view.screen_to_world(anchor, canvas);
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: anchor,
            wheel: [0., 60.],
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let after = editor.view.screen_to_world(anchor, canvas);
    assert!((world[0] - after[0]).abs() < 0.01 && (world[1] - after[1]).abs() < 0.01);
    let pan = editor.view.pan;
    frame(&mut editor, &mut atlas, press(anchor, MouseButton::Middle));
    frame(
        &mut editor,
        &mut atlas,
        release([anchor[0] + 25., anchor[1] + 18.], MouseButton::Middle),
    );
    assert_eq!(editor.view.pan, [pan[0] + 25., pan[1] + 18.]);
    // Restore an easy coordinate system; sockets use N1's sorted port order.
    editor.view = CanvasView {
        pan: [30., 30.],
        zoom: 1.,
    };
    let source = editor.view.world_to_screen([224., 300.5], canvas);
    let target = editor.view.world_to_screen([380., 300.5], canvas);
    frame(&mut editor, &mut atlas, press(source, MouseButton::Left));
    let response = frame(&mut editor, &mut atlas, release(target, MouseButton::Left));
    assert!(response.semantic_changed);
    assert!(editor
        .document
        .connections
        .iter()
        .any(|c| c.from == output("number") && c.to == PortRef::new("math", "a")));
    frame(&mut editor, &mut atlas, press(target, MouseButton::Right));
    assert!(
        editor
            .document
            .connections
            .iter()
            .any(|c| c.to == PortRef::new("math", "a")),
        "opening a context menu cannot disconnect immediately"
    );
    frame(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![(Key::Enter, Modifiers::default())],
            ..Default::default()
        },
    );
    assert!(!editor
        .document
        .connections
        .iter()
        .any(|c| c.to == PortRef::new("math", "a")));
    // A previously unconnected input can initiate the wire too.
    frame(&mut editor, &mut atlas, press(target, MouseButton::Left));
    frame(&mut editor, &mut atlas, release(source, MouseButton::Left));
    assert!(editor
        .document
        .connections
        .iter()
        .any(|c| c.to == PortRef::new("math", "a")));
    let before = editor.document.clone();
    frame(&mut editor, &mut atlas, press(target, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        release([1430., 890.], MouseButton::Left),
    );
    assert_eq!(
        editor.document, before,
        "releasing outside canvas cancels rewire"
    );
}

#[test]
fn wheel_navigation_uses_ctrl_zoom_and_normal_scrolling_without_graph_edits() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let original = editor.snapshot();
    let anchor = [600., 450.];
    let pan = editor.view.pan;
    let zoom = editor.view.zoom;
    let up = crate::mouse_wheel_delta(winit::event::MouseScrollDelta::LineDelta(0., 1.), 1.);
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: anchor,
            wheel: up,
            ..Default::default()
        },
    );
    assert_eq!(editor.view.pan, [pan[0], pan[1] + 32.]);
    assert_eq!(editor.view.zoom, zoom);
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: anchor,
            wheel: [0., 32.],
            ..Default::default()
        },
    );
    assert_eq!(editor.view.pan, pan);
    let world = editor.view.screen_to_world(anchor, editor.last_canvas);
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: anchor,
            wheel: up,
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(editor.view.zoom > zoom, "Ctrl+wheel up zooms in");
    let after = editor.view.screen_to_world(anchor, editor.last_canvas);
    assert!(world.iter().zip(after).all(|(a, b)| (*a - b).abs() < 0.01));
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: anchor,
            wheel: [0., 32.],
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(
        (editor.view.zoom - zoom).abs() < 0.0001,
        "Ctrl+wheel down zooms out"
    );
    let pan = editor.view.pan;
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: anchor,
            wheel: up,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert_eq!(editor.view.pan, [pan[0] + 32., pan[1]]);
    assert_eq!(editor.snapshot().document, original.document);
    assert_eq!(editor.stamp(), (original.epoch, original.semantic_revision));
    assert_eq!(editor.revision, original.revision);
    assert!(editor.undo.is_empty());
}

#[test]
fn marquee_selection_supports_group_drag_modifiers_and_cancel() {
    let mut editor = simple();
    // Keep the lower selection band clear of the expanded color controls.
    for node in &mut editor.document.nodes {
        if node.id == "number" || node.id == "math" {
            node.position[1] = 340.;
        }
    }
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let original = editor.snapshot();
    let canvas = editor.last_canvas;
    let start = editor.view.world_to_screen([10., 60.], canvas);
    let end = editor.view.world_to_screen([600., 210.], canvas);
    frame(&mut editor, &mut atlas, press(start, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: end,
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    let pair: BTreeSet<String> = ["color", "output"].map(String::from).into();
    assert_eq!(editor.selected, pair);
    assert!(matches!(editor.drag, Some(Drag::Marquee { .. })));
    frame(&mut editor, &mut atlas, release(end, MouseButton::Left));
    let bottom = editor.view.world_to_screen([250., 490.], canvas);
    let top = editor.view.world_to_screen([10., 325.], canvas);
    let mut additive = press(bottom, MouseButton::Left);
    additive.modifiers.shift = true;
    frame(&mut editor, &mut atlas, additive);
    frame(&mut editor, &mut atlas, release(top, MouseButton::Left));
    assert_eq!(editor.selected.len(), 3);
    assert!(editor.selected.contains("number"));
    let mut toggle = press(bottom, MouseButton::Left);
    toggle.modifiers.control = true;
    frame(&mut editor, &mut atlas, toggle);
    for _ in 0..2 {
        frame(
            &mut editor,
            &mut atlas,
            Input {
                mouse: top,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        );
        assert_eq!(editor.selected, pair);
    }
    frame(&mut editor, &mut atlas, release(top, MouseButton::Left));
    frame(&mut editor, &mut atlas, press(bottom, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: top,
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    assert_eq!(editor.selected, [String::from("number")].into());
    frame(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![(Key::Escape, Modifiers::default())],
            ..Default::default()
        },
    );
    assert_eq!(editor.selected, pair);
    let pan = editor.view.pan;
    let mut navigation = press(bottom, MouseButton::Left);
    navigation.modifiers.alt = true;
    frame(&mut editor, &mut atlas, navigation);
    let moved = [bottom[0] + 20., bottom[1] - 15.];
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: moved,
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    frame(&mut editor, &mut atlas, release(moved, MouseButton::Left));
    assert_eq!(editor.view.pan, [pan[0] + 20., pan[1] - 15.]);
    assert_eq!(editor.selected, pair);
    assert_eq!(editor.snapshot().document, original.document);
    assert_eq!(editor.revision, original.revision);
    let node = editor.view.world_to_screen([55., 95.], canvas);
    frame(&mut editor, &mut atlas, press(node, MouseButton::Left));
    let moved = [node[0] + 40., node[1] + 25.];
    frame(&mut editor, &mut atlas, release(moved, MouseButton::Left));
    assert_eq!(editor.document.nodes[0].position, [60., 105.]);
    assert_eq!(editor.document.nodes[1].position, [420., 105.]);
    assert_eq!(editor.undo.len(), 1);
    assert_eq!(editor.semantic_revision, original.semantic_revision);
    assert!(editor.undo());
    assert_eq!(editor.snapshot().document, original.document);
}

#[test]
fn wheel_scrolls_node_library_and_inspector_in_the_same_direction() {
    let mut editor = NodeEditor::default();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let layout = editor.layout(Rect::new(0., 0., 1440., 900.));
    let library = layout.library.unwrap();
    editor.library_scroll = 64.;
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: [library.min[0] + 30., library.min[1] + 130.],
            wheel: [0., -32.],
            ..Default::default()
        },
    );
    assert_eq!(editor.library_scroll, 32.);
    editor.select(None, false);
    let area = Rect::new(0., 0., 1440., 340.);
    let inspector = editor.layout(area).inspector.unwrap();
    editor.inspector_scroll = 64.;
    editor.draw(
        &mut PaintList::default(),
        &mut atlas,
        area,
        &Input {
            mouse: [inspector.min[0] + 30., inspector.min[1] + 130.],
            wheel: [0., -32.],
            ..Default::default()
        },
        Language::English,
    );
    assert_eq!(editor.inspector_scroll, 32.);
}

#[test]
fn real_inspector_text_input_and_save_shortcut_capture_committed_value() {
    let mut editor = simple();
    editor.select(Some("number"), false);
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let inspector = editor
        .layout(Rect::new(0., 0., 1440., 900.))
        .inspector
        .unwrap();
    // Embedded header, title/delete row and ID row precede the first field.
    let styles = PaintList::default().styles();
    let point = [
        inspector.min[0] + styles.metric("padding.panel", 8.)
            + styles.metric("label.column", 72.) + styles.metric("padding", 5.) * 2.,
        inspector.min[1] + styles.metric("panel.header.height", 24.)
            + styles.metric("padding.panel", 8.)
            + styles.metric("node.dropdown.row.height", 28.) * 2.5
            + styles.metric("padding", 5.),
    ];
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    assert!(editor.field.is_some());
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "23.5".into(),
            ..Default::default()
        },
    );
    let response = frame(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![(
                Key::S,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        },
    );
    let snapshot = response
        .actions
        .iter()
        .find_map(|a| {
            if let Action::Save(s) = a {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    assert!(matches!(
        snapshot.document.nodes[2].operation,
        NodeOp::Float { value: 23.5 }
    ));
    assert!(response.semantic_changed);
}

#[test]
fn responsive_layout_and_paint_remain_within_panel() {
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for width in [375., 414., 768., 1024., 1440.] {
        let mut editor = NodeEditor::default();
        let area = Rect::new(21., 37., width, 650.);
        let layout = editor.layout(area);
        assert!(layout.canvas.width() > 100.);
        assert!(layout.canvas.min[1] >= layout.toolbar.max[1]);
        assert!(layout.canvas.max[1] <= layout.footer.min[1]);
        for side in [layout.library, layout.inspector].into_iter().flatten() {
            assert!(area.contains_rect(side));
        }
        let mut paint = PaintList::default();
        editor.draw(
            &mut paint,
            &mut atlas,
            area,
            &Input::default(),
            Language::Chinese,
        );
        assert!(!paint.glyphs.is_empty());
        for shape in paint.shapes {
            let points: Vec<_> = match shape {
                motion_ui::paint::Shape::Quad { points } => points.into(),
                motion_ui::paint::Shape::Line { points, clip }
                | motion_ui::paint::Shape::Stroke { points, clip, .. } => {
                    assert!(points.iter().flatten().all(|v| v.is_finite()));
                    clip.map_or(points, |bounds| vec![bounds.min, bounds.max])
                }
                motion_ui::paint::Shape::Triangles { points } => points,
                motion_ui::paint::Shape::Procedural { clip, .. } => vec![clip.min, clip.max],
            };
            assert!(points.iter().all(|p| p[0].is_finite()
                && p[1].is_finite()
                && p[0] >= area.min[0]
                && p[0] <= area.max[0]
                && p[1] >= area.min[1]
                && p[1] <= area.max[1]));
        }
    }
}

#[test]
fn library_search_add_delete_and_focus_transfer_use_real_input() {
    let mut editor = simple();
    editor.library_open = true;
    let mut atlas = TextAtlas::from_system_language(13., true).unwrap();
    let area = Rect::new(0., 0., 1440., 900.);
    let draw = |editor: &mut NodeEditor, atlas: &mut TextAtlas, input: Input| {
        editor.draw(
            &mut PaintList::default(),
            atlas,
            area,
            &input,
            Language::Chinese,
        )
    };
    draw(&mut editor, &mut atlas, Input::default());
    let library = editor.layout(area).library.unwrap();
    draw(
        &mut editor,
        &mut atlas,
        press(
            [library.min[0] + 30., library.min[1] + 49.],
            MouseButton::Left,
        ),
    );
    assert!(editor.search_focus);
    draw(
        &mut editor,
        &mut atlas,
        Input {
            text: "circle".into(),
            ..Default::default()
        },
    );
    assert_eq!(editor.search, "circle");
    let count = editor.document.nodes.len();
    let response = draw(
        &mut editor,
        &mut atlas,
        press(
            [library.min[0] + 50., library.min[1] + 118.],
            MouseButton::Left,
        ),
    );
    assert_eq!(
        editor.document.nodes.len(),
        count + 1,
        "English search adds a circle in Chinese UI"
    );
    assert!(matches!(
        editor.document.nodes.last().unwrap().operation,
        NodeOp::SdfCircle
    ));
    assert!(response.semantic_changed);
    draw(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![(Key::Delete, Modifiers::default())],
            ..Default::default()
        },
    );
    assert_eq!(editor.document.nodes.len(), count);
    editor.field = Some(active_field(Binding::Constant("number".into(), 0), "19"));
    let response = draw(
        &mut editor,
        &mut atlas,
        press([1500., 400.], MouseButton::Left),
    );
    assert!(!response.wants_keyboard);
    assert!(editor.field.is_none());
    assert!(matches!(
        editor.document.nodes[2].operation,
        NodeOp::Float { value: 19. }
    ));
    let before = editor.document.clone();
    draw(
        &mut editor,
        &mut atlas,
        Input {
            keys: vec![(Key::Delete, Modifiers::default())],
            ..Default::default()
        },
    );
    assert_eq!(editor.document, before, "outside panel owns the keyboard");
}
