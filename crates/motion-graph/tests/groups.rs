use motion_graph::*;
#[path = "support/group_fixture.rs"]
mod fixture;

#[test]
fn group_interfaces_lower_and_preview_resolves_outer_dependencies() {
    let document = fixture::grouped_math();
    let original = document.clone();
    let options = CompileOptions::default();
    let compiled = compile(&document, &options).unwrap();
    let mut plain = document.clone();
    plain.nodes[3].operation = NodeOp::Math {
        operation: MathOp::Multiply,
        value_type: ValueType::Float,
    };
    for edge in &mut plain.connections {
        if edge.to.node == "mul" {
            edge.to.port = if edge.to.port == "x" { "a" } else { "b" }.into();
        }
        if edge.from.node == "mul" {
            edge.from.port = "value".into();
        }
    }
    assert_eq!(
        compiled.report.total_sample_sites,
        compile(&plain, &options).unwrap().report.total_sample_sites
    );
    let target = PortRef::new("mul/internal", "value");
    let preview = compile_preview(&document, &options, &target).unwrap();
    assert_eq!(preview.report.reachable_nodes, 3);
    assert_eq!(document, original);
    compile_preview(&document, &options, &PortRef::new("mul", "result")).unwrap();
    compile_preview(&document, &options, &PortRef::new("mul/input", "x")).unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("groups.msfx");
    let report = export_msfx(&document, &options, &path).unwrap();
    assert_eq!(GraphDocument::load(&report.source_path).unwrap(), original);
}

#[test]
fn nested_groups_keep_root_parameters_and_have_bounded_depth_and_real_leaf_budgets() {
    let base = demo_graph();
    let nested = fixture::nested_demo(4);
    let options = CompileOptions::default();
    let original = compile(&base, &options).unwrap();
    let compiled = compile(&nested, &options).unwrap();
    assert_eq!(
        compiled.report.total_sample_sites,
        original.report.total_sample_sites
    );
    assert_eq!(
        compiled.report.parameter_slots,
        original.report.parameter_slots
    );
    let loaded = GraphDocument::from_json(&nested.to_json().unwrap()).unwrap();
    assert_eq!(loaded, nested);
    compile_preview(
        &nested,
        &options,
        &PortRef::new("wrap_3/wrap_2/wrap_1/wrap_0/circle", "distance"),
    )
    .unwrap();
    assert_eq!(
        validate(&fixture::nested_demo(17), &options)
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::Budget
    );
    assert_eq!(
        validate(
            &nested,
            &CompileOptions {
                max_nodes: base.nodes.len() - 1,
                ..options
            }
        )
        .unwrap_err()
        .diagnostic()
        .code,
        ErrorCode::Budget
    );
}

#[test]
fn root_and_inner_cycles_types_missing_boundaries_and_units_are_rejected() {
    let options = CompileOptions::default();
    let mut cycle = fixture::grouped_math();
    cycle
        .connections
        .push(Connection::new("mul", "result", "mul", "x"));
    cycle.connections.retain(|edge| edge.from.node != "left");
    assert_eq!(
        validate(&cycle, &options).unwrap_err().diagnostic().code,
        ErrorCode::Cycle
    );
    let mut inner = fixture::grouped_math();
    if let NodeOp::Group { graph } = &mut inner.nodes[3].operation {
        graph.connections[0].from = PortRef::new("internal", "value");
    }
    assert_eq!(
        validate(&inner, &options).unwrap_err().diagnostic().code,
        ErrorCode::Cycle
    );
    let mut unit = fixture::grouped_math();
    if let NodeOp::Group { graph } = &mut unit.nodes[3].operation {
        if let NodeOp::GroupInput { ports } = &mut graph.nodes[0].operation {
            ports.get_mut("x").unwrap().semantic = PortSemantic::Color;
        }
    }
    assert_eq!(
        validate(&unit, &options).unwrap_err().diagnostic().code,
        ErrorCode::TypeMismatch
    );
    let mut missing = fixture::grouped_math();
    if let NodeOp::Group { graph } = &mut missing.nodes[3].operation {
        graph.nodes.remove(0);
    }
    assert_eq!(
        validate(&missing, &options).unwrap_err().diagnostic().code,
        ErrorCode::UnknownNode
    );
    let mut root_boundary = demo_graph();
    root_boundary.nodes.push(Node::new(
        "boundary",
        NodeOp::GroupInput {
            ports: Default::default(),
        },
    ));
    assert_eq!(
        validate(&root_boundary, &options)
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::InvalidValue
    );
}

#[test]
fn collapse_round_trips_and_does_not_change_shader_or_runtime_costs() {
    let original = fixture::nested_demo(2);
    let mut collapsed = original.clone();
    collapsed.nodes[0].collapsed = true;
    if let NodeOp::Group { graph } = &mut collapsed.nodes[0].operation {
        graph
            .nodes
            .iter_mut()
            .for_each(|node| node.collapsed = true);
    }
    let options = CompileOptions::default();
    assert_eq!(
        compile(&collapsed, &options).unwrap().source,
        compile(&original, &options).unwrap().source
    );
    assert_eq!(
        GraphDocument::from_json(&collapsed.to_json().unwrap()).unwrap(),
        collapsed
    );
}

#[test]
fn group_structure_does_not_consume_the_soft_shader_leaf_budget() {
    let mut graph = demo_graph();
    graph.public_parameters.clear();
    graph.nodes = vec![
        Node::new(
            "color",
            NodeOp::Color {
                value: [0.2, 0.3, 0.4, 1.],
            },
        ),
        Node::new("output", NodeOp::ColorOutput),
    ];
    graph.connections = vec![Connection::new("color", "value", "output", "color")];
    for index in 0..16 {
        let mut nodes = std::mem::take(&mut graph.nodes);
        nodes.push(Node::new(
            "input",
            NodeOp::GroupInput {
                ports: Default::default(),
            },
        ));
        nodes.push(Node::new(
            "result",
            NodeOp::GroupOutput {
                ports: Default::default(),
            },
        ));
        let group = NodeGroup {
            name: "组".into(),
            english_name: "Group".into(),
            nodes,
            connections: std::mem::take(&mut graph.connections),
            input_node: "input".into(),
            output_node: "result".into(),
        };
        graph.nodes.push(Node::new(
            format!("group_{index}"),
            NodeOp::Group {
                graph: Box::new(group),
            },
        ));
    }
    let options = CompileOptions {
        max_nodes: 2,
        max_connections: 1,
        ..Default::default()
    };
    compile(&graph, &options).unwrap();
    assert_eq!(
        GraphDocument::from_json(&graph.to_json().unwrap()).unwrap(),
        graph
    );
}
