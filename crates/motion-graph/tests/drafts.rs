use motion_graph::*;

#[path = "support/group_fixture.rs"]
mod fixture;

fn draft_node() -> Node {
    Node::new(
        "draft",
        NodeOp::Math {
            operation: MathOp::Add,
            value_type: ValueType::Float,
        },
    )
}

#[test]
fn unfinished_unused_nodes_compile_and_export_without_changing_the_effect() {
    let options = CompileOptions::default();
    let original = demo_graph();
    let before = compile(&original, &options).unwrap();
    let mut draft = original.clone();
    draft.nodes.push(draft_node());
    draft.nodes.push(Node::new(
        "downstream",
        NodeOp::Splat {
            value_type: ValueType::Vec2,
        },
    ));
    draft.connections.extend([
        Connection::new("zero", "value", "draft", "a"),
        Connection::new("draft", "value", "downstream", "value"),
    ]);
    let plan = validate(&draft, &options).unwrap();
    assert_eq!(plan.unused_nodes, ["draft", "downstream"]);
    let after = compile(&draft, &options).unwrap();
    assert_eq!(before.source, after.source);
    assert_eq!(before.manifest, after.manifest);
    assert_eq!(before.report.parameter_slots, after.report.parameter_slots);
    assert_eq!(
        before.report.total_sample_sites,
        after.report.total_sample_sites
    );
    let folder = tempfile::tempdir().unwrap();
    let report = export_msfx(&draft, &options, &folder.path().join("draft.msfx")).unwrap();
    assert_eq!(GraphDocument::load(&report.source_path).unwrap(), draft);
    assert_eq!(compile(&draft, &options).unwrap().source, before.source);
}

#[test]
fn wiring_a_draft_into_the_output_reports_the_exact_missing_input_and_recovers() {
    let options = CompileOptions::default();
    let mut graph = demo_graph();
    graph.nodes.push(draft_node());
    graph
        .connections
        .push(Connection::new("zero", "value", "draft", "a"));
    compile(&graph, &options).unwrap();
    let edge = graph
        .connections
        .iter_mut()
        .find(|edge| edge.to == PortRef::new("circle", "radius"))
        .unwrap();
    edge.from = PortRef::new("draft", "value");
    let diagnostic = compile(&graph, &options).err().unwrap().diagnostic();
    assert_eq!(diagnostic.code, ErrorCode::MissingInput);
    assert_eq!(diagnostic.node.as_deref(), Some("draft"));
    assert_eq!(diagnostic.port.as_deref(), Some("b"));
    graph
        .connections
        .push(Connection::new("radius", "value", "draft", "b"));
    compile(&graph, &options).unwrap();
}

#[test]
fn unused_drafts_still_obey_structure_types_units_cycles_values_and_budgets() {
    let options = CompileOptions::default();
    let mut draft = demo_graph();
    draft.nodes.push(draft_node());
    let check = |graph: &GraphDocument, expected| {
        assert_eq!(
            validate(graph, &options).unwrap_err().diagnostic().code,
            expected
        );
    };
    let mut graph = draft.clone();
    graph
        .connections
        .push(Connection::new("color", "value", "draft", "a"));
    check(&graph, ErrorCode::TypeMismatch);
    let mut graph = draft.clone();
    graph
        .connections
        .push(Connection::new("draft", "value", "draft", "a"));
    check(&graph, ErrorCode::Cycle);
    let mut graph = draft.clone();
    graph
        .connections
        .push(Connection::new("missing", "value", "draft", "a"));
    check(&graph, ErrorCode::UnknownNode);
    let mut graph = draft.clone();
    graph
        .connections
        .push(Connection::new("zero", "missing", "draft", "a"));
    check(&graph, ErrorCode::UnknownPort);
    let mut graph = draft.clone();
    graph.connections.extend([
        Connection::new("zero", "value", "draft", "a"),
        Connection::new("radius", "value", "draft", "a"),
    ]);
    check(&graph, ErrorCode::DuplicateInput);
    let mut graph = draft.clone();
    graph
        .nodes
        .push(Node::new("bad", NodeOp::Float { value: f32::NAN }));
    check(&graph, ErrorCode::InvalidValue);
    let mut graph = draft.clone();
    graph.nodes.push(Node::new(
        "sample",
        NodeOp::SampleImage {
            coordinates: SampleCoordinates::Uv,
        },
    ));
    graph
        .connections
        .push(Connection::new("position", "value", "sample", "coordinate"));
    check(&graph, ErrorCode::TypeMismatch);
    let limited = CompileOptions {
        max_nodes: draft.nodes.len() - 1,
        ..options
    };
    assert_eq!(
        validate(&draft, &limited).unwrap_err().diagnostic().code,
        ErrorCode::Budget
    );
}

#[test]
fn nested_drafts_do_not_block_the_output_but_previewing_them_checks_their_inputs() {
    let options = CompileOptions::default();
    let original = fixture::nested_demo(2);
    let mut graph = original.clone();
    let NodeOp::Group { graph: outer } = &mut graph.nodes[0].operation else {
        panic!("group fixture")
    };
    let NodeOp::Group { graph: inner } = &mut outer.nodes[0].operation else {
        panic!("nested group fixture")
    };
    inner.nodes.push(draft_node());
    inner
        .connections
        .push(Connection::new("zero", "value", "draft", "a"));
    assert_eq!(
        compile(&graph, &options).unwrap().source,
        compile(&original, &options).unwrap().source
    );
    let target = PortRef::new("wrap_1/wrap_0/draft", "value");
    let diagnostic = compile_preview(&graph, &options, &target)
        .err()
        .unwrap()
        .diagnostic();
    assert_eq!(diagnostic.code, ErrorCode::MissingInput);
    assert_eq!(diagnostic.node.as_deref(), Some("wrap_1/wrap_0/draft"));
    assert_eq!(diagnostic.port.as_deref(), Some("b"));
}

#[test]
fn unused_group_interfaces_can_be_incomplete_and_keep_boundary_diagnostics_when_used() {
    let options = CompileOptions::default();
    let mut graph = fixture::grouped_math();
    // Move the live output onto a complete constant, leaving the group as a draft.
    for edge in &mut graph.connections {
        if edge.from.node == "mul" {
            edge.from = PortRef::new("left", "value");
        }
    }
    graph
        .connections
        .retain(|edge| edge.to != PortRef::new("mul", "y"));
    compile(&graph, &options).unwrap();
    let diagnostic = compile_preview(&graph, &options, &PortRef::new("mul", "result"))
        .err()
        .unwrap()
        .diagnostic();
    assert_eq!(diagnostic.code, ErrorCode::MissingInput);
    assert_eq!(diagnostic.node.as_deref(), Some("mul"));
    assert_eq!(diagnostic.port.as_deref(), Some("y"));
    graph
        .connections
        .iter_mut()
        .find(|edge| edge.to == PortRef::new("rgba", "r"))
        .unwrap()
        .from = PortRef::new("mul", "result");
    let diagnostic = compile(&graph, &options).err().unwrap().diagnostic();
    assert_eq!(diagnostic.code, ErrorCode::MissingInput);
    assert_eq!(diagnostic.node.as_deref(), Some("mul"));
    assert_eq!(diagnostic.port.as_deref(), Some("y"));
    graph
        .connections
        .push(Connection::new("right", "value", "mul", "y"));
    compile(&graph, &options).unwrap();
}

#[test]
fn unconnected_group_output_ports_are_drafts_until_used_or_previewed() {
    let options = CompileOptions::default();
    let mut graph = fixture::grouped_math();
    let NodeOp::Group { graph: group } = &mut graph.nodes[3].operation else {
        panic!("group fixture")
    };
    let NodeOp::GroupOutput { ports } = &mut group.nodes[2].operation else {
        panic!("output fixture")
    };
    ports.insert(
        "draft".into(),
        GroupPort {
            name: "草稿".into(),
            english_name: "Draft".into(),
            value_type: ValueType::Float,
            semantic: PortSemantic::Scalar,
        },
    );
    compile(&graph, &options).unwrap();
    let diagnostic = compile_preview(&graph, &options, &PortRef::new("mul", "draft"))
        .err()
        .unwrap()
        .diagnostic();
    assert_eq!(diagnostic.code, ErrorCode::MissingInput);
    assert_eq!(diagnostic.node.as_deref(), Some("mul/output"));
    assert_eq!(diagnostic.port.as_deref(), Some("draft"));
    graph
        .connections
        .iter_mut()
        .find(|edge| edge.to == PortRef::new("rgba", "r"))
        .unwrap()
        .from = PortRef::new("mul", "draft");
    let diagnostic = compile(&graph, &options).err().unwrap().diagnostic();
    assert_eq!(diagnostic.code, ErrorCode::MissingInput);
    assert_eq!(diagnostic.node.as_deref(), Some("mul/output"));
    assert_eq!(diagnostic.port.as_deref(), Some("draft"));
}

#[test]
fn unused_groups_still_validate_boundary_structure_connections_and_cycles() {
    let options = CompileOptions::default();
    let mut original = fixture::grouped_math();
    for edge in &mut original.connections {
        if edge.from.node == "mul" {
            edge.from = PortRef::new("left", "value");
        }
    }
    original.connections.retain(|edge| edge.to.node != "mul");
    for (operation, expected) in [
        (0, ErrorCode::Cycle),
        (1, ErrorCode::UnknownPort),
        (2, ErrorCode::InvalidValue),
        (3, ErrorCode::TypeMismatch),
    ] {
        let mut graph = original.clone();
        let NodeOp::Group { graph: group } = &mut graph.nodes[3].operation else {
            panic!("group fixture")
        };
        match operation {
            0 => {
                group.connections[0].from = PortRef::new("internal", "value");
            }
            1 => {
                group.connections[0].from.port = "missing".into();
            }
            2 => {
                group.name.clear();
            }
            3 => {
                let NodeOp::GroupInput { ports } = &mut group.nodes[0].operation else {
                    panic!("input fixture")
                };
                ports.get_mut("x").unwrap().semantic = PortSemantic::Color;
            }
            _ => unreachable!(),
        }
        assert_eq!(
            validate(&graph, &options).unwrap_err().diagnostic().code,
            expected
        );
    }
}
