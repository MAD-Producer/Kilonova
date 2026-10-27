use motion_graph::*;

pub fn graph(translation: [f32; 2], rotation: f32, scale: [f32; 2], pivot: [f32; 2]) -> GraphDocument {
    let mut graph = GraphDocument::from_json(include_str!("../../examples/compact-gradient.msgraph")).unwrap();
    graph.id = "uv_mapping_test".into();
    let mut mapping = Node::new("mapping", NodeOp::UvMapping);
    mapping.input_defaults = authoring_input_defaults(&mapping.operation);
    mapping.input_defaults.insert("translation".into(), InputValue::Vec2(translation));
    mapping.input_defaults.insert("rotation".into(), InputValue::Float(rotation));
    mapping.input_defaults.insert("scale".into(), InputValue::Vec2(scale));
    mapping.input_defaults.insert("pivot".into(), InputValue::Vec2(pivot));
    graph.nodes.push(mapping);
    let gradient = graph.nodes.iter_mut().find(|node| node.id == "gradient").unwrap();
    gradient.input_defaults.insert("start".into(), InputValue::Vec2([0., 0.]));
    gradient.input_defaults.insert("end".into(), InputValue::Vec2([1., 0.]));
    gradient.input_defaults.insert("a".into(), InputValue::Color([0., 0., 0., 1.]));
    gradient.input_defaults.insert("b".into(), InputValue::Color([1.; 4]));
    graph.connections.retain(|edge| edge.to.node != "gradient");
    graph.connections.extend([
        Connection::new("uv", "value", "mapping", "coordinate"),
        Connection::new("mapping", "value", "gradient", "coordinate"),
    ]);
    graph
}
