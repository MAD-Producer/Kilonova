//! An actual compiled, project-owned node package through the public host API.
use motion_core::{EffectInstance, FrameRate, Layer, LayerTimeline, NodeResource, Project};
use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};
use motion_effects::{AlphaMode, EffectPackage, WorkingSpace};
use motion_graph::{Connection, GraphDocument, MathOp, Node, NodeOp, PublicParameter, ValueType};
use motion_host::Session;
use motion_render::{image_resources::Resolution, OutputTransform, Renderer};
use serde_json::{json, Value};
use std::{fs, path::Path};

const SIZE: u32 = 32;

fn graph() -> GraphDocument {
    let parameters = [
        ("gain", ValueType::Float, [0.2, 0., 0., 0.]),
        ("vector", ValueType::Vec3, [0.1, 0.2, 0.3, 0.]),
        ("tint", ValueType::Color, [0.8, 0.4, 0.2, 1.]),
        ("enabled", ValueType::Bool, [1., 0., 0., 0.]),
    ];
    let mut nodes: Vec<_> = parameters
        .iter()
        .map(|(id, _, _)| {
            Node::new(
                *id,
                NodeOp::Parameter {
                    parameter: (*id).into(),
                },
            )
        })
        .collect();
    nodes.extend([
        Node::new(
            "weights",
            NodeOp::Vec3 {
                value: [0.2, 0.3, 0.5],
            },
        ),
        Node::new(
            "dot",
            NodeOp::Math {
                operation: MathOp::Dot,
                value_type: ValueType::Vec3,
            },
        ),
        Node::new(
            "multiply",
            NodeOp::Math {
                operation: MathOp::Multiply,
                value_type: ValueType::Float,
            },
        ),
        Node::new("one", NodeOp::Float { value: 1. }),
        Node::new("gray", NodeOp::CombineColor),
        Node::new(
            "tinted",
            NodeOp::ColorMath {
                operation: motion_graph::ColorOp::Multiply,
            },
        ),
        Node::new(
            "black",
            NodeOp::Color {
                value: [0., 0., 0., 1.],
            },
        ),
        Node::new(
            "select",
            NodeOp::Select {
                value_type: ValueType::Color,
            },
        ),
        Node::new("output", NodeOp::ColorOutput),
    ]);
    let mut connections = vec![
        Connection::new("vector", "value", "dot", "a"),
        Connection::new("weights", "value", "dot", "b"),
        Connection::new("dot", "value", "multiply", "a"),
        Connection::new("gain", "value", "multiply", "b"),
        Connection::new("one", "value", "gray", "a"),
        Connection::new("tint", "value", "tinted", "a"),
        Connection::new("gray", "color", "tinted", "b"),
        Connection::new("enabled", "value", "select", "condition"),
        Connection::new("tinted", "color", "select", "true_value"),
        Connection::new("black", "value", "select", "false_value"),
        Connection::new("select", "value", "output", "color"),
    ];
    for channel in ["r", "g", "b"] {
        connections.push(Connection::new("multiply", "value", "gray", channel));
    }
    GraphDocument {
        format_version: motion_graph::GRAPH_FORMAT_VERSION,
        id: "parameter_contract".into(),
        name: "Parameter contract".into(),
        english_name: "Parameter contract".into(),
        working_space: WorkingSpace::Srgb,
        alpha_mode: AlphaMode::Straight,
        nodes,
        connections,
        public_parameters: parameters
            .into_iter()
            .map(|(id, value_type, default)| PublicParameter {
                id: id.into(),
                name: id.into(),
                english_name: id.into(),
                value_type,
                default,
                min: 0.,
                max: 1.,
                step: 0.01,
                units: String::new(),
                animatable: true,
            })
            .collect(),
        images: Default::default(),
    }
}

fn setup(root: &Path) -> (Session, Vec<u8>) {
    let document = graph();
    let options = motion_graph::CompileOptions {
        plugin_id: "local.motionstudio.parameter-contract".into(),
        ..Default::default()
    };
    let compiled = motion_graph::compile(&document, &options).unwrap();
    let bytes = compiled.package_bytes(&compiled.manifest).unwrap();
    let package = EffectPackage::from_bytes(bytes.clone()).unwrap();
    let hash = motion_core::storage::resource_sha256(&bytes);
    let resource = NodeResource {
        path: format!("assets/node-{hash}.msfx"),
        bytes: bytes.len() as u64,
        sha256: hash,
    };
    let mut stage = motion_core::storage::NodeResourceStage::new(root).unwrap();
    stage.stage(&resource, &bytes).unwrap();
    let project =
        Project::new_with_frame_rate(SIZE, SIZE, FrameRate::new(30000, 1001).unwrap(), 40).unwrap();
    let mut session = Session::new(project, root.into(), motion_desktop_media::platform()).unwrap();
    let mut layer = Layer::solid(
        7,
        "Compiled node parameters",
        [SIZE as f32; 2],
        [SIZE as f32 / 2., SIZE as f32 / 2., 0.],
        [1.; 4],
    );
    layer.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 30,
        offset_frame: 10,
    });
    let mut effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        layer.size,
    );
    effect.node_graph = Some(
        json!({"format_version":motion_core::node_resources::NODE_SOURCE_VERSION,
        "key":"parameter-contract", "document":document, "options":options, "runtime":resource}),
    );
    session
        .apply_commands(
            &json!([
                {"op":"add","layer":layer},
                {"op":"effect","object":7,"action":{"kind":"insert","instance":effect}}
            ])
            .to_string(),
        )
        .unwrap();
    stage.keep_resources();
    // No manual registry installation: the host must restore the actual runtime.
    session
        .effects
        .registry
        .resolve(
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
        )
        .unwrap();
    (session, bytes)
}

fn render(session: &mut Session, renderer: &mut Renderer, frame: f64) -> Vec<u8> {
    session.frame = frame;
    session.sample().unwrap();
    renderer.set_effect_registry(session.effects.registry.clone());
    renderer
        .configure_assets_with_scene(session.engine.project(), &session.root, &session.scene)
        .unwrap();
    assert!(renderer
        .prepare_scene_assets(&session.scene, Resolution::Full, false)
        .unwrap());
    let target = renderer
        .sdr_capture_target(SIZE, SIZE, OutputTransform::SDR_CLIP)
        .unwrap();
    renderer
        .capture_sdr(&session.scene, &target)
        .unwrap()
        .0
        .pixels
}

fn set(param: &str, value: [f32; 4]) -> Value {
    json!({"op":"effect","object":7,"action":{"kind":"set","effect":1,"param":param,"frame":20,"value":value}})
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn compiled_node_parameters_animate_render_reopen_and_freeze_with_owned_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    fs::create_dir(&root).unwrap();
    let (mut session, package) = setup(&root);
    let original = session.engine.snapshot();
    let contract = session.snapshot().unwrap()["effectParameterCapabilities"].clone();
    for (id, kind, interpolation, expressions) in [
        ("gain", "float", "numeric", true),
        ("vector", "vec3", "numeric", true),
        ("tint", "color", "numeric", true),
        ("enabled", "bool", "hold", false),
    ] {
        let actual = &contract[0]["parameters"][id];
        assert_eq!(actual["kind"], kind);
        assert_eq!(
            actual["animation"],
            json!({"keyframes":true,"expressions":expressions,"interpolation":interpolation})
        );
    }
    let mut commands: Vec<_> = ["gain", "vector", "tint", "enabled"].into_iter().map(|param|
        json!({"op":"effect","object":7,"action":{"kind":"animate","effect":1,"param":param,"frame":10,"enabled":true}})).collect();
    commands.extend([
        set("gain", [0.8, 0., 0., 0.]), set("vector", [0.3, 0.4, 0.5, 0.]),
        set("tint", [0.4, 0.8, 0.6, 1.]), set("enabled", [0., 0., 0., 0.]),
        json!({"op":"set_expression","frame":10,"expression":{"target":{"kind":"effect","object":7,"effect":1,"param":"vector"},
            "source":"value + [0.05, 0.1, 0.15]","enabled":true,"seed":0,"profile":motion_core::EXPRESSION_PROFILE}}),
    ]);
    session
        .apply_commands(&serde_json::to_string(&commands).unwrap())
        .unwrap();
    let authored = session.engine.snapshot();
    let revision = session.engine.revision();
    for param in authored.layers[0].effects[0].params.values() {
        assert_eq!(
            param
                .track
                .keys
                .iter()
                .map(|key| key.frame)
                .collect::<Vec<_>>(),
            vec![0, 10]
        );
    }
    let history = session.engine.history_diagnostics();
    let error = session
        .apply_commands(
            &json!([
                set("gain", [0.9, 0., 0., 0.]),
                set("enabled", [0.5, 0., 0., 0.])
            ])
            .to_string(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("integral"), "{error}");
    assert_eq!(session.engine.project(), &authored);
    assert_eq!(session.engine.history_diagnostics(), history);
    let error = session
        .apply_commands(
            &json!({"op":"set_expression","frame":10,
        "expression":{"target":{"kind":"effect","object":7,"effect":1,"param":"enabled"},
        "source":"value","enabled":true,"seed":0,"profile":motion_core::EXPRESSION_PROFILE}})
            .to_string(),
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("continuous effect parameter"),
        "{error}"
    );
    assert_eq!(session.engine.project(), &authored);
    assert_eq!(session.engine.history_diagnostics(), history);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let frames = [10., 15.5, 19.999, 20.];
    let mut samples = Vec::new();
    let mut plans = Vec::new();
    let mut states = Vec::new();
    for frame in frames {
        let pixels = render(&mut session, &mut renderer, frame);
        let state = session.snapshot().unwrap();
        let values = &state["sampledEffects"][0]["values"];
        let t = ((frame - 10.) / 10.).clamp(0., 1.) as f32;
        let gain = 0.2 + 0.6 * t;
        assert!((values["gain"][0].as_f64().unwrap() - f64::from(gain)).abs() < 0.00001);
        let vector = [0.15 + 0.2 * t, 0.3 + 0.2 * t, 0.45 + 0.2 * t];
        for (index, expected) in vector.into_iter().enumerate() {
            assert!(
                (values["vector"][index].as_f64().unwrap() - f64::from(expected)).abs() < 0.00001
            );
        }
        let factor = gain * (vector[0] * 0.2 + vector[1] * 0.3 + vector[2] * 0.5);
        let tint = [0.8 - 0.4 * t, 0.4 + 0.4 * t, 0.2 + 0.4 * t];
        for (index, expected) in tint.into_iter().enumerate() {
            assert!(
                (values["tint"][index].as_f64().unwrap() - f64::from(expected)).abs() < 0.00001
            );
        }
        let center = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
        let expected = tint.map(|value| {
            if frame < 20. {
                (value * factor * 255.).round() as u8
            } else {
                0
            }
        });
        for (actual, expected) in pixels[center..center + 3].iter().zip(expected) {
            assert!(
                actual.abs_diff(expected) <= 1,
                "independent numeric oracle at {frame}: {:?}, expected {expected}",
                &pixels[center..center + 4]
            );
        }
        assert_eq!(pixels[center + 3], 255);
        // This graph is spatially constant: independently check the entire
        // frame, not merely equality between two consumers of the same shader.
        for pixel in pixels.chunks_exact(4) {
            assert!(
                pixel[..3]
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| actual.abs_diff(expected) <= 1),
                "independent full-frame oracle at {frame}: {pixel:?}, expected {expected:?}"
            );
            assert_eq!(pixel[3], 255);
        }
        assert_eq!(values["enabled"][0], if frame < 20. { 1. } else { 0. });
        plans.push(
            FrameFreezePlan::capture(
                &session,
                Some(directory.path().join(format!("frozen-{frame}.png"))),
                &ExportControl::default(),
            )
            .unwrap(),
        );
        samples.push(pixels);
        states.push(state["sampledEffects"].clone());
    }
    assert_ne!(samples[0], samples[1]);
    assert_ne!(samples[2], samples[3]);
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &original);
    assert_ne!(render(&mut session, &mut renderer, 15.5), samples[1]);
    session.engine.redo().unwrap();
    assert_eq!(session.engine.project(), &authored);
    assert_eq!(render(&mut session, &mut renderer, 15.5), samples[1]);
    motion_core::storage::save(&root, &authored).unwrap();
    let mut reopened = Session::new(
        motion_core::storage::load(&root).unwrap(),
        root.clone(),
        motion_desktop_media::platform(),
    )
    .unwrap();
    assert_eq!(reopened.engine.project(), &authored);
    for (index, frame) in frames.into_iter().enumerate() {
        assert_eq!(render(&mut reopened, &mut renderer, frame), samples[index]);
        let state = reopened.snapshot().unwrap();
        assert_eq!(state["sampledEffects"], states[index]);
        assert_eq!(state["effectParameterCapabilities"], contract);
    }
    session
        .apply_commands(&json!({"op":"delete","object":7}).to_string())
        .unwrap();
    assert_ne!(render(&mut session, &mut renderer, 15.5), samples[1]);
    drop(reopened);
    drop(session);
    let output = std::env::var_os("MOTION_NODE_PARAMETER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        fs::create_dir_all(output).unwrap();
    }
    for (index, plan) in plans.into_iter().enumerate() {
        let result = plan.export(&ExportControl::default()).unwrap();
        assert_eq!(result["revision"], revision);
        let image = image::open(result["path"].as_str().unwrap())
            .unwrap()
            .into_rgba8();
        assert_eq!(image.dimensions(), (SIZE, SIZE));
        assert_eq!(image.as_raw(), &samples[index]);
        if let Some(output) = &output {
            image
                .save(output.join(format!("frozen-{}.png", frames[index])))
                .unwrap();
        }
    }
    if let Some(output) = &output {
        fs::write(
            output.join("initial-project.json"),
            serde_json::to_vec_pretty(&original).unwrap(),
        )
        .unwrap();
        fs::copy(root.join("project.json"), output.join("project.json")).unwrap();
        let resources = authored.node_resources().unwrap();
        for resource in &resources {
            fs::create_dir_all(output.join("assets")).unwrap();
            fs::copy(root.join(&resource.path), output.join(&resource.path)).unwrap();
        }
        fs::write(
            output.join("graph.msgraph"),
            serde_json::to_vec_pretty(&graph()).unwrap(),
        )
        .unwrap();
        fs::write(
            output.join("commands.json"),
            serde_json::to_vec_pretty(&commands).unwrap(),
        )
        .unwrap();
        fs::write(output.join("result.json"), serde_json::to_vec_pretty(&json!({
            "scope":"Compiled project-owned node runtime, public host parameters, independent full-frame numeric oracle, undo/redo, reopen and four frozen PNGs after live deletion/session drop",
            "limits":["Default 8-bit working precision only", "No desktop HWND capture", "No video or external resource input in this fixture"],
            "license":"MIT", "author":"Motion Studio repository contributors", "externalAssets":false,
            "frames":frames,"contract":contract,"sampledEffects":states,"resources":resources,"packageBytes":package.len()
        })).unwrap()).unwrap();
    }
}
