use motion_graph::*;

pub fn gradient(mode: GradientMode) -> GraphDocument {
    let mut graph =
        GraphDocument::from_json(include_str!("../../examples/compact-gradient.msgraph")).unwrap();
    let node = graph
        .nodes
        .iter_mut()
        .find(|node| node.id == "gradient")
        .unwrap();
    node.operation = NodeOp::Gradient { mode };
    node.input_defaults = authoring_input_defaults(&node.operation);
    graph
}
