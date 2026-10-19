use motion_graph::*;
#[path = "support/group_fixture.rs"]
mod fixture;

fn fallback(graph: &mut GraphDocument, node: &str, port: &str, value: InputValue) {
    graph.format_version = INPUT_DEFAULTS_FORMAT_VERSION;
    graph
        .connections
        .retain(|edge| edge.to != PortRef::new(node, port));
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == node)
        .unwrap()
        .input_defaults
        .insert(port.into(), value);
}
fn compile_ok(graph: &GraphDocument) {
    compile(graph, &CompileOptions::default()).unwrap();
}

#[test]
fn old_documents_stay_v1_and_typed_defaults_round_trip_as_v2() {
    let original = demo_graph();
    let json = original.to_json().unwrap();
    assert!(!json.contains("input_defaults"));
    assert_eq!(GraphDocument::from_json(&json).unwrap(), original);
    let mut graph = original;
    fallback(&mut graph, "circle", "radius", InputValue::Float(18.));
    let json = graph.to_json().unwrap();
    assert!(json.contains("input_defaults"));
    assert_eq!(GraphDocument::from_json(&json).unwrap(), graph);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.msgraph");
    graph.save(&path).unwrap();
    assert_eq!(GraphDocument::load(&path).unwrap(), graph);
    graph.format_version = 1;
    assert_eq!(
        graph.to_json().unwrap_err().diagnostic().code,
        ErrorCode::DocumentVersion
    );
    assert_eq!(
        validate(&graph, &CompileOptions::default())
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::DocumentVersion
    );
    let mut future = serde_json::to_value(&graph).unwrap();
    future["format_version"] = 3.into();
    future["future_payload"] = true.into();
    assert_eq!(
        GraphDocument::from_json(&future.to_string())
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::DocumentVersion
    );
}

#[test]
fn all_storage_types_compile_without_implicit_casts() {
    for (node, port, value) in [
        ("circle", "radius", InputValue::Float(18.)),
        ("circle", "center", InputValue::Vec2([2., 3.])),
        ("fill", "distance", InputValue::DistancePixels(-2.)),
        ("output", "color", InputValue::Color([0.8, 0.3, 0.2, 0.5])),
    ] {
        let mut graph = demo_graph();
        // The public demo's output node ID is discovered rather than assumed.
        let output = graph
            .nodes
            .iter()
            .find(|n| matches!(n.operation, NodeOp::ColorOutput))
            .unwrap()
            .id
            .clone();
        fallback(
            &mut graph,
            if node == "output" { &output } else { node },
            port,
            value,
        );
        compile_ok(&graph);
    }
    let mut graph = demo_graph();
    let mut length = Node::new(
        "length",
        NodeOp::Math {
            operation: MathOp::Length,
            value_type: ValueType::Vec3,
        },
    );
    length
        .input_defaults
        .insert("value".into(), InputValue::Vec3([3., 4., 12.]));
    graph.nodes.push(length);
    graph
        .connections
        .retain(|edge| edge.to != PortRef::new("circle", "radius"));
    graph
        .connections
        .push(Connection::new("length", "value", "circle", "radius"));
    graph.format_version = 2;
    compile_ok(&graph);
    let mut select = Node::new(
        "select",
        NodeOp::Select {
            value_type: ValueType::Float,
        },
    );
    select.input_defaults = authoring_input_defaults(&select.operation);
    select
        .input_defaults
        .insert("condition".into(), InputValue::Bool(true));
    graph.nodes.push(select);
    graph
        .connections
        .retain(|edge| edge.to != PortRef::new("circle", "radius"));
    graph
        .connections
        .push(Connection::new("select", "value", "circle", "radius"));
    compile_ok(&graph);
}

#[test]
fn connected_inputs_win_and_disconnect_restores_the_stored_literal() {
    let mut graph = demo_graph();
    graph.format_version = 2;
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "circle")
        .unwrap()
        .input_defaults
        .insert("radius".into(), InputValue::Float(7.));
    let source = compile(&graph, &CompileOptions::default()).unwrap().source;
    let mut plain = graph.clone();
    plain
        .nodes
        .iter_mut()
        .find(|n| n.id == "circle")
        .unwrap()
        .input_defaults
        .clear();
    assert_eq!(
        source,
        compile(&plain, &CompileOptions::default()).unwrap().source
    );
    graph
        .connections
        .retain(|edge| edge.to != PortRef::new("circle", "radius"));
    compile_ok(&graph);
    assert_ne!(
        source,
        compile(&graph, &CompileOptions::default()).unwrap().source
    );
    assert_eq!(
        graph
            .nodes
            .iter()
            .find(|n| n.id == "circle")
            .unwrap()
            .input_defaults["radius"],
        InputValue::Float(7.)
    );
}

#[test]
fn invalid_defaults_are_rejected_even_when_unused_or_connected() {
    for (port, value, code) in [
        ("unknown", InputValue::Float(0.), ErrorCode::UnknownPort),
        (
            "radius",
            InputValue::Vec2([0., 0.]),
            ErrorCode::TypeMismatch,
        ),
        (
            "radius",
            InputValue::Float(f32::NAN),
            ErrorCode::InvalidValue,
        ),
        (
            "radius",
            InputValue::Float(1_000_001.),
            ErrorCode::InvalidValue,
        ),
    ] {
        let mut graph = demo_graph();
        graph.format_version = 2;
        graph
            .nodes
            .iter_mut()
            .find(|n| n.id == "circle")
            .unwrap()
            .input_defaults
            .insert(port.into(), value);
        let diagnostic = validate(&graph, &CompileOptions::default())
            .unwrap_err()
            .diagnostic();
        assert_eq!(diagnostic.code, code);
        assert_eq!(diagnostic.node.as_deref(), Some("circle"));
        assert_eq!(diagnostic.port.as_deref(), Some(port));
    }
    let mut graph = demo_graph();
    let mut unused = Node::new(
        "unused",
        NodeOp::Select {
            value_type: ValueType::Color,
        },
    );
    unused
        .input_defaults
        .insert("true_value".into(), InputValue::Color([2., 0., 0., 1.]));
    graph.nodes.push(unused);
    graph.format_version = 2;
    assert_eq!(
        validate(&graph, &CompileOptions::default())
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::InvalidValue
    );
}

#[test]
fn unresolved_group_wires_cannot_be_masked_by_a_leaf_default() {
    let mut graph = fixture::grouped_math();
    graph.format_version = 2;
    let NodeOp::Group { graph: inner } = &mut graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "mul")
        .unwrap()
        .operation
    else {
        panic!()
    };
    inner
        .nodes
        .iter_mut()
        .find(|n| n.id == "internal")
        .unwrap()
        .input_defaults = authoring_input_defaults(&NodeOp::Math {
        operation: MathOp::Multiply,
        value_type: ValueType::Float,
    });
    compile_ok(&graph);
    graph
        .connections
        .retain(|edge| edge.to != PortRef::new("mul", "x"));
    let before = graph.clone();
    let diagnostic = compile(&graph, &CompileOptions::default())
        .err()
        .unwrap()
        .diagnostic();
    assert_eq!(diagnostic.code, ErrorCode::MissingInput);
    assert_eq!(diagnostic.node.as_deref(), Some("mul"));
    assert_eq!(diagnostic.port.as_deref(), Some("x"));
    assert_eq!(graph, before);
}

#[test]
fn nested_leaf_defaults_round_trip_and_group_boundary_defaults_are_explicitly_rejected() {
    let mut graph = fixture::nested_demo(2);
    let NodeOp::Group { graph: outer } = &mut graph.nodes[0].operation else {
        panic!()
    };
    let NodeOp::Group { graph: inner } = &mut outer.nodes[0].operation else {
        panic!()
    };
    inner
        .connections
        .retain(|edge| edge.to != PortRef::new("circle", "radius"));
    inner
        .nodes
        .iter_mut()
        .find(|n| n.id == "circle")
        .unwrap()
        .input_defaults
        .insert("radius".into(), InputValue::Float(18.));
    assert_eq!(graph.minimum_format_version(), 2);
    graph.format_version = 2;
    compile_ok(&graph);
    assert_eq!(
        GraphDocument::from_json(&graph.to_json().unwrap()).unwrap(),
        graph
    );
    graph.nodes[0]
        .input_defaults
        .insert("value".into(), InputValue::Float(1.));
    assert_eq!(
        validate(&graph, &CompileOptions::default())
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::InvalidValue
    );
}

#[test]
fn authoring_defaults_match_their_ports_and_leave_context_inputs_required() {
    let mut operations = vec![
        NodeOp::Splat {
            value_type: ValueType::Vec2,
        },
        NodeOp::CombineVec2,
        NodeOp::CombineVec3,
        NodeOp::CombineColor,
        NodeOp::Select {
            value_type: ValueType::Bool,
        },
        NodeOp::Select {
            value_type: ValueType::Color,
        },
        NodeOp::ColorMath {
            operation: ColorOp::Mix,
        },
        NodeOp::SdfCircle,
        NodeOp::SdfRoundedRectangle,
        NodeOp::SdfLine,
        NodeOp::SdfCapsule,
        NodeOp::SdfSmoothUnion,
        NodeOp::SdfStroke,
    ];
    for operation in [
        MathOp::Add,
        MathOp::Subtract,
        MathOp::Multiply,
        MathOp::Divide,
        MathOp::Min,
        MathOp::Max,
        MathOp::Abs,
        MathOp::Sin,
        MathOp::Cos,
        MathOp::Sqrt,
        MathOp::Floor,
        MathOp::Fract,
        MathOp::Clamp,
        MathOp::Lerp,
        MathOp::Smoothstep,
        MathOp::Dot,
        MathOp::Length,
        MathOp::Normalize,
    ] {
        for value_type in [ValueType::Float, ValueType::Vec2, ValueType::Vec3] {
            if value_type == ValueType::Float
                && matches!(operation, MathOp::Dot | MathOp::Length | MathOp::Normalize)
            {
                continue;
            }
            operations.push(NodeOp::Math {
                operation,
                value_type,
            });
        }
    }
    for operation in operations {
        let mut node = Node::new("draft", operation);
        node.input_defaults = authoring_input_defaults(&node.operation);
        assert!(!node.input_defaults.is_empty());
        node_ports(&node, &[]).unwrap();
        for port in ["position", "distance", "image", "uv"] {
            assert!(!node.input_defaults.contains_key(port));
        }
    }
    assert!(authoring_input_defaults(&NodeOp::ColorMath {
        operation: ColorOp::Invert
    })
    .is_empty());
}
