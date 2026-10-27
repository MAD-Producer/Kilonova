use motion_graph::*;
#[path = "support/uv_mapping_fixture.rs"]
mod fixture;

#[test]
fn mapping_has_explicit_uv_units_and_typed_defaults() {
    let mut graph = fixture::graph([0.; 2], 0., [1.; 2], [0.5; 2]);
    let mapping = graph.nodes.iter().find(|node| node.id == "mapping").unwrap();
    let ports = node_port_specs(mapping, &[]).unwrap();
    assert_eq!(ports.inputs.len(), 5);
    for id in ["coordinate", "translation", "pivot"] {
        assert_eq!(ports.inputs[id].semantic, PortSemantic::Uv);
        assert!(!ports.inputs[id].english_description.is_empty());
    }
    assert_eq!(ports.outputs["value"].semantic, PortSemantic::Uv);
    assert!(ports.inputs["rotation"].english_name.contains("degrees"));
    assert!(!mapping.input_defaults.contains_key("coordinate"));
    graph.connections.retain(|edge| edge.to.node != "mapping");
    assert_eq!(compile(&graph, &CompileOptions::default()).err().unwrap().diagnostic().code, ErrorCode::MissingInput);
    graph.nodes.iter_mut().find(|n| n.id == "uv").unwrap().operation = NodeOp::Position;
    assert!(check_connection(&graph, &PortRef::new("uv", "value"), &PortRef::new("mapping", "coordinate")).is_err());
}

#[test]
fn mapping_roundtrips_exports_and_preserves_editable_parameters() {
    let dir = tempfile::tempdir().unwrap();
    let graph = fixture::graph([0.15, -0.1], 37.5, [-0.5, 1.2], [0.2, 0.8]);
    let text = graph.to_json().unwrap();
    assert_eq!(GraphDocument::from_json(&text).unwrap(), graph);
    let compiled = compile(&graph, &CompileOptions::default()).unwrap();
    assert!(compiled.shader.glsl.fragment.contains("#version 300 es"));
    assert_eq!(compiled.report.package_resource_textures, 0);
    let report = export_msfx(&graph, &CompileOptions::default(), &dir.path().join("mapping.msfx")).unwrap();
    assert_eq!(GraphDocument::load(&report.source_path).unwrap(), graph);
    motion_effects::EffectPackage::open(&report.path).unwrap();
    let editable = compile_editable(&graph, &CompileOptions::default()).unwrap();
    let mut changed = graph.clone();
    changed.nodes.iter_mut().find(|n| n.id == "mapping").unwrap()
        .input_defaults.insert("rotation".into(), InputValue::Float(-23.));
    let updated = compile_editable(&changed, &CompileOptions::default()).unwrap();
    assert_eq!(editable.source, updated.source, "changing mapping values must not rebuild shader source");
    assert_ne!(editable.manifest.effects[0].shader_data, updated.manifest.effects[0].shader_data);
}

#[test]
fn malformed_mapping_inputs_are_rejected_with_node_and_port_context() {
    for (port, value) in [("rotation", InputValue::Float(f32::INFINITY)),
        ("scale", InputValue::Float(1.)), ("translation", InputValue::Vec2([1.0e7, 0.]))] {
        let mut graph = fixture::graph([0.; 2], 0., [1.; 2], [0.5; 2]);
        graph.nodes.iter_mut().find(|n| n.id == "mapping").unwrap().input_defaults.insert(port.into(), value);
        let error = compile(&graph, &CompileOptions::default()).err().unwrap();
        assert!(error.to_string().contains("mapping"), "{error}");
        assert!(error.to_string().contains(port), "{error}");
    }
}
