use motion_effects::{AlphaMode, EffectPackage, Registry, WorkingSpace};
use motion_graph::*;
use std::sync::Arc;

fn color_graph(overflow: bool, color: [f32; 4]) -> GraphDocument {
    let mut graph = demo_graph();
    graph.working_space = WorkingSpace::Linear;
    graph.alpha_mode = AlphaMode::Premultiplied;
    graph.nodes = vec![
        Node::new("color", NodeOp::Color { value: color }),
        Node::new("output", NodeOp::ColorOutput),
    ];
    graph.connections = vec![Connection::new("color", "value", "output", "color")];
    graph.public_parameters.clear();
    if overflow {
        for i in 0..32 {
            graph.public_parameters.push(PublicParameter {
                id: format!("p{i}"),
                name: format!("P{i}"),
                english_name: format!("P{i}"),
                value_type: ValueType::Float,
                default: [0.; 4],
                min: 0.,
                max: 1.,
                step: 0.01,
                units: String::new(),
                animatable: true,
            });
        }
    }
    graph
}

#[test]
fn all_literal_types_keep_code_stable_across_value_changes() {
    for (first, second) in [
        (
            NodeOp::Float { value: -0.0 },
            NodeOp::Float { value: 0.31415927 },
        ),
        (
            NodeOp::Vec2 { value: [1., 1.] },
            NodeOp::Vec2 {
                value: [-12., 0.125],
            },
        ),
        (
            NodeOp::Vec3 { value: [1.; 3] },
            NodeOp::Vec3 {
                value: [0.1, 0.2, 0.3],
            },
        ),
        (
            NodeOp::Color { value: [1.; 4] },
            NodeOp::Color {
                value: [0.1, 0.4, 0.8, 0.5],
            },
        ),
        (NodeOp::Bool { value: true }, NodeOp::Bool { value: false }),
    ] {
        let mut graph = demo_graph();
        graph.nodes.push(Node::new("inspected", first));
        let target = PortRef::new("inspected", "value");
        let first = compile_preview_editable(&graph, &Default::default(), &target).unwrap();
        graph.nodes.last_mut().unwrap().operation = second;
        let second = compile_preview_editable(&graph, &Default::default(), &target).unwrap();
        assert_eq!(first.source, second.source);
        assert_eq!(first.shader.wgsl, second.shader.wgsl);
        assert_eq!(first.shader.glsl.fragment, second.shader.glsl.fragment);
        assert_ne!(
            first.manifest.effects[0].shader_data,
            second.manifest.effects[0].shader_data
        );
    }
}

#[test]
fn defaults_and_folded_values_are_data_without_value_sensitive_cse() {
    let mut graph = color_graph(false, [1.; 4]);
    graph.format_version = INPUT_DEFAULTS_FORMAT_VERSION;
    graph.nodes.extend([
        Node::new("a", NodeOp::Float { value: 0.25 }),
        Node::new("b", NodeOp::Float { value: 0.25 }),
        Node::new(
            "sum",
            NodeOp::Math {
                operation: MathOp::Add,
                value_type: ValueType::Float,
            },
        ),
    ]);
    graph.connections.extend([
        Connection::new("a", "value", "sum", "a"),
        Connection::new("b", "value", "sum", "b"),
    ]);
    let target = PortRef::new("sum", "value");
    let first = compile_preview_editable(&graph, &Default::default(), &target).unwrap();
    assert_eq!(first.report.folded_math_nodes, 1);
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "b")
        .unwrap()
        .operation = NodeOp::Float { value: 0.5 };
    let second = compile_preview_editable(&graph, &Default::default(), &target).unwrap();
    assert_eq!(
        first.source, second.source,
        "equal literals becoming different must not change code/CSE"
    );
    assert_ne!(
        first.manifest.effects[0].shader_data,
        second.manifest.effects[0].shader_data
    );
    graph
        .connections
        .retain(|c| c.to != PortRef::new("sum", "b"));
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "sum")
        .unwrap()
        .input_defaults
        .insert("b".into(), InputValue::Float(0.75));
    let first = compile_preview_editable(&graph, &Default::default(), &target).unwrap();
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "sum")
        .unwrap()
        .input_defaults
        .insert("b".into(), InputValue::Float(0.125));
    let second = compile_preview_editable(&graph, &Default::default(), &target).unwrap();
    assert_eq!(first.source, second.source);
    assert_eq!(second.report.folded_math_nodes, 1);
    assert_ne!(
        first.manifest.effects[0].shader_data,
        second.manifest.effects[0].shader_data
    );
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "sum")
        .unwrap()
        .operation = NodeOp::Math {
        operation: MathOp::Multiply,
        value_type: ValueType::Float,
    };
    // A fully folded operation can legitimately have identical GPU code. Its
    // recomputed private result must still differ after this structural edit.
    let structural = compile_preview_editable(&graph, &Default::default(), &target).unwrap();
    assert_ne!(
        second.manifest.effects[0].shader_data,
        structural.manifest.effects[0].shader_data
    );
}

#[test]
fn actual_package_loader_checks_data_version_capability_capacity_and_float_bits() {
    for overflow in [false, true] {
        let compiled = compile_editable(
            &color_graph(overflow, [0.1, 0.3, 0.8, 0.5]),
            &Default::default(),
        )
        .unwrap();
        let bytes = compiled.package_bytes(&compiled.manifest).unwrap();
        let package = EffectPackage::from_bytes(bytes).unwrap();
        let data = package.manifest.effects[0].shader_data.as_ref().unwrap();
        assert_eq!(data.overflow.is_some(), overflow);
        let mut values = vec![];
        if let Some(texture) = &data.overflow {
            let pixels = image::load_from_memory(&package.files[&texture.path])
                .unwrap()
                .into_rgba8();
            values.extend(
                pixels
                    .as_raw()
                    .chunks_exact(4)
                    .take(texture.scalars as usize)
                    .map(|p| f32::from_le_bytes(p.try_into().unwrap())),
            );
        } else {
            values.extend(data.uniforms.iter().flatten().copied());
        }
        assert_eq!(
            &values[..4],
            &[0.1f32 * 0.5, 0.3f32 * 0.5, 0.8f32 * 0.5, 0.5]
        );
        for failure in 0..3 {
            let mut manifest = compiled.manifest.clone();
            match failure {
                0 => manifest.effects[0].shader_data.as_mut().unwrap().version += 1,
                1 => manifest.effects[0]
                    .required_capabilities
                    .retain(|c| c != motion_effects::shader_data::SHADER_DATA_CAPABILITY),
                _ => manifest.effects[0].shader_data.as_mut().unwrap().uniforms = vec![[1.; 4]; 33],
            }
            assert!(EffectPackage::from_bytes(compiled.package_bytes(&manifest).unwrap()).is_err());
        }
        EffectPackage::from_bytes(compiled.package_bytes(&compiled.manifest).unwrap()).unwrap();
        if overflow {
            use image::ImageEncoder;
            use std::io::Write;
            for wrong_dimensions in [false, true] {
                let (width, height) = if wrong_dimensions { (1, 4) } else { (256, 1) };
                let mut pixels = vec![0u8; width * height * 4];
                pixels[..4].copy_from_slice(&f32::NAN.to_le_bytes());
                let mut png = vec![];
                image::codecs::png::PngEncoder::new(&mut png)
                    .write_image(
                        &pixels,
                        width as u32,
                        height as u32,
                        image::ExtendedColorType::Rgba8,
                    )
                    .unwrap();
                let mut files = package.files.clone();
                let path = data.overflow.as_ref().unwrap().path.clone();
                files.insert(path, png);
                let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
                for (path, bytes) in files {
                    zip.start_file(path, zip::write::SimpleFileOptions::default())
                        .unwrap();
                    zip.write_all(&bytes).unwrap();
                }
                let error = EffectPackage::from_bytes(zip.finish().unwrap().into_inner())
                    .err()
                    .unwrap();
                assert!(error.to_string().contains("shader data"), "{error}");
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_uniform_and_byte_texture_data_match_baked_pixels_in_four_precisions() {
    use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
    use motion_effects::image_contract::{ChannelPrecision, ImageContract, ImageMeaning};
    use motion_render::{Renderer, Scene};
    let mut rows = vec![];
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let format = motion_render::image_formats::native_format(
            motion_render::image_formats::color_storage(precision),
        );
        let mut renderer = pollster::block_on(Renderer::new(&instance, None, format)).unwrap();
        let target = renderer
            .image_capture_target(
                16,
                16,
                ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Premultiplied,
                    },
                    precision,
                ),
            )
            .unwrap();
        for overflow in [false, true] {
            let mut previous = None;
            for color in [
                [0.125, 0.25, 0.75, 1.],
                [0.75, 0.5, 0.25, 1.],
                [0.1, 0.3, 0.8, 0.5],
            ] {
                let graph = color_graph(overflow, color);
                let mut oracle = None;
                for editable in [false, true] {
                    let compiled = if editable {
                        compile_editable(&graph, &Default::default())
                    } else {
                        compile(&graph, &Default::default())
                    }
                    .unwrap();
                    let package = Arc::new(
                        EffectPackage::from_bytes(
                            compiled.package_bytes(&compiled.manifest).unwrap(),
                        )
                        .unwrap(),
                    );
                    let mut registry = Registry::default();
                    registry.insert(package.clone()).unwrap();
                    renderer.set_effect_registry(registry);
                    let mut project = Project::new(16, 16, 30, 60).unwrap();
                    project.working_precision = precision;
                    project.background = [0.; 4];
                    let mut layer = Layer::solid(1, "Data", [16.; 2], [8., 8., 0.], [1.; 4]);
                    layer.effects.push(EffectInstance::new(
                        1,
                        &package.manifest.id,
                        &package.manifest.version,
                        &package.hash,
                        &package.manifest.effects[0],
                        [16.; 2],
                    ));
                    project.layers.push(layer);
                    project.rebuild_plugin_dependencies();
                    project.validate().unwrap();
                    let mut scene = Scene::new(&project).unwrap();
                    scene
                        .sample(&project, 0., None, &ExpressionEvaluator)
                        .unwrap();
                    renderer.draw(&scene, target.view(), 16, 16).unwrap();
                    let pixels = renderer.read_image_target(&target).unwrap().pixels;
                    assert!(pixels.iter().any(|v| *v != 0));
                    if editable {
                        assert_eq!(
                            &pixels,
                            oracle.as_ref().unwrap(),
                            "{precision:?} overflow={overflow} color={color:?}"
                        );
                        if let Some(previous) = &previous {
                            assert_ne!(
                                &pixels, previous,
                                "data update must change actual GPU pixels"
                            );
                        }
                        previous = Some(pixels);
                        rows.push(serde_json::json!({"precision":precision,"overflow":overflow,"color":color,"resource_bytes":compiled.report.package_resource_bytes,"shader_bytes":compiled.report.shader_bytes,"exact_baked_pixels":true}));
                    } else {
                        oracle = Some(pixels);
                    }
                }
            }
        }
    }
    if let Some(out) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            std::path::Path::new(&out).join("editable-data.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
