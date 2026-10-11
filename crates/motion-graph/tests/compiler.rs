use motion_effects::{AlphaMode, WorkingSpace};
use motion_graph::*;

fn graph_for(operation: NodeOp) -> GraphDocument {
    let mut graph = GraphDocument {
        format_version: 1,
        id: "coverage".into(),
        name: "节点验收".into(),
        english_name: "Node Acceptance".into(),
        working_space: WorkingSpace::Srgb,
        alpha_mode: AlphaMode::Straight,
        nodes: vec![],
        connections: vec![],
        public_parameters: vec![],
    };
    for (id, ty, default, min, max) in [
        (
            "scalar",
            ValueType::Float,
            [16.0, 0.0, 0.0, 0.0],
            -100.0,
            100.0,
        ),
        (
            "vector2",
            ValueType::Vec2,
            [32.0, 32.0, 0.0, 0.0],
            -100.0,
            100.0,
        ),
        (
            "vector3",
            ValueType::Vec3,
            [0.2, 0.4, 0.6, 0.0],
            -100.0,
            100.0,
        ),
        ("color", ValueType::Color, [0.2, 0.4, 0.6, 0.5], 0.0, 1.0),
        ("boolean", ValueType::Bool, [1.0, 0.0, 0.0, 0.0], 0.0, 1.0),
    ] {
        graph.public_parameters.push(PublicParameter {
            id: id.into(),
            name: id.into(),
            english_name: id.into(),
            value_type: ty,
            default,
            min,
            max,
            step: 0.1,
            units: String::new(),
            animatable: true,
        });
        graph.nodes.push(Node::new(
            id,
            NodeOp::Parameter {
                parameter: id.into(),
            },
        ));
    }
    graph.nodes.extend([
        Node::new("pixel", NodeOp::Position),
        Node::new(
            "image",
            NodeOp::Image {
                source: ImageSource::Input,
            },
        ),
        Node::new("base_circle", NodeOp::SdfCircle),
        Node::new("one", NodeOp::Float { value: 1.0 }),
        Node::new("zero", NodeOp::Float { value: 0.0 }),
    ]);
    graph.connections.extend([
        Connection::new("pixel", "value", "base_circle", "position"),
        Connection::new("vector2", "value", "base_circle", "center"),
        Connection::new("scalar", "value", "base_circle", "radius"),
    ]);
    let node = Node::new("tested", operation);
    let ports = node_ports(&node, &graph.public_parameters).unwrap();
    for (input, ty) in &ports.inputs {
        let (source, port) = match ty {
            ValueType::Float => ("scalar", "value"),
            ValueType::Vec2 => ("vector2", "value"),
            ValueType::Vec3 => ("vector3", "value"),
            ValueType::Color => ("color", "value"),
            ValueType::Bool => ("boolean", "value"),
            ValueType::Image => ("image", "image"),
            ValueType::DistancePixels => ("base_circle", "distance"),
        };
        graph
            .connections
            .push(Connection::new(source, port, "tested", input));
    }
    graph.nodes.push(node);
    if ports.outputs.is_empty() {
        return graph;
    }
    let (port, ty) = ports.outputs.iter().next().unwrap();
    let mut value = PortRef::new("tested", port);
    match ty {
        ValueType::Image => {
            graph.nodes.push(Node::new(
                "result",
                NodeOp::SampleImage {
                    coordinates: SampleCoordinates::Pixels,
                },
            ));
            graph.connections.extend([
                Connection {
                    from: value,
                    to: PortRef::new("result", "image"),
                },
                Connection::new("pixel", "value", "result", "coordinate"),
            ]);
            value = PortRef::new("result", "color");
        }
        ValueType::DistancePixels => {
            graph.nodes.push(Node::new("coverage", NodeOp::SdfFill));
            graph.connections.push(Connection {
                from: value,
                to: PortRef::new("coverage", "distance"),
            });
            value = PortRef::new("coverage", "coverage");
        }
        ValueType::Vec2 | ValueType::Vec3 => {
            graph.nodes.push(Node::new(
                "length",
                NodeOp::Math {
                    operation: MathOp::Length,
                    value_type: *ty,
                },
            ));
            graph.connections.push(Connection {
                from: value,
                to: PortRef::new("length", "value"),
            });
            value = PortRef::new("length", "value");
        }
        ValueType::Bool => {
            graph.nodes.push(Node::new(
                "select",
                NodeOp::Select {
                    value_type: ValueType::Color,
                },
            ));
            graph.connections.extend([
                Connection {
                    from: value,
                    to: PortRef::new("select", "condition"),
                },
                Connection::new("color", "value", "select", "true_value"),
                Connection::new("color", "value", "select", "false_value"),
            ]);
            value = PortRef::new("select", "value");
        }
        _ => {}
    }
    if !matches!(ty, ValueType::Image | ValueType::Color | ValueType::Bool) {
        graph.nodes.push(Node::new("rgba", NodeOp::CombineColor));
        graph.connections.extend([
            Connection {
                from: value,
                to: PortRef::new("rgba", "r"),
            },
            Connection::new("zero", "value", "rgba", "g"),
            Connection::new("zero", "value", "rgba", "b"),
            Connection::new("one", "value", "rgba", "a"),
        ]);
        value = PortRef::new("rgba", "color");
    }
    graph.nodes.push(Node::new("output", NodeOp::ColorOutput));
    graph.connections.push(Connection {
        from: value,
        to: PortRef::new("output", "color"),
    });
    graph
}

#[test]
fn every_n1_node_and_dynamic_math_variant_compiles_to_validated_wgsl_and_gles300() {
    let mut operations = vec![
        NodeOp::Time,
        NodeOp::FrameIndex,
        NodeOp::Resolution,
        NodeOp::Position,
        NodeOp::Uv,
        NodeOp::Image {
            source: ImageSource::Input,
        },
        NodeOp::Image {
            source: ImageSource::Source,
        },
        NodeOp::SampleImage {
            coordinates: SampleCoordinates::Pixels,
        },
        NodeOp::SampleImage {
            coordinates: SampleCoordinates::Uv,
        },
        NodeOp::Float { value: 0.5 },
        NodeOp::Vec2 { value: [0.5, 0.5] },
        NodeOp::Vec3 {
            value: [0.5, 0.5, 0.5],
        },
        NodeOp::Color {
            value: [0.1, 0.2, 0.3, 0.4],
        },
        NodeOp::Bool { value: true },
        NodeOp::Parameter {
            parameter: "scalar".into(),
        },
        NodeOp::Splat {
            value_type: ValueType::Vec2,
        },
        NodeOp::Splat {
            value_type: ValueType::Vec3,
        },
        NodeOp::CombineVec2,
        NodeOp::CombineVec3,
        NodeOp::CombineColor,
        NodeOp::SplitColor,
        NodeOp::ApplyMask,
        NodeOp::Select {
            value_type: ValueType::Float,
        },
        NodeOp::Select {
            value_type: ValueType::Bool,
        },
        NodeOp::SdfCircle,
        NodeOp::SdfRoundedRectangle,
        NodeOp::SdfLine,
        NodeOp::SdfCapsule,
        NodeOp::SdfUnion,
        NodeOp::SdfIntersection,
        NodeOp::SdfDifference,
        NodeOp::SdfSmoothUnion,
        NodeOp::SdfFill,
        NodeOp::SdfStroke,
    ];
    for op in [
        ColorOp::Invert,
        ColorOp::Mix,
        ColorOp::Multiply,
        ColorOp::Screen,
        ColorOp::SrgbEncode,
        ColorOp::SrgbDecode,
    ] {
        operations.push(NodeOp::ColorMath { operation: op });
    }
    for ty in [ValueType::Float, ValueType::Vec2, ValueType::Vec3] {
        for op in [
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
            if ty != ValueType::Float
                || !matches!(op, MathOp::Dot | MathOp::Length | MathOp::Normalize)
            {
                operations.push(NodeOp::Math {
                    operation: op,
                    value_type: ty,
                });
            }
        }
    }
    for operation in operations {
        let description = format!("{operation:?}");
        let graph = graph_for(operation);
        let compiled = compile(&graph, &CompileOptions::default())
            .unwrap_or_else(|e| panic!("{description}: {e}"));
        assert!(compiled.shader.glsl.fragment.contains("#version 300 es"));
        assert_eq!(compiled.report.loop_work, compiled.shader.loop_work);
        assert_eq!(
            compiled.report.total_sample_sites,
            compiled.report.graph_sample_sites + 1
        );
    }
    let mut premult = graph_for(NodeOp::ColorMath {
        operation: ColorOp::Invert,
    });
    premult.alpha_mode = AlphaMode::Premultiplied;
    compile(&premult, &CompileOptions::default()).unwrap();
}

#[test]
fn common_sampling_is_emitted_once_and_loop_work_does_not_stand_in_for_samples() {
    let mut graph = graph_for(NodeOp::SampleImage {
        coordinates: SampleCoordinates::Pixels,
    });
    graph.nodes.push(Node::new(
        "same_sample",
        NodeOp::SampleImage {
            coordinates: SampleCoordinates::Pixels,
        },
    ));
    graph.connections.extend([
        Connection::new("image", "image", "same_sample", "image"),
        Connection::new("vector2", "value", "same_sample", "coordinate"),
    ]);
    graph.nodes.push(Node::new(
        "mix",
        NodeOp::ColorMath {
            operation: ColorOp::Mix,
        },
    ));
    graph.connections.retain(|e| e.to.node != "output");
    graph.connections.extend([
        Connection::new("tested", "color", "mix", "a"),
        Connection::new("same_sample", "color", "mix", "b"),
        Connection::new("one", "value", "mix", "factor"),
        Connection::new("mix", "color", "output", "color"),
    ]);
    let compiled = compile(&graph, &CompileOptions::default()).unwrap();
    assert_eq!(compiled.report.graph_sample_sites, 1);
    assert_eq!(compiled.report.total_sample_sites, 2);
    assert_eq!(compiled.report.loop_work, 1);
    assert!(compiled.report.common_values_reused > 0);
    let mut options = CompileOptions::default();
    options.max_samples_per_fragment = 1;
    assert_eq!(
        compile(&graph, &options).err().unwrap().diagnostic().code,
        ErrorCode::Budget
    );
}

#[test]
fn constant_folding_retains_declared_safe_math_semantics() {
    let mut graph = graph_for(NodeOp::Math {
        operation: MathOp::Divide,
        value_type: ValueType::Float,
    });
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "scalar")
        .unwrap()
        .operation = NodeOp::Float { value: 0.0 };
    let result = compile(&graph, &CompileOptions::default()).unwrap();
    assert!(result.report.folded_math_nodes > 0);
    assert!(!result
        .source
        .lines()
        .any(|line| line.contains("let ng") && line.contains("ng_div1(")));
}
