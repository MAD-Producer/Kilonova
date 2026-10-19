use motion_graph::*;
#[path = "support/blend_fixture.rs"]
mod fixture;

#[test]
fn all_blend_modes_roundtrip_export_and_validate_on_the_portable_single_pass() {
    let folder = tempfile::tempdir().unwrap();
    for mode in ColorBlendMode::ALL {
        let graph = fixture::blend(mode);
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
            &folder.path().join(format!("{mode:?}.msfx")),
        )
        .unwrap();
        assert_eq!(GraphDocument::load(&exported.source_path).unwrap(), graph);
        motion_effects::EffectPackage::open(&exported.path).unwrap();
        compile_preview(
            &graph,
            &CompileOptions::default(),
            &PortRef::new("blend", "color"),
        )
        .unwrap();
    }
}

#[test]
fn blend_ports_are_typed_documented_and_do_not_accept_unknown_modes() {
    let mut graph = fixture::blend(ColorBlendMode::Normal);
    let specs = node_port_specs(&graph.nodes[0], &[]).unwrap();
    assert_eq!(specs.inputs.len(), 3);
    assert_eq!(specs.inputs["backdrop"].value_type, ValueType::Color);
    assert_eq!(specs.inputs["source"].semantic, PortSemantic::Color);
    assert_eq!(specs.inputs["factor"].value_type, ValueType::Float);
    assert!(specs.inputs["factor"]
        .english_description
        .contains("source alpha"));
    graph.nodes[0]
        .input_defaults
        .insert("source".into(), InputValue::Float(0.5));
    let err = compile(&graph, &CompileOptions::default()).err().unwrap();
    assert_eq!(err.diagnostic().node.as_deref(), Some("blend"));
    assert_eq!(err.diagnostic().port.as_deref(), Some("source"));
    let text = fixture::blend(ColorBlendMode::Normal).to_json().unwrap();
    assert!(
        GraphDocument::from_json(&text.replace("\"normal\"", "\"unknown_blend_mode\"")).is_err()
    );
}

#[test]
fn unused_blend_has_no_helper_cost_or_effect_on_existing_graph_source() {
    let mut graph =
        GraphDocument::from_json(include_str!("../examples/solid-color.msgraph")).unwrap();
    let plain = compile(&graph, &CompileOptions::default()).unwrap();
    graph.nodes.push(Node::new(
        "unused_blend",
        NodeOp::Blend {
            mode: ColorBlendMode::SoftLight,
        },
    ));
    let draft = compile(&graph, &CompileOptions::default()).unwrap();
    assert_eq!(plain.source, draft.source);
    assert!(!draft.source.contains("fn ng_blend"));
    assert_eq!(
        draft.report.graph_sample_sites,
        plain.report.graph_sample_sites
    );
    assert_eq!(draft.report.passes, plain.report.passes);
}
