use motion_graph::*;
#[path = "support/group_fixture.rs"]
mod fixture;

#[test]
fn optional_names_read_legacy_graphs_and_roundtrip_without_changing_wgsl() {
    let graph = demo_graph();
    let json = graph.to_json().unwrap();
    assert!(!json.contains("display_name"));
    let mut graph = GraphDocument::from_json(&json).unwrap();
    let shader = compile(&graph, &CompileOptions::default()).unwrap().source;
    graph.nodes[0].display_name = "Named node · 主色".into();
    let saved = graph.to_json().unwrap();
    assert_eq!(GraphDocument::from_json(&saved).unwrap(), graph);
    assert_eq!(
        compile(&graph, &CompileOptions::default()).unwrap().source,
        shader
    );
}

#[test]
fn invalid_names_are_rejected_even_for_unreachable_and_structured_nodes() {
    for mut graph in [demo_graph(), fixture::grouped_math()] {
        for invalid in ["\n".to_owned(), "字".repeat(41), " ".into()] {
            graph.nodes[0].display_name = invalid;
            let error = validate(&graph, &CompileOptions::default())
                .unwrap_err()
                .diagnostic();
            assert_eq!(error.code, ErrorCode::InvalidValue);
            assert_eq!(error.node.as_deref(), Some(graph.nodes[0].id.as_str()));
            assert_eq!(error.port.as_deref(), Some("display_name"));
        }
    }
    let mut graph = fixture::grouped_math();
    let group = graph
        .nodes
        .iter_mut()
        .find(|n| matches!(n.operation, NodeOp::Group { .. }))
        .unwrap();
    group.display_name = "x\ny".into();
    assert_eq!(
        validate(&graph, &CompileOptions::default())
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::InvalidValue
    );
}
