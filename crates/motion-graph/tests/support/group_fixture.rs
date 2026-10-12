use motion_graph::*;
use std::collections::BTreeMap;

pub fn nested_demo(depth: usize) -> GraphDocument {
    let mut document = demo_graph();
    for level in 0..depth {
        let mut nodes = std::mem::take(&mut document.nodes);
        nodes.push(Node::new(
            "input",
            NodeOp::GroupInput {
                ports: BTreeMap::new(),
            },
        ));
        nodes.push(Node::new(
            "output_boundary",
            NodeOp::GroupOutput {
                ports: BTreeMap::new(),
            },
        ));
        let group = NodeGroup {
            name: "节点组".into(),
            english_name: "Node Group".into(),
            nodes,
            connections: std::mem::take(&mut document.connections),
            input_node: "input".into(),
            output_node: "output_boundary".into(),
        };
        document.nodes.push(Node::new(
            format!("wrap_{level}"),
            NodeOp::Group {
                graph: Box::new(group),
            },
        ));
    }
    document
}

#[allow(dead_code)]
pub fn grouped_math() -> GraphDocument {
    let mut document = demo_graph();
    document.public_parameters.clear();
    let port = |name: &str| GroupPort {
        name: name.into(),
        english_name: name.into(),
        value_type: ValueType::Float,
        semantic: PortSemantic::Scalar,
    };
    let group = NodeGroup {
        name: "乘法组".into(),
        english_name: "Multiply Group".into(),
        input_node: "input".into(),
        output_node: "output".into(),
        nodes: vec![
            Node::new(
                "input",
                NodeOp::GroupInput {
                    ports: [("x".into(), port("X")), ("y".into(), port("Y"))].into(),
                },
            ),
            Node::new(
                "internal",
                NodeOp::Math {
                    operation: MathOp::Multiply,
                    value_type: ValueType::Float,
                },
            ),
            Node::new(
                "output",
                NodeOp::GroupOutput {
                    ports: [("result".into(), port("Result"))].into(),
                },
            ),
        ],
        connections: vec![
            Connection::new("input", "x", "internal", "a"),
            Connection::new("input", "y", "internal", "b"),
            Connection::new("internal", "value", "output", "result"),
        ],
    };
    document.nodes = vec![
        Node::new("left", NodeOp::Float { value: 0.25 }),
        Node::new("right", NodeOp::Float { value: 0.5 }),
        Node::new("alpha", NodeOp::Float { value: 1. }),
        Node::new(
            "mul",
            NodeOp::Group {
                graph: Box::new(group),
            },
        ),
        Node::new("rgba", NodeOp::CombineColor),
        Node::new("final", NodeOp::ColorOutput),
    ];
    document.connections = vec![
        Connection::new("left", "value", "mul", "x"),
        Connection::new("right", "value", "mul", "y"),
        Connection::new("mul", "result", "rgba", "r"),
        Connection::new("mul", "result", "rgba", "g"),
        Connection::new("mul", "result", "rgba", "b"),
        Connection::new("alpha", "value", "rgba", "a"),
        Connection::new("rgba", "color", "final", "color"),
    ];
    document
}
