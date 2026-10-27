use motion_graph::*;
#[path = "support/image_fixture.rs"]
mod fixture;

#[test]
fn imported_images_are_portable_content_owned_and_exactly_packaged() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("source.png");
    std::fs::write(&input, fixture::image().png()).unwrap();
    let image = GraphImage::import(&input).unwrap();
    assert_eq!((image.width(), image.height()), (2, 2));
    assert!(
        format!("{image:?}").len() < 256,
        "diagnostics describe dimensions and identity, never PNG payload"
    );
    let graph = fixture::graph(image.clone());
    std::fs::remove_file(&input).unwrap();
    let encoded = graph.to_json().unwrap();
    assert!(!encoded.contains(&input.to_string_lossy().to_string()));
    assert_eq!(GraphDocument::from_json(&encoded).unwrap(), graph);
    let report = export_msfx(
        &graph,
        &CompileOptions::default(),
        &root.path().join("portable.msfx"),
    )
    .unwrap();
    assert_eq!(GraphDocument::load(&report.source_path).unwrap(), graph);
    let package = motion_effects::EffectPackage::open(&report.path).unwrap();
    assert_eq!(
        package.manifest.effects[0].resources,
        vec![image.resource_path()]
    );
    let compiled = compile(&graph, &CompileOptions::default()).unwrap();
    assert_eq!(
        compiled.resource(&image.resource_path()).unwrap(),
        image.png()
    );
    assert_eq!(compiled.report.package_resource_textures, 1);
    assert_eq!(compiled.report.graph_sample_sites, 4);
    assert!(compiled.shader.glsl.fragment.contains("#version 300 es"));
}

#[test]
fn source_reads_and_interactive_metadata_checks_keep_their_independent_limits() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("oversized.msgraph");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_DOCUMENT_BYTES as u64 + 1)
        .unwrap();
    assert_eq!(
        GraphDocument::load(&path).unwrap_err().diagnostic().code,
        ErrorCode::Budget
    );
    let mut graph = fixture::graph(fixture::image());
    graph.validate_metadata_budget().unwrap();
    graph.english_name = "m".repeat(MAX_GRAPH_METADATA_BYTES);
    assert_eq!(
        graph
            .validate_metadata_budget()
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::Budget
    );
    assert_eq!(
        graph.to_json().unwrap_err().diagnostic().code,
        ErrorCode::Budget
    );
    let image = serde_json::json!({"width": u32::MAX, "height": u32::MAX, "png": "bad"});
    let error = serde_json::from_value::<GraphImage>(image).unwrap_err();
    assert!(
        error.to_string().contains("decoded-image budget"),
        "dimension rejection must precede base64/codec work: {error}"
    );
}

#[test]
fn embedded_image_faults_keep_native_sources_after_document_loading() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::error::Error as _;
    let graph = fixture::graph(fixture::image());
    let id = graph.images.keys().next().unwrap();
    let mut source = serde_json::to_value(&graph).unwrap();
    source["images"][id]["png"] = STANDARD.encode(b"independent broken PNG").into();
    let error = GraphDocument::from_json(&source.to_string()).unwrap_err();
    assert_eq!(error.diagnostic().port, Some(format!("images.{id}")));
    let effect = error
        .source()
        .unwrap()
        .downcast_ref::<Box<motion_effects::Error>>()
        .unwrap();
    assert!(
        effect
            .source()
            .unwrap()
            .downcast_ref::<Box<image::ImageError>>()
            .is_some()
    );
    source["images"][id]["png"] = "!".into();
    let error = GraphDocument::from_json(&source.to_string()).unwrap_err();
    assert_eq!(error.diagnostic().port, Some(format!("images.{id}")));
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<base64::DecodeError>()
            .is_some()
    );
    assert_eq!(
        GraphDocument::from_json(&graph.to_json().unwrap()).unwrap(),
        graph
    );
}

#[test]
fn sharing_one_content_identity_uses_one_slot_in_baked_editable_and_preview() {
    let mut graph = fixture::graph(fixture::image());
    graph
        .nodes
        .push(Node::new("duplicate", graph.nodes[0].operation.clone()));
    for compiled in [
        compile(&graph, &CompileOptions::default()).unwrap(),
        compile_editable(&graph, &CompileOptions::default()).unwrap(),
        compile_preview(
            &graph,
            &CompileOptions::default(),
            &PortRef::new("image", "image"),
        )
        .unwrap(),
    ] {
        assert_eq!(compiled.report.package_resource_textures, 1);
        motion_effects::EffectPackage::from_bytes(
            compiled.package_bytes(&compiled.manifest).unwrap(),
        )
        .unwrap();
    }
    let original = compile(&demo_graph(), &CompileOptions::default()).unwrap();
    let mut unused = demo_graph();
    unused.format_version = IMAGE_ASSETS_FORMAT_VERSION;
    unused.images = graph.images;
    unused
        .nodes
        .push(Node::new("unused_image", graph.nodes[0].operation.clone()));
    let compiled = compile(&unused, &CompileOptions::default()).unwrap();
    assert_eq!(compiled.source, original.source);
    assert_eq!(compiled.manifest, original.manifest);
    assert_eq!(compiled.report.package_resource_textures, 0);
}

#[test]
fn sdk_slots_include_numeric_overflow_and_fail_explicitly() {
    let mut graph = fixture::many_images(4);
    assert_eq!(
        compile(&graph, &CompileOptions::default())
            .unwrap()
            .report
            .package_resource_textures,
        4
    );
    assert_eq!(
        compile_editable(&graph, &CompileOptions::default())
            .unwrap()
            .report
            .package_resource_textures,
        4
    );
    for index in 0..32 {
        graph.public_parameters.push(PublicParameter {
            id: format!("parameter_{index}"),
            name: "参数".into(),
            english_name: "Parameter".into(),
            value_type: ValueType::Float,
            default: [0.; 4],
            min: 0.,
            max: 1.,
            step: 0.1,
            units: "".into(),
            animatable: true,
        });
    }
    let error = compile_editable(&graph, &CompileOptions::default())
        .err()
        .unwrap();
    assert_eq!(error.diagnostic().code, ErrorCode::Budget);
    assert!(error.to_string().contains("four SDK resource slots"));
    let error = compile(&fixture::many_images(5), &CompileOptions::default())
        .err()
        .unwrap();
    assert_eq!(error.diagnostic().code, ErrorCode::Budget);
}

#[test]
fn malformed_or_missing_images_retain_original_faults_and_do_not_change_source() {
    use std::error::Error as _;
    let cause = GraphImage::from_png(b"independent invalid PNG fixture".to_vec())
        .err()
        .unwrap();
    assert!(cause.to_string().contains("node_image_decode"));
    let effect = cause
        .source()
        .unwrap()
        .downcast_ref::<Box<motion_effects::Error>>()
        .unwrap();
    let native = effect.source().unwrap();
    assert!(native.downcast_ref::<Box<image::ImageError>>().is_some());
    let graph = fixture::graph(fixture::image());
    let mut missing = graph.clone();
    missing.images.clear();
    let failure = validate(&missing, &CompileOptions::default())
        .unwrap_err()
        .diagnostic();
    assert_eq!(failure.code, ErrorCode::InvalidValue);
    assert_eq!(failure.node.as_deref(), Some("image"));
    assert_eq!(failure.port.as_deref(), Some("image"));
    assert_ne!(graph, missing);
    let mut value = serde_json::to_value(&graph).unwrap();
    let id = graph.images.keys().next().unwrap();
    value["images"][id]["width"] = 99.into();
    assert!(
        GraphDocument::from_json(&value.to_string())
            .unwrap_err()
            .to_string()
            .contains("dimensions")
    );
    let mut wrong = graph.clone();
    let (_, image) = wrong.images.pop_first().unwrap();
    wrong.images.insert("wrong".into(), image);
    assert!(
        wrong
            .validate_images()
            .unwrap_err()
            .to_string()
            .contains("SHA-256")
    );
    assert_eq!(
        GraphDocument::from_json(&graph.to_json().unwrap()).unwrap(),
        graph
    );
}

#[test]
fn image_payload_does_not_expand_metadata_budget_and_decode_is_preflighted() {
    let graph = fixture::graph(fixture::image());
    let mut value = serde_json::to_value(&graph).unwrap();
    value["name"] = "x".repeat(MAX_GRAPH_METADATA_BYTES).into();
    assert_eq!(
        GraphDocument::from_json(&value.to_string())
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::Budget
    );
    let mut value = serde_json::to_value(&graph).unwrap();
    let id = graph.images.keys().next().unwrap();
    value["images"][id]["width"] = 100000.into();
    value["images"][id]["height"] = 100000.into();
    value["images"][id]["png"] = "broken base64".into();
    assert_eq!(
        GraphDocument::from_json(&value.to_string())
            .unwrap_err()
            .diagnostic()
            .code,
        ErrorCode::Budget
    );
    let mut old = graph.clone();
    old.format_version = INPUT_DEFAULTS_FORMAT_VERSION;
    assert_eq!(
        old.to_json().unwrap_err().diagnostic().code,
        ErrorCode::DocumentVersion
    );
}
