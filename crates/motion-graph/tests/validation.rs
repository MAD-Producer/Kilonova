use motion_graph::*;

fn code(graph: &GraphDocument) -> ErrorCode {
    validate(graph, &CompileOptions::default())
        .err()
        .unwrap()
        .diagnostic()
        .code
}
#[test]
fn invalid_dags_have_stable_node_and_port_diagnostics() {
    let original = demo_graph();
    let mut graph = original.clone();
    graph.nodes.push(graph.nodes[0].clone());
    assert_eq!(code(&graph), ErrorCode::DuplicateNode);
    let mut graph = original.clone();
    graph.connections[0].from.node = "missing".into();
    assert_eq!(code(&graph), ErrorCode::UnknownNode);
    let mut graph = original.clone();
    graph.connections[0].from.port = "wrong".into();
    let error = validate(&graph, &CompileOptions::default())
        .err()
        .unwrap()
        .diagnostic();
    assert_eq!(error.code, ErrorCode::UnknownPort);
    assert_eq!(error.port.as_deref(), Some("wrong"));
    let mut graph = original.clone();
    graph
        .connections
        .iter_mut()
        .find(|e| e.to.node == "circle" && e.to.port == "center")
        .unwrap()
        .from = PortRef::new("radius", "value");
    assert_eq!(code(&graph), ErrorCode::TypeMismatch);
    let mut graph = original.clone();
    graph
        .connections
        .push(Connection::new("speed", "value", "circle", "radius"));
    assert_eq!(code(&graph), ErrorCode::DuplicateInput);
    let mut graph = original.clone();
    graph
        .connections
        .retain(|e| !(e.to.node == "circle" && e.to.port == "radius"));
    let error = validate(&graph, &CompileOptions::default())
        .err()
        .unwrap()
        .diagnostic();
    assert_eq!(error.code, ErrorCode::MissingInput);
    assert_eq!(error.node.as_deref(), Some("circle"));
    assert_eq!(error.port.as_deref(), Some("radius"));
    let mut graph = original.clone();
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "time")
        .unwrap()
        .operation = NodeOp::Math {
        operation: MathOp::Add,
        value_type: ValueType::Float,
    };
    graph.connections.extend([
        Connection::new("move", "value", "time", "a"),
        Connection::new("zero", "value", "time", "b"),
    ]);
    assert_eq!(code(&graph), ErrorCode::Cycle);
    let mut graph = original.clone();
    graph.nodes.retain(|n| n.id != "output");
    graph.connections.retain(|e| e.to.node != "output");
    assert_eq!(code(&graph), ErrorCode::MissingOutput);
    let mut graph = original.clone();
    graph
        .nodes
        .push(Node::new("other_output", NodeOp::ColorOutput));
    assert_eq!(code(&graph), ErrorCode::MultipleOutputs);
    let mut graph = original.clone();
    graph.nodes[0].version = 2;
    assert_eq!(code(&graph), ErrorCode::NodeVersion);
    let mut graph = original.clone();
    graph.format_version = 42;
    assert_eq!(code(&graph), ErrorCode::DocumentVersion);
    let mut graph = original.clone();
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "zero")
        .unwrap()
        .operation = NodeOp::Float { value: f32::NAN };
    assert_eq!(code(&graph), ErrorCode::InvalidValue);
    let mut graph = original.clone();
    graph.public_parameters[0].default[0] = 65.0;
    assert_eq!(code(&graph), ErrorCode::InvalidValue);
    let mut graph = original.clone();
    graph
        .public_parameters
        .push(graph.public_parameters[0].clone());
    assert_eq!(code(&graph), ErrorCode::DuplicateParameter);
    let mut options = CompileOptions::default();
    options.max_nodes = original.nodes.len() - 1;
    assert_eq!(
        validate(&original, &options)
            .err()
            .unwrap()
            .diagnostic()
            .code,
        ErrorCode::Budget
    );
    let mut graph = original;
    graph.public_parameters[0].units = "x".repeat(512 * 1024);
    assert_eq!(
        compile(&graph, &CompileOptions::default())
            .err()
            .unwrap()
            .diagnostic()
            .code,
        ErrorCode::Budget
    );
}

#[test]
fn unsupported_and_unknown_nodes_are_rejected_even_if_unconnected() {
    let mut graph = demo_graph();
    graph.nodes.push(Node::new(
        "future",
        NodeOp::Unsupported {
            feature: "world_position".into(),
        },
    ));
    let diagnostic = validate(&graph, &CompileOptions::default())
        .err()
        .unwrap()
        .diagnostic();
    assert_eq!(diagnostic.code, ErrorCode::UnsupportedNode);
    assert_eq!(diagnostic.node.as_deref(), Some("future"));
    assert!(diagnostic.message.contains("world-space"));
    for (kind, expected) in [
        ("world_position", ErrorCode::UnsupportedNode),
        ("depth", ErrorCode::UnsupportedNode),
        ("pbr_output", ErrorCode::UnsupportedNode),
        ("unknown_node", ErrorCode::UnknownNode),
    ] {
        let mut json = serde_json::to_value(demo_graph()).unwrap();
        json["nodes"][0]["operation"] = serde_json::json!({"kind":kind});
        let error = GraphDocument::from_json(&json.to_string())
            .err()
            .unwrap()
            .diagnostic();
        assert_eq!(error.code, expected);
        assert_eq!(error.node.as_deref(), Some("position"));
    }
}

#[test]
fn source_roundtrip_and_layout_changes_preserve_shader_and_parameter_abi() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.msgraph");
    let mut graph = demo_graph();
    graph.save(&path).unwrap();
    assert_eq!(GraphDocument::load(&path).unwrap(), graph);
    let before = compile(&graph, &CompileOptions::default()).unwrap();
    graph.nodes.reverse();
    graph.connections.reverse();
    for node in &mut graph.nodes {
        node.position = [100.0, -240.0];
    }
    let after = compile(&graph, &CompileOptions::default()).unwrap();
    assert_eq!(before.source, after.source);
    assert_eq!(before.manifest, after.manifest);
    assert_eq!(before.report.parameter_slots, after.report.parameter_slots);
    assert_eq!(after.report.passes, 1);
    assert_eq!(after.report.loop_work, after.shader.loop_work);
    assert_eq!(after.report.graph_sample_sites, 0);
    assert_eq!(after.report.host_sample_sites, 1);
    assert_eq!(after.report.package_resource_textures, 0);
    let mut options = CompileOptions::default();
    options.max_samples_per_fragment = 0;
    assert_eq!(
        compile(&graph, &options).err().unwrap().diagnostic().code,
        ErrorCode::Budget
    );
}
