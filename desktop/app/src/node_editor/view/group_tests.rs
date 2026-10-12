use super::super::tests::{frame, press, release};
use super::*;
use motion_ui::input::{Key, Modifiers, MouseButton};

fn key(key: Key, control: bool, alt: bool) -> Input {
    Input {
        keys: vec![(
            key,
            Modifiers {
                control,
                alt,
                shift: false,
            },
        )],
        ..Default::default()
    }
}
fn editor() -> NodeEditor {
    let mut editor = NodeEditor::default();
    editor.library_open = false;
    editor.inspector_open = false;
    editor.first_frame = false;
    editor.focus = true;
    editor
}
fn pack(editor: &mut NodeEditor) -> String {
    editor.select(Some("circle"), false);
    editor.select(Some("fill"), true);
    assert!(editor.group_selected().unwrap());
    editor.primary.clone().unwrap()
}
fn node_map(document: &GraphDocument) -> BTreeMap<String, Node> {
    document
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect()
}
fn edges(document: &GraphDocument) -> BTreeSet<(PortRef, PortRef)> {
    document
        .connections
        .iter()
        .map(|edge| (edge.from.clone(), edge.to.clone()))
        .collect()
}

#[test]
fn group_boundaries_preserve_types_units_export_and_atomic_undo_then_ungroup() {
    let mut editor = editor();
    let original = editor.snapshot();
    let normal = original.compile().unwrap();
    let id = pack(&mut editor);
    let grouped = editor.snapshot();
    let compiled = grouped.compile().unwrap();
    assert_eq!(
        compiled.report.total_sample_sites,
        normal.report.total_sample_sites
    );
    assert_eq!(
        serde_json::to_value(compiled.manifest).unwrap(),
        serde_json::to_value(normal.manifest).unwrap()
    );
    let ports = motion_graph::node_port_specs(
        editor
            .document
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap(),
        &editor.document.public_parameters,
    )
    .unwrap();
    assert!(ports
        .inputs
        .values()
        .any(|port| port.semantic == motion_graph::PortSemantic::PositionPixels));
    assert_eq!(ports.outputs.len(), 1);
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, original.document);
    assert!(editor.redo());
    assert_eq!(editor.document, grouped.document);
    editor.select(Some(&id), false);
    assert!(editor.ungroup_selected().unwrap());
    assert_eq!(node_map(&editor.document), node_map(&original.document));
    assert_eq!(edges(&editor.document), edges(&original.document));
    assert_eq!(editor.snapshot().compile().unwrap().source, normal.source);
    assert!(editor.undo());
    assert_eq!(editor.document, grouped.document);
}

#[test]
fn nested_scope_edits_save_full_root_shared_parameters_and_reopen_folded_nodes() {
    let mut editor = editor();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let id = pack(&mut editor);
    let root_view = editor.view;
    editor.saved_document = Some(editor.root_document());
    assert!(editor.enter_group(&id));
    assert!(!editor.dirty());
    assert_eq!(editor.parents.len(), 1);
    assert!(
        matches!(&editor.parents[0].document.nodes.iter().find(|node| node.id == id).unwrap().operation,NodeOp::Group { graph } if graph.nodes.is_empty())
    );
    editor.select(Some("circle"), false);
    editor.group_selected().unwrap();
    let nested = editor.primary.clone().unwrap();
    assert!(editor.enter_group(&nested));
    editor.toggle_collapsed("circle");
    editor.set_parameter_default("radius", 0, 18.).unwrap();
    let snapshot = editor.snapshot();
    snapshot.validate().unwrap();
    snapshot.compile().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("nested.msgraph");
    snapshot.save(&path).unwrap();
    editor.accept_saved(&snapshot, path.clone());
    assert!(!editor.dirty());
    let loaded = GraphDocument::load(&path).unwrap();
    assert_eq!(loaded, snapshot.document);
    let mut reopened = NodeEditor::new(loaded);
    assert!(reopened.enter_group(&id));
    assert!(reopened.enter_group(&nested));
    assert!(
        reopened
            .document
            .nodes
            .iter()
            .find(|node| node.id == "circle")
            .unwrap()
            .collapsed
    );
    assert_eq!(
        reopened
            .document
            .public_parameters
            .iter()
            .find(|p| p.id == "radius")
            .unwrap()
            .default[0],
        18.
    );
    assert!(editor.leave_group());
    assert!(editor.leave_group());
    assert_eq!(editor.view.pan, root_view.pan);
    assert_eq!(editor.view.zoom, root_view.zoom);
    assert_eq!(editor.document, snapshot.document);
    assert!(editor.undo());
    assert_eq!(editor.parents.len(), 2);
    assert_ne!(editor.snapshot().document, snapshot.document);
    assert!(editor.redo());
    assert_eq!(editor.snapshot().document, snapshot.document);
}

#[test]
fn group_duplication_respects_recursive_budgets_and_output_uniqueness() {
    let mut editor = editor();
    let leaf_count = editor.document.nodes.len();
    let id = pack(&mut editor);
    let before = editor.snapshot();
    editor.options.max_nodes = leaf_count;
    assert_eq!(
        editor.duplicate_selected().unwrap_err().code,
        ErrorCode::Budget
    );
    assert_eq!(editor.document, before.document);
    assert_eq!(editor.undo.len(), 1);
    editor.options = CompileOptions::default();
    editor.select(Some("output"), false);
    editor.group_selected().unwrap();
    let before = editor.snapshot();
    assert_eq!(
        editor.duplicate_selected().unwrap_err().code,
        ErrorCode::MultipleOutputs
    );
    assert_eq!(editor.document, before.document);
    editor.select(Some(&id), false);
    editor.duplicate_selected().unwrap();
    let copy = editor.primary.clone().unwrap();
    let incoming: Vec<_> = editor
        .document
        .connections
        .iter()
        .filter(|edge| edge.to.node == id)
        .cloned()
        .collect();
    for edge in incoming {
        editor
            .connect_ports(edge.from, PortRef::new(&copy, edge.to.port))
            .unwrap();
    }
    editor.ungroup_selected().unwrap();
    editor.snapshot().validate().unwrap();
    let ids: BTreeSet<_> = editor.document.nodes.iter().map(|node| &node.id).collect();
    assert_eq!(ids.len(), editor.document.nodes.len());
}

#[test]
fn boundaries_cannot_be_deleted_or_regrouped_and_nested_cycles_are_atomic() {
    let mut editor = editor();
    let id = pack(&mut editor);
    assert!(editor.enter_group(&id));
    let input = editor
        .document
        .nodes
        .iter()
        .find(|node| matches!(node.operation, NodeOp::GroupInput { .. }))
        .unwrap()
        .id
        .clone();
    let output = editor
        .document
        .nodes
        .iter()
        .find(|node| matches!(node.operation, NodeOp::GroupOutput { .. }))
        .unwrap()
        .id
        .clone();
    editor.select(Some(&input), false);
    assert!(!editor.delete_selected());
    assert!(!editor.group_selected().unwrap());
    editor.select(Some(&output), false);
    assert!(!editor.delete_selected());
    let before = editor.document.clone();
    assert_eq!(
        editor
            .connect_ports(
                PortRef::new("fill", "coverage"),
                PortRef::new("circle", "radius")
            )
            .unwrap_err()
            .code,
        ErrorCode::Cycle
    );
    assert_eq!(editor.document, before);
    editor.select(Some("circle"), false);
    editor.group_selected().unwrap();
    editor.snapshot().compile().unwrap();
}

#[test]
fn shortcuts_double_click_navigation_and_header_fold_are_independent_of_drag() {
    let mut editor = editor();
    editor.select(Some("circle"), false);
    editor.select(Some("fill"), true);
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, key(Key::G, true, false));
    let id = editor.primary.clone().unwrap();
    assert!(matches!(
        editor.document.nodes.last().unwrap().operation,
        NodeOp::Group { .. }
    ));
    let before = editor.snapshot();
    frame(&mut editor, &mut atlas, key(Key::Tab, false, false));
    assert_eq!(editor.parents.len(), 1);
    assert_eq!(editor.snapshot().document, before.document);
    frame(&mut editor, &mut atlas, key(Key::Tab, true, false));
    assert!(editor.parents.is_empty());
    let node = editor
        .document
        .nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap();
    let point = editor.view.world_to_screen(
        [node.position[0] + 80., node.position[1] + 12.],
        editor.last_canvas,
    );
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    frame(&mut editor, &mut atlas, release(point, MouseButton::Left));
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    assert_eq!(editor.parents.len(), 1);
    assert!(editor.drag.is_none());
    frame(&mut editor, &mut atlas, key(Key::Tab, true, false));
    frame(&mut editor, &mut atlas, key(Key::G, false, true));
    assert!(editor
        .document
        .nodes
        .iter()
        .all(|node| !matches!(node.operation, NodeOp::Group { .. })));
}

#[test]
fn collapsed_cards_keep_visible_wires_but_no_controls_or_connectable_sockets() {
    let mut editor = editor();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.snapshot();
    let shader = before.compile().unwrap().source;
    let node = editor
        .document
        .nodes
        .iter()
        .find(|node| node.id == "circle")
        .unwrap();
    let point = editor.view.world_to_screen(
        [
            node.position[0] + view::NODE_WIDTH - 16.,
            node.position[1] + 14.,
        ],
        editor.last_canvas,
    );
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    assert!(
        editor
            .document
            .nodes
            .iter()
            .find(|node| node.id == "circle")
            .unwrap()
            .collapsed
    );
    assert!(editor.drag.is_none());
    assert_eq!(editor.semantic_revision, before.semantic_revision);
    assert_eq!(editor.snapshot().compile().unwrap().source, shader);
    let cards = editor.cards(editor.last_canvas);
    let card = cards.iter().find(|card| card.id == "circle").unwrap();
    assert!((card.rect.height() - 29. * editor.view.zoom).abs() < 1e-5);
    assert!(card.sockets.iter().all(|socket| !socket.connectable));
    for edge in editor
        .document
        .connections
        .iter()
        .filter(|edge| edge.from.node == "circle" || edge.to.node == "circle")
    {
        assert!(cards
            .iter()
            .flat_map(|card| &card.sockets)
            .any(|socket| socket.output && socket.port == edge.from));
        assert!(cards
            .iter()
            .flat_map(|card| &card.sockets)
            .any(|socket| !socket.output && socket.port == edge.to));
    }
    let input = card
        .sockets
        .iter()
        .find(|socket| !socket.output)
        .unwrap()
        .point;
    frame(&mut editor, &mut atlas, press(input, MouseButton::Left));
    assert!(!matches!(
        editor.drag,
        Some(Drag::Wire { .. } | Drag::WireInput { .. })
    ));
    editor.cancel_interaction();
    assert!(editor.undo());
    assert!(
        !editor
            .document
            .nodes
            .iter()
            .find(|node| node.id == "circle")
            .unwrap()
            .collapsed
    );
}

#[test]
fn preview_inside_a_group_uses_root_dependencies_without_mutating_export() {
    let mut editor = editor();
    let id = pack(&mut editor);
    let original = editor.snapshot();
    assert!(editor.enter_group(&id));
    editor
        .toggle_preview(PortRef::new("circle", "distance"))
        .unwrap();
    let preview = editor.snapshot();
    assert_eq!(preview.document, original.document);
    assert_eq!(
        preview.preview_target.as_ref().unwrap().node,
        format!("{id}/circle")
    );
    preview.compile_preview().unwrap();
    assert_eq!(
        preview.compile().unwrap().source,
        original.compile().unwrap().source
    );
    assert!(editor.leave_group());
    assert!(editor.preview_target.is_none());
    assert_eq!(editor.snapshot().document, original.document);
}

#[test]
fn exposing_new_group_ports_updates_parent_types_and_can_rewire_a_new_input() {
    let mut editor = editor();
    editor.selected = editor
        .document
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect();
    editor.group_selected().unwrap();
    let id = editor.primary.clone().unwrap();
    assert!(editor.enter_group(&id));
    assert!(editor
        .expose_group_port(PortRef::new("circle", "distance"), true)
        .unwrap());
    assert!(!editor
        .expose_group_port(PortRef::new("circle", "distance"), true)
        .unwrap());
    assert!(editor
        .expose_group_port(PortRef::new("circle", "radius"), false)
        .unwrap());
    let pending = editor.snapshot();
    assert_eq!(
        pending.validate().unwrap_err().diagnostic().code,
        ErrorCode::MissingInput
    );
    assert!(editor.leave_group());
    let group = editor
        .document
        .nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap();
    let ports = motion_graph::node_port_specs(group, &editor.document.public_parameters).unwrap();
    assert_eq!(ports.inputs.len(), 1);
    assert_eq!(ports.outputs.len(), 1);
    assert_eq!(
        ports.outputs.values().next().unwrap().value_type,
        ValueType::DistancePixels
    );
    let input = ports.inputs.keys().next().unwrap().clone();
    let value = editor
        .add_node(NodeOp::Float { value: 18. }, [-250., 0.])
        .unwrap();
    editor
        .connect_ports(PortRef::new(value, "value"), PortRef::new(&id, input))
        .unwrap();
    editor.snapshot().compile().unwrap();
    assert!(editor.enter_group(&id));
    editor.snapshot().compile().unwrap();
}
