use motion_effects::{AlphaMode, WorkingSpace};
use motion_graph::*;

fn graph() -> GraphDocument {
    let mut graph = GraphDocument {
        format_version: GRAPH_FORMAT_VERSION,
        id: "masked-scalar-proof".into(),
        name: "标量混合验收".into(),
        english_name: "Masked Scalar Proof".into(),
        // Scalar arithmetic has no colour encoding. This explicit display
        // wrapper merely makes [0,1] test values visible in an SDR capture.
        working_space: WorkingSpace::Srgb,
        alpha_mode: AlphaMode::Straight,
        nodes: vec![],
        connections: vec![],
        public_parameters: vec![],
        images: Default::default(),
    };
    for (id, value) in [
        ("reference", 0.8),
        ("value", 0.2),
        ("factor", 1.),
        ("mask", 1.),
    ] {
        graph.public_parameters.push(PublicParameter {
            id: id.into(),
            name: id.into(),
            english_name: id.into(),
            value_type: ValueType::Float,
            default: [value, 0., 0., 0.],
            min: 0.,
            max: if id == "reference" || id == "value" {
                16.
            } else {
                1.
            },
            step: 0.01,
            units: String::new(),
            animatable: true,
        });
        graph.nodes.push(Node::new(
            id,
            NodeOp::Parameter {
                parameter: id.into(),
            },
        ));
        graph
            .connections
            .push(Connection::new(id, "value", "mixed", id));
    }
    graph.nodes.extend([
        Node::new(
            "mixed",
            NodeOp::Group {
                graph: Box::new(scalar_presets::masked_lerp()),
            },
        ),
        Node::new("opaque", NodeOp::Float { value: 1. }),
        Node::new("display", NodeOp::CombineColor),
        Node::new("final", NodeOp::ColorOutput),
    ]);
    for port in ["r", "g", "b"] {
        graph
            .connections
            .push(Connection::new("mixed", "value", "display", port));
    }
    graph.connections.extend([
        Connection::new("opaque", "value", "display", "a"),
        Connection::new("display", "color", "final", "color"),
    ]);
    graph
}

#[test]
fn masked_scalar_preset_uses_existing_operations_and_preserves_its_document() {
    let group = scalar_presets::masked_lerp();
    assert_eq!(group.nodes.len(), 4);
    assert!(group.nodes.iter().all(|node| matches!(
        node.operation,
        NodeOp::GroupInput { .. }
            | NodeOp::GroupOutput { .. }
            | NodeOp::Math {
                operation: MathOp::Multiply | MathOp::Lerp,
                value_type: ValueType::Float
            }
    )));
    let document = graph();
    assert_eq!(
        GraphDocument::from_json(&serde_json::to_string(&document).unwrap()).unwrap(),
        document
    );
    validate(&document, &CompileOptions::default()).unwrap();
}

#[test]
fn masked_scalar_parameters_compile_through_normal_and_editable_paths() {
    let document = graph();
    for compiled in [
        compile(&document, &CompileOptions::default()).unwrap(),
        compile_editable(&document, &CompileOptions::default()).unwrap(),
    ] {
        assert!(compiled.shader.wgsl.contains("mix("));
        assert!(!compiled.shader.glsl.fragment.is_empty());
        assert_eq!(compiled.report.graph_sample_sites, 0);
        assert_eq!(compiled.report.passes, 1);
        for id in ["reference", "value", "factor", "mask"] {
            assert!(compiled.manifest.effects[0]
                .params
                .iter()
                .any(|p| p.id == id));
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn exported_masked_scalar_gpu_values_match_independent_weights_without_alpha_masking() {
    use motion_core::{EffectInstance, Layer, Project};
    use motion_effects::{EffectPackage, Registry};
    use motion_render::{Renderer, Scene};
    use std::sync::Arc;
    let directory = tempfile::tempdir().unwrap();
    let document = graph();
    let compiled = compile(&document, &CompileOptions::default()).unwrap();
    let exported = export_msfx(
        &document,
        &CompileOptions::default(),
        &directory.path().join("masked.msfx"),
    )
    .unwrap();
    let package = Arc::new(EffectPackage::open(&exported.path).unwrap());
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    let mut project = Project::new(16, 8, 30, 30).unwrap();
    project.background = [0.; 4];
    let mut layer = Layer::solid(1, "Scalar display", [16., 8.], [8., 4., 0.], [1.; 4]);
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [16., 8.],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(registry);
    let target = renderer.capture_target(16, 8).unwrap();
    let artifacts = std::env::var_os("MOTION_DEPTH_BLEND_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(root) = &artifacts {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(root.join("masked-scalar.wgsl"), &compiled.shader.wgsl).unwrap();
        std::fs::write(
            root.join("masked-scalar.glsl"),
            &compiled.shader.glsl.fragment,
        )
        .unwrap();
        std::fs::write(
            root.join("masked-scalar.msgraph"),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
    }
    // Independently specified expected values, not a call to the shared kernel.
    for (index, (factor, mask, expected)) in [
        (0., 1., 0.8_f64),
        (1., 1., 0.2),
        (1., 0., 0.8),
        (1., 0.5, 0.5),
        (0.5, 0.5, 0.65),
    ]
    .into_iter()
    .enumerate()
    {
        let params = &mut project.layers[0].effects[0].params;
        params.get_mut("factor").unwrap().track.value[0] = factor;
        params.get_mut("mask").unwrap().track.value[0] = mask;
        let mut scene = Scene::new(&project).unwrap();
        scene
            .sample(&project, 15.5, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        let expected_byte = (expected * 255.).round() as u8;
        for pixel in pixels.chunks_exact(4) {
            assert!(
                pixel[..3]
                    .iter()
                    .all(|channel| channel.abs_diff(expected_byte) <= 2),
                "case={index}, actual={pixel:?}, expected={expected_byte}"
            );
            assert_eq!(pixel[3], 255, "mask weight must not become alpha");
        }
        if let Some(root) = &artifacts {
            image::save_buffer(
                root.join(format!("scalar-{index}.png")),
                &pixels,
                16,
                8,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
