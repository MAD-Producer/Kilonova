use motion_graph::*;

#[test]
fn typed_preview_compiles_numeric_color_bool_and_image_without_mutating_export() {
    for operation in [
        NodeOp::Float { value: 0.25 },
        NodeOp::Vec2 { value: [0.2, 0.4] },
        NodeOp::Vec3 {
            value: [0.2, 0.4, 0.6],
        },
        NodeOp::Bool { value: true },
        NodeOp::Color {
            value: [0.1, 0.3, 0.8, 0.5],
        },
        NodeOp::Image {
            source: ImageSource::Input,
        },
    ] {
        let mut graph = demo_graph();
        graph.nodes.push(Node::new("inspected", operation));
        let original = graph.clone();
        let options = CompileOptions::default();
        let normal = compile(&graph, &options).unwrap();
        let port = if matches!(graph.nodes.last().unwrap().operation, NodeOp::Image { .. }) {
            "image"
        } else {
            "value"
        };
        let preview = compile_preview(&graph, &options, &PortRef::new("inspected", port)).unwrap();
        assert_eq!(preview.report.reachable_nodes, 1);
        if port == "image" {
            assert!(preview.source.contains("sample_input(p)"));
        } else {
            assert!(preview.source.contains("inspected"));
        }
        assert_eq!(
            preview.report.parameter_slots,
            normal.report.parameter_slots
        );
        assert_eq!(graph, original);
        assert_eq!(compile(&graph, &options).unwrap().source, normal.source);
        assert_ne!(preview.source, normal.source);
    }
}

#[test]
fn preview_isolates_dependencies_but_keeps_required_inputs_cycles_and_budgets_checked() {
    let mut graph = demo_graph();
    graph.nodes.push(Node::new(
        "unrelated",
        NodeOp::Unsupported {
            feature: "future renderer".into(),
        },
    ));
    let original = graph.clone();
    let options = CompileOptions::default();
    assert!(compile(&graph, &options).is_err());
    compile_preview(&graph, &options, &PortRef::new("circle", "distance")).unwrap();
    assert_eq!(graph, original);
    graph
        .connections
        .retain(|c| c.to.node != "circle" || c.to.port != "radius");
    assert_eq!(
        compile_preview(&graph, &options, &PortRef::new("circle", "distance"))
            .err()
            .unwrap()
            .diagnostic()
            .code,
        ErrorCode::MissingInput
    );
    graph.nodes.push(Node::new(
        "cycle",
        NodeOp::Math {
            operation: MathOp::Abs,
            value_type: ValueType::Float,
        },
    ));
    graph
        .connections
        .push(Connection::new("cycle", "value", "cycle", "value"));
    assert_eq!(
        compile_preview(&graph, &options, &PortRef::new("cycle", "value"))
            .err()
            .unwrap()
            .diagnostic()
            .code,
        ErrorCode::Cycle
    );
    let mut limited = options;
    limited.max_nodes = 1;
    assert_eq!(
        compile_preview(&graph, &limited, &PortRef::new("color", "value"))
            .err()
            .unwrap()
            .diagnostic()
            .code,
        ErrorCode::Budget
    );
}

#[test]
fn image_preview_accounts_for_sampling_and_rejects_missing_output() {
    let mut graph = demo_graph();
    graph.nodes.push(Node::new(
        "image",
        NodeOp::Image {
            source: ImageSource::Source,
        },
    ));
    let mut options = CompileOptions::default();
    let preview = compile_preview(&graph, &options, &PortRef::new("image", "image")).unwrap();
    assert_eq!(preview.report.graph_sample_sites, 1);
    assert_eq!(preview.report.total_sample_sites, 2);
    options.max_samples_per_fragment = 1;
    assert_eq!(
        compile_preview(&graph, &options, &PortRef::new("image", "image"))
            .err()
            .unwrap()
            .diagnostic()
            .code,
        ErrorCode::Budget
    );
    assert_eq!(
        compile_preview(&graph, &options, &PortRef::new("image", "missing"))
            .err()
            .unwrap()
            .diagnostic()
            .code,
        ErrorCode::UnknownPort
    );
}
