//! Reusable scalar programs composed from existing operations, not new nodes.
use crate::{Connection, GroupPort, MathOp, Node, NodeGroup, NodeOp, PortSemantic, ValueType};

/// `mix(reference, value, factor * mask)` on four explicit linear scalar ports.
/// Resource calibration, registration, validity and sample costs belong to the
/// caller. This group performs no colour conversion and never uses RGBA alpha.
pub fn masked_lerp() -> NodeGroup {
    let port = |name: &str, english_name: &str| GroupPort {
        name: name.into(),
        english_name: english_name.into(),
        value_type: ValueType::Float,
        semantic: PortSemantic::Scalar,
    };
    let mut input = Node::new(
        "input",
        NodeOp::GroupInput {
            ports: [
                ("reference".into(), port("参考", "Reference")),
                ("value".into(), port("数值", "Value")),
                ("factor".into(), port("权重", "Factor")),
                ("mask".into(), port("蒙版", "Mask")),
            ]
            .into(),
        },
    );
    let mut weight = Node::new(
        "weight",
        NodeOp::Math {
            operation: MathOp::Multiply,
            value_type: ValueType::Float,
        },
    );
    let mut mix = Node::new(
        "mix",
        NodeOp::Math {
            operation: MathOp::Lerp,
            value_type: ValueType::Float,
        },
    );
    let mut output = Node::new(
        "output",
        NodeOp::GroupOutput {
            ports: [("value".into(), port("数值", "Value"))].into(),
        },
    );
    input.position = [0., 96.];
    weight.position = [192., 0.];
    mix.position = [384., 96.];
    output.position = [576., 96.];
    NodeGroup {
        name: "标量蒙版混合".into(),
        english_name: "Masked Scalar Mix".into(),
        nodes: vec![input, weight, mix, output],
        connections: vec![
            Connection::new("input", "reference", "mix", "a"),
            Connection::new("input", "value", "mix", "b"),
            Connection::new("input", "factor", "weight", "a"),
            Connection::new("input", "mask", "weight", "b"),
            Connection::new("weight", "value", "mix", "factor"),
            Connection::new("mix", "value", "output", "value"),
        ],
        input_node: "input".into(),
        output_node: "output".into(),
    }
}
