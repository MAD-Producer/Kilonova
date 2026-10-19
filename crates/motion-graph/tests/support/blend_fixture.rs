use motion_graph::*;

pub fn blend(mode: ColorBlendMode) -> GraphDocument {
    let mut graph =
        GraphDocument::from_json(include_str!("../../examples/solid-color.msgraph")).unwrap();
    graph.id = "blend_test".into();
    graph.format_version = INPUT_DEFAULTS_FORMAT_VERSION;
    let operation = NodeOp::Blend { mode };
    let mut node = Node::new("blend", operation.clone());
    node.input_defaults = authoring_input_defaults(&operation);
    graph.nodes = vec![node, Node::new("output", NodeOp::ColorOutput)];
    graph.connections = vec![Connection::new("blend", "color", "output", "color")];
    graph
}

#[allow(dead_code)]
pub fn parameterized(mode: ColorBlendMode) -> GraphDocument {
    let mut graph = blend(mode);
    for (id, ty, default) in [
        ("backdrop", ValueType::Color, [0.1, 0.3, 0.8, 1.]),
        ("source", ValueType::Color, [0.95, 0.45, 0.15, 1.]),
        ("factor", ValueType::Float, [1., 0., 0., 0.]),
    ] {
        graph.public_parameters.push(PublicParameter {
            id: id.into(),
            name: id.into(),
            english_name: id.into(),
            value_type: ty,
            default,
            min: if ty == ValueType::Float { -2. } else { 0. },
            max: if ty == ValueType::Float { 2. } else { 1. },
            step: 0.01,
            units: String::new(),
            animatable: true,
        });
        graph.nodes.push(Node::new(
            id,
            NodeOp::Parameter {
                parameter: id.into(),
            },
        ));
        graph
            .connections
            .push(Connection::new(id, "value", "blend", id));
    }
    graph
}
