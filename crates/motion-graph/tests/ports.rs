use motion_graph::*;

#[test]
fn known_coordinates_have_units_but_numeric_vectors_remain_generic() {
    for (operation, expected) in [
        (NodeOp::Position, PortSemantic::PositionPixels),
        (NodeOp::Uv, PortSemantic::Uv),
        (NodeOp::Resolution, PortSemantic::SizePixels),
        (NodeOp::Time, PortSemantic::Seconds),
        (NodeOp::FrameIndex, PortSemantic::Frame),
        (NodeOp::Vec2 { value: [32., 32.] }, PortSemantic::Vector),
        (
            NodeOp::Math {
                operation: MathOp::Add,
                value_type: ValueType::Vec2,
            },
            PortSemantic::Vector,
        ),
    ] {
        let node = Node::new("source", operation);
        let original = node_ports(&node, &[]).unwrap();
        let specs = node_port_specs(&node, &[]).unwrap();
        assert_eq!(specs.outputs["value"].semantic, expected);
        assert_eq!(specs.outputs["value"].value_type, original.outputs["value"]);
        let description = node_description(&node.operation);
        assert!(!description.0.is_empty() && !description.1.is_empty());
    }
    let circle = node_port_specs(&Node::new("circle", NodeOp::SdfCircle), &[]).unwrap();
    assert_eq!(
        circle.inputs["position"].semantic,
        PortSemantic::PositionPixels
    );
    assert_eq!(
        circle.inputs["center"].semantic,
        PortSemantic::PositionPixels
    );
    assert_eq!(circle.inputs["radius"].semantic, PortSemantic::SizePixels);
    assert_eq!(
        circle.outputs["distance"].semantic,
        PortSemantic::DistancePixels
    );
    let rectangle = node_port_specs(&Node::new("rect", NodeOp::SdfRoundedRectangle), &[]).unwrap();
    assert_eq!(
        rectangle.inputs["half_size"].semantic,
        PortSemantic::SizePixels
    );
    let stroke = node_port_specs(&Node::new("stroke", NodeOp::SdfStroke), &[]).unwrap();
    assert_eq!(stroke.inputs["width"].semantic, PortSemantic::SizePixels);
    for example in builtin_examples() {
        let graph = example.load().unwrap();
        for node in &graph.nodes {
            let specs = node_port_specs(node, &graph.public_parameters).unwrap();
            for spec in specs.inputs.values().chain(specs.outputs.values()) {
                assert!(
                    !spec.id.is_empty() && !spec.name.is_empty() && !spec.english_name.is_empty()
                );
                assert!(!spec.description.is_empty() && !spec.english_description.is_empty());
                assert!(!spec.semantic.label().0.is_empty() && !spec.semantic.label().1.is_empty());
            }
        }
    }
}

#[test]
fn authoring_validation_and_compilation_share_known_coordinate_rejection() {
    let mut graph = demo_graph();
    graph.nodes.push(Node::new("uv", NodeOp::Uv));
    let edge = graph
        .connections
        .iter_mut()
        .find(|c| c.to == PortRef::new("circle", "position"))
        .unwrap();
    edge.from = PortRef::new("uv", "value");
    let error = check_connection(
        &graph,
        &PortRef::new("uv", "value"),
        &PortRef::new("circle", "position"),
    )
    .unwrap_err()
    .diagnostic();
    assert_eq!(error.code, ErrorCode::TypeMismatch);
    assert_eq!(error.node.as_deref(), Some("circle"));
    assert_eq!(error.port.as_deref(), Some("position"));
    assert_eq!(
        validate(&graph, &CompileOptions::default())
            .unwrap_err()
            .diagnostic(),
        error
    );
    assert_eq!(
        compile(&graph, &CompileOptions::default())
            .err()
            .unwrap()
            .diagnostic(),
        error
    );
    // Existing Vec2 literals still express coordinates chosen by their target input.
    graph.nodes.last_mut().unwrap().operation = NodeOp::Vec2 { value: [32., 32.] };
    check_connection(
        &graph,
        &PortRef::new("uv", "value"),
        &PortRef::new("circle", "position"),
    )
    .unwrap();
    validate(&graph, &CompileOptions::default()).unwrap();
    // A known UV can be explicitly converted with UV * Resolution. Math output
    // is conservatively generic, not falsely proven to be pixel coordinates.
    graph.nodes.last_mut().unwrap().operation = NodeOp::Uv;
    graph
        .nodes
        .push(Node::new("resolution", NodeOp::Resolution));
    graph.nodes.push(Node::new(
        "convert",
        NodeOp::Math {
            operation: MathOp::Multiply,
            value_type: ValueType::Vec2,
        },
    ));
    graph
        .connections
        .iter_mut()
        .find(|c| c.to == PortRef::new("circle", "position"))
        .unwrap()
        .from = PortRef::new("convert", "value");
    graph.connections.extend([
        Connection::new("uv", "value", "convert", "a"),
        Connection::new("resolution", "value", "convert", "b"),
    ]);
    compile(&graph, &CompileOptions::default()).unwrap();
    assert_eq!(
        node_port_specs(graph.nodes.last().unwrap(), &graph.public_parameters)
            .unwrap()
            .outputs["value"]
            .semantic,
        PortSemantic::Vector
    );
}

#[test]
fn sample_modes_and_direct_clock_to_pixel_length_check_shared_units() {
    let mut graph = builtin_examples()
        .iter()
        .find(|e| e.id == "image_invert")
        .unwrap()
        .load()
        .unwrap();
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "uv")
        .unwrap()
        .operation = NodeOp::Position;
    let from = PortRef::new("uv", "value");
    let to = PortRef::new("sample", "coordinate");
    assert_eq!(
        check_connection(&graph, &from, &to)
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::TypeMismatch
    );
    assert!(compile(&graph, &CompileOptions::default()).is_err());
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "sample")
        .unwrap()
        .operation = NodeOp::SampleImage {
        coordinates: SampleCoordinates::Pixels,
    };
    check_connection(&graph, &from, &to).unwrap();
    compile(&graph, &CompileOptions::default()).unwrap();
    let mut graph = demo_graph();
    graph
        .connections
        .iter_mut()
        .find(|c| c.to == PortRef::new("circle", "radius"))
        .unwrap()
        .from = PortRef::new("time", "value");
    assert_eq!(
        check_connection(
            &graph,
            &PortRef::new("time", "value"),
            &PortRef::new("circle", "radius")
        )
        .unwrap_err()
        .diagnostic()
        .code,
        ErrorCode::TypeMismatch
    );
    assert!(compile(&graph, &CompileOptions::default()).is_err());
}
