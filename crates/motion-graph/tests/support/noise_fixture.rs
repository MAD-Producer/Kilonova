use motion_graph::*;

pub fn noise(mode: NoiseMode, octaves: u8) -> GraphDocument {
    let mut graph =
        GraphDocument::from_json(include_str!("../../examples/compact-noise.msgraph")).unwrap();
    graph.nodes[1].operation = NodeOp::Noise { mode, octaves };
    graph
}
