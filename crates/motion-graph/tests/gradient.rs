use motion_graph::*;
#[path = "support/gradient_fixture.rs"]
mod fixture;

#[test]
fn gradient_modes_roundtrip_compile_and_export_a_portable_single_pass() {
    let folder = tempfile::tempdir().unwrap();
    for mode in [
        GradientMode::Linear,
        GradientMode::Radial,
        GradientMode::Conic,
    ] {
        let graph = fixture::gradient(mode);
        assert_eq!(
            GraphDocument::from_json(&graph.to_json().unwrap()).unwrap(),
            graph
        );
        let compiled = compile(&graph, &CompileOptions::default()).unwrap();
        assert_eq!(compiled.report.passes, 1);
        assert_eq!(compiled.report.graph_sample_sites, 0);
        assert_eq!(compiled.report.package_resource_textures, 0);
        assert!(compiled.shader.glsl.fragment.contains("#version 300 es"));
        let report = export_msfx(
            &graph,
            &CompileOptions::default(),
            &folder.path().join(format!("{mode:?}.msfx")),
        )
        .unwrap();
        assert_eq!(GraphDocument::load(&report.source_path).unwrap(), graph);
        motion_effects::EffectPackage::open(&report.path).unwrap();
        let factor = compile_preview(
            &graph,
            &CompileOptions::default(),
            &PortRef::new("gradient", "factor"),
        )
        .unwrap();
        assert_eq!(factor.report.passes, 1);
    }
}

#[test]
fn gradient_requires_explicit_uv_and_exposes_units_and_both_outputs() {
    let mut graph = fixture::gradient(GradientMode::Linear);
    let node = graph
        .nodes
        .iter()
        .find(|node| node.id == "gradient")
        .unwrap();
    let specs = node_port_specs(node, &[]).unwrap();
    assert_eq!(specs.inputs.len(), 6);
    for port in ["coordinate", "start", "end"] {
        assert_eq!(specs.inputs[port].semantic, PortSemantic::Uv);
        assert!(!specs.inputs[port].english_description.is_empty());
    }
    assert_eq!(specs.outputs["factor"].value_type, ValueType::Float);
    assert_eq!(specs.outputs["color"].value_type, ValueType::Color);
    graph.connections.remove(0);
    let missing = compile(&graph, &CompileOptions::default()).err().unwrap();
    assert_eq!(missing.diagnostic().code, ErrorCode::MissingInput);
    graph.nodes[0].operation = NodeOp::Position;
    assert!(check_connection(
        &graph,
        &PortRef::new("uv", "value"),
        &PortRef::new("gradient", "coordinate")
    )
    .is_err());
    let malformed = graph
        .to_json()
        .unwrap()
        .replace("\"linear\"", "\"unknown_gradient_mode\"");
    assert!(GraphDocument::from_json(&malformed).is_err());
}
