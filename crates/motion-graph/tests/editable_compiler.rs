use motion_graph::*;

fn graph() -> GraphDocument {
    let mut graph = demo_graph();
    graph.public_parameters.clear();
    graph.nodes = vec![
        Node::new(
            "color",
            NodeOp::Color {
                value: [0.2, 0.3, 0.8, 1.],
            },
        ),
        Node::new("output", NodeOp::ColorOutput),
    ];
    graph.connections = vec![Connection::new("color", "value", "output", "color")];
    graph
}

#[test]
fn cache_owns_validated_output_and_rechecks_current_budgets_and_data() {
    let mut compiler = EditableCompiler::default();
    let mut graph = graph();
    let mut first = compiler.compile(&graph, &Default::default()).unwrap();
    assert_eq!(first.report.portable_compilations, 1);
    let valid = first.shader.wgsl.clone();
    first.shader.wgsl = "invalid mutable returned shader".into();
    first.source = "caller cannot poison the owned cache".into();
    graph.nodes[0].operation = NodeOp::Color {
        value: [0.8, 0.2, 0.1, 0.5],
    };
    let second = compiler.compile(&graph, &Default::default()).unwrap();
    assert_eq!(second.report.portable_compilations, 0);
    assert_eq!(second.shader.wgsl, valid);
    assert_ne!(
        first.manifest.effects[0].shader_data,
        second.manifest.effects[0].shader_data
    );

    let mut limit = CompileOptions::default();
    limit.max_shader_bytes = 1;
    assert!(compiler.compile(&graph, &limit).is_err());
    limit = CompileOptions::default();
    limit.max_samples_per_fragment = 0;
    assert!(
        compiler.compile(&graph, &limit).is_err(),
        "cache must not bypass a lower current sampling budget"
    );
    graph.nodes[0].operation = NodeOp::Color {
        value: [f32::NAN, 0., 0., 1.],
    };
    assert!(compiler.compile(&graph, &Default::default()).is_err());
    graph.nodes[0].operation = NodeOp::Color {
        value: [0.1, 0.2, 0.3, 1.],
    };
    assert_eq!(
        compiler
            .compile(&graph, &Default::default())
            .unwrap()
            .report
            .portable_compilations,
        0
    );
    assert_eq!(compiler.stats().portable_compilations, 1);
    assert!(compiler.stats().reuses >= 2);
    assert!(compiler.stats().retained_code_bytes > 0);
    compiler.clear();
    assert_eq!(compiler.stats().entries, 0);
    assert_eq!(compiler.stats().retained_code_bytes, 0);
    assert_eq!(
        compiler
            .compile(&graph, &Default::default())
            .unwrap()
            .report
            .portable_compilations,
        1
    );
}

#[test]
fn canonical_and_diagnostic_pair_reuse_and_changed_code_evicts_with_bounds() {
    let mut compiler = EditableCompiler::default();
    let mut graph = graph();
    graph.nodes.push(Node::new("time", NodeOp::Time));
    let port = node_ports(&graph.nodes[2], &[])
        .unwrap()
        .outputs
        .keys()
        .next()
        .unwrap()
        .clone();
    let target = PortRef::new("time", port);
    let a = compiler.compile(&graph, &Default::default()).unwrap();
    let b = compiler
        .compile_preview(&graph, &Default::default(), &target)
        .unwrap();
    assert_ne!(a.source, b.source);
    assert_eq!(compiler.stats().portable_compilations, 2);
    for i in 1..5 {
        graph.nodes[0].operation = NodeOp::Color {
            value: [i as f32 * 0.1, 0.2, 0.3, 1.],
        };
        assert_eq!(
            compiler
                .compile(&graph, &Default::default())
                .unwrap()
                .report
                .portable_compilations,
            0
        );
        assert_eq!(
            compiler
                .compile_preview(&graph, &Default::default(), &target)
                .unwrap()
                .report
                .portable_compilations,
            0
        );
    }
    // A different actual program must invoke the compiler. Pure names do not.
    graph.nodes.push(Node::new("uv", NodeOp::Uv));
    let uv = PortRef::new("uv", "value");
    assert_eq!(
        compiler
            .compile_preview(&graph, &Default::default(), &uv)
            .unwrap()
            .report
            .portable_compilations,
        1
    );
    assert_eq!(compiler.stats().entries, EDITABLE_CACHE_ENTRIES);
    assert!(compiler.stats().retained_code_bytes <= EDITABLE_CACHE_CODE_BYTES);
    assert_eq!(
        compiler
            .compile(&graph, &Default::default())
            .unwrap()
            .report
            .portable_compilations,
        1,
        "oldest canonical entry was evicted"
    );
    assert_eq!(compiler.stats().entries, EDITABLE_CACHE_ENTRIES);
}

#[test]
fn folded_and_default_values_use_the_same_actual_compiler_entry() {
    let mut graph = graph();
    graph.format_version = INPUT_DEFAULTS_FORMAT_VERSION;
    graph.nodes.extend([
        Node::new("a", NodeOp::Float { value: 0.25 }),
        Node::new(
            "sum",
            NodeOp::Math {
                operation: MathOp::Add,
                value_type: ValueType::Float,
            },
        ),
    ]);
    graph
        .nodes
        .last_mut()
        .unwrap()
        .input_defaults
        .insert("b".into(), InputValue::Float(0.25));
    graph
        .connections
        .push(Connection::new("a", "value", "sum", "a"));
    let mut compiler = EditableCompiler::default();
    let target = PortRef::new("sum", "value");
    let first = compiler
        .compile_preview(&graph, &Default::default(), &target)
        .unwrap();
    graph.nodes[2].operation = NodeOp::Float { value: 0.5 };
    graph.nodes[3]
        .input_defaults
        .insert("b".into(), InputValue::Float(0.125));
    let next = compiler
        .compile_preview(&graph, &Default::default(), &target)
        .unwrap();
    assert_eq!(next.report.folded_math_nodes, 1);
    assert_eq!(compiler.stats().portable_compilations, 1);
    assert_ne!(
        first.manifest.effects[0].shader_data,
        next.manifest.effects[0].shader_data
    );
}
