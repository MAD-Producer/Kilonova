use motion_graph::*;
#[path = "support/noise_fixture.rs"]
mod fixture;

#[test]
fn noise_modes_and_octaves_roundtrip_export_and_compile_on_the_portable_path() {
    let folder = tempfile::tempdir().unwrap();
    for mode in [NoiseMode::Value, NoiseMode::Perlin] {
        for octaves in 1..=4 {
            let graph = fixture::noise(mode, octaves);
            assert_eq!(
                GraphDocument::from_json(&graph.to_json().unwrap()).unwrap(),
                graph
            );
            let compiled = compile(&graph, &CompileOptions::default()).unwrap();
            assert_eq!(compiled.report.passes, 1);
            assert_eq!(compiled.report.graph_sample_sites, 0);
            assert_eq!(compiled.report.package_resource_textures, 0);
            assert!(compiled.shader.glsl.fragment.contains("#version 300 es"));
            let exported = export_msfx(
                &graph,
                &CompileOptions::default(),
                &folder.path().join(format!("{mode:?}-{octaves}.msfx")),
            )
            .unwrap();
            assert_eq!(GraphDocument::load(&exported.source_path).unwrap(), graph);
            motion_effects::EffectPackage::open(&exported.path).unwrap();
            compile_preview(
                &graph,
                &CompileOptions::default(),
                &PortRef::new("noise", "factor"),
            )
            .unwrap();
        }
    }
}

#[test]
fn noise_rejects_unbounded_octaves_with_node_context() {
    for octaves in [0, 5, 255] {
        let graph = fixture::noise(NoiseMode::Value, octaves);
        let err = compile(&graph, &CompileOptions::default()).err().unwrap();
        assert_eq!(err.diagnostic().code, ErrorCode::InvalidValue);
        assert_eq!(err.diagnostic().node.as_deref(), Some("noise"));
        assert!(err.to_string().contains("1..=4"));
    }
}

#[test]
fn noise_requires_uv_and_typed_inputs_and_has_no_implicit_sources() {
    let mut graph = fixture::noise(NoiseMode::Perlin, 4);
    let ports = node_port_specs(&graph.nodes[1], &[]).unwrap();
    assert_eq!(ports.inputs.len(), 4);
    assert_eq!(ports.inputs["coordinate"].semantic, PortSemantic::Uv);
    assert_eq!(ports.inputs["offset"].semantic, PortSemantic::Uv);
    assert_eq!(ports.outputs["factor"].value_type, ValueType::Float);
    assert_eq!(ports.outputs["color"].value_type, ValueType::Color);
    graph.connections.remove(0);
    assert_eq!(
        compile(&graph, &CompileOptions::default())
            .err()
            .unwrap()
            .diagnostic()
            .code,
        ErrorCode::MissingInput
    );
    graph.nodes[0].operation = NodeOp::Position;
    assert!(check_connection(
        &graph,
        &PortRef::new("uv", "value"),
        &PortRef::new("noise", "coordinate")
    )
    .is_err());
    graph.nodes[1]
        .input_defaults
        .insert("seed".into(), InputValue::Vec2([0.; 2]));
    assert!(compile(&graph, &CompileOptions::default()).is_err());
    assert!(GraphDocument::from_json(
        &graph
            .to_json()
            .unwrap()
            .replace("\"perlin\"", "\"unknown\"")
    )
    .is_err());
}

#[test]
fn unused_noise_does_not_emit_helper_or_increase_reachable_cost() {
    let mut graph =
        GraphDocument::from_json(include_str!("../examples/solid-color.msgraph")).unwrap();
    let baseline = compile(&graph, &CompileOptions::default()).unwrap();
    graph.nodes.push(Node::new(
        "unused_noise",
        NodeOp::Noise {
            mode: NoiseMode::Perlin,
            octaves: 4,
        },
    ));
    let compiled = compile(&graph, &CompileOptions::default()).unwrap();
    assert_eq!(compiled.source, baseline.source);
    assert!(!compiled.source.contains("fn ng_noise"));
    let active = compile(
        &fixture::noise(NoiseMode::Perlin, 4),
        &CompileOptions::default(),
    )
    .unwrap();
    assert!(active.source.contains("fn ng_noise"));
    assert!(!active.source.contains("for ("));
    assert!(!active.source.contains("loop {"));
}
