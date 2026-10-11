use motion_core::{Ease, EffectInstance, Layer, Project};
use motion_effects::{AlphaMode, EffectPackage, Registry, WorkingSpace};
use motion_graph::*;
use motion_render::Scene;
use motion_render::{CaptureTarget, Renderer};
use std::sync::Arc;

fn setup(graph: &GraphDocument, path: &std::path::Path) -> (Project, Renderer, CaptureTarget) {
    let report = export_msfx(
        graph,
        &CompileOptions {
            plugin_id: format!("test.graph.{}", graph.id),
            ..CompileOptions::default()
        },
        path,
    )
    .unwrap();
    let package = Arc::new(EffectPackage::open(&report.path).unwrap());
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    let mut p = Project::new(64, 64, 30, 60).unwrap();
    p.background = [0.0; 4];
    let mut layer = Layer::solid(1, "graph", [64.0; 2], [32.0, 32.0, 0.0], [1.0; 4]);
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [64.0; 2],
    ));
    p.layers.push(layer);
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(registry);
    let target = renderer.capture_target(64, 64).unwrap();
    (p, renderer, target)
}
fn render(p: &Project, r: &mut Renderer, t: &CaptureTarget, frame: f64) -> Vec<u8> {
    let mut scene = Scene::new(p);
    scene
        .sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    r.capture(&scene, t).unwrap().0
}
fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 64 + x) * 4..(y * 64 + x + 1) * 4]
        .try_into()
        .unwrap()
}

#[test]
fn style_examples_render_animate_and_accept_public_colors_through_exported_packages() {
    let dir = tempfile::tempdir().unwrap();
    for example in builtin_examples()
        .iter()
        .filter(|example| example.collection != ExampleCollection::Fundamentals)
    {
        let graph = example.load().unwrap();
        let (mut project, mut renderer, target) =
            setup(&graph, &dir.path().join(format!("{}.msfx", example.id)));
        let first = render(&project, &mut renderer, &target, 0.0);
        let later = render(&project, &mut renderer, &target, 15.5);
        // Each style example generates its entire opaque background. A failed
        // pass or unchanged clock would otherwise look like the input solid.
        assert!(first.chunks_exact(4).all(|p| p[3] == 255), "{}", example.id);
        let changed_pixels = first
            .chunks_exact(4)
            .zip(later.chunks_exact(4))
            .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(x, y)| x.abs_diff(*y) > 2))
            .count();
        assert!(
            changed_pixels > 8,
            "{} did not animate: {changed_pixels}",
            example.id
        );
        let tone = graph
            .public_parameters
            .iter()
            .find(|p| p.value_type == ValueType::Color)
            .unwrap();
        project.layers[0].effects[0]
            .params
            .get_mut(&tone.id)
            .unwrap()
            .track
            .value = [0.8, 0.15, 0.3, 1.0];
        let recolored = render(&project, &mut renderer, &target, 15.5);
        let changed_pixels = later
            .chunks_exact(4)
            .zip(recolored.chunks_exact(4))
            .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(x, y)| x.abs_diff(*y) > 8))
            .count();
        assert!(
            changed_pixels > 16,
            "{} ignored public color: {changed_pixels}",
            example.id
        );
    }
}
#[test]
fn exported_sdf_pixels_preserve_alpha_fractional_time_and_animated_public_slots() {
    let dir = tempfile::tempdir().unwrap();
    let graph = demo_graph();
    let (mut p, mut renderer, target) = setup(&graph, &dir.path().join("circle.msfx"));
    let first = render(&p, &mut renderer, &target, 0.0);
    let center = pixel(&first, 32, 32);
    assert!(
        center[0].abs_diff(26) < 3
            && center[1].abs_diff(179) < 3
            && center[2] > 250
            && center[3].abs_diff(166) < 2,
        "center={center:?}"
    );
    assert_eq!(pixel(&first, 0, 0)[3], 0);
    let edge = pixel(&first, 43, 43)[3];
    assert!(edge > 0 && edge < 166, "AA boundary alpha={edge}");
    let later = render(&p, &mut renderer, &target, 15.5);
    assert!(
        pixel(&first, 19, 32)[3] > 160 && pixel(&later, 19, 32)[3] < 5,
        "fractional seconds must move the circle"
    );
    assert!(pixel(&later, 51, 32)[3] > 160);
    let params = &mut p.layers[0].effects[0].params;
    params.get_mut("speed").unwrap().track.value[0] = 0.0;
    params
        .get_mut("radius")
        .unwrap()
        .track
        .upsert(0, [8.0, 0.0, 0.0, 0.0], Ease::Linear)
        .unwrap();
    params
        .get_mut("radius")
        .unwrap()
        .track
        .upsert(30, [24.0, 0.0, 0.0, 0.0], Ease::Linear)
        .unwrap();
    let small = render(&p, &mut renderer, &target, 0.0);
    let interpolated = render(&p, &mut renderer, &target, 15.5);
    assert_eq!(pixel(&small, 46, 32)[3], 0);
    assert!(pixel(&interpolated, 46, 32)[3] > 160);
    let fractional_edge = pixel(&interpolated, 48, 32)[3];
    assert!(
        fractional_edge > 5 && fractional_edge < 100,
        "fractional public radius alpha={fractional_edge}"
    );
    p.layers[0].effects[0]
        .params
        .get_mut("color")
        .unwrap()
        .track
        .value = [0.75, 0.1, 0.4, 0.3];
    let changed = render(&p, &mut renderer, &target, 15.5);
    let center = pixel(&changed, 32, 32);
    assert!(
        center[0].abs_diff(191) < 3
            && center[1].abs_diff(26) < 3
            && center[2].abs_diff(102) < 3
            && center[3].abs_diff(77) < 2,
        "public color={center:?}"
    );
}
#[test]
fn straight_and_premultiplied_graph_contracts_produce_the_same_visible_pixels() {
    let dir = tempfile::tempdir().unwrap();
    let graph = demo_graph();
    let (p, mut r, t) = setup(&graph, &dir.path().join("straight.msfx"));
    let straight = render(&p, &mut r, &t, 0.0);
    let mut graph = graph;
    graph.id = "premultiplied".into();
    graph.alpha_mode = AlphaMode::Premultiplied;
    let (p, mut r, t) = setup(&graph, &dir.path().join("premult.msfx"));
    let premult = render(&p, &mut r, &t, 0.0);
    for point in [(32, 32), (43, 43), (0, 0)] {
        let a = pixel(&straight, point.0, point.1);
        let b = pixel(&premult, point.0, point.1);
        assert!(a[3].abs_diff(b[3]) <= 1, "alpha {point:?}: {a:?} != {b:?}");
        let premult_linear = |channel: u8, alpha: u8| {
            let srgb = f32::from(channel) / 255.0;
            let linear = if srgb <= 0.04045 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            };
            linear * f32::from(alpha)
        };
        // RGBA8 premultiplication magnifies unassociated RGB quantization at
        // small alpha. Compare the visible premultiplied linear pixel instead.
        for channel in 0..3 {
            assert!(
                (premult_linear(a[channel], a[3]) - premult_linear(b[channel], b[3])).abs() <= 2.0,
                "visible pixel {point:?}: {a:?} != {b:?}"
            );
        }
    }
}
#[test]
fn frame_index_and_seconds_remain_fractional_in_the_actual_gpu_uniforms() {
    let dir = tempfile::tempdir().unwrap();
    let graph = GraphDocument {
        format_version: 1,
        id: "clock".into(),
        name: "时钟".into(),
        english_name: "Clock".into(),
        working_space: WorkingSpace::Srgb,
        alpha_mode: AlphaMode::Straight,
        public_parameters: vec![],
        nodes: vec![
            Node::new("time", NodeOp::Time),
            Node::new("frame", NodeOp::FrameIndex),
            Node::new("fps", NodeOp::Float { value: 30.0 }),
            Node::new(
                "div",
                NodeOp::Math {
                    operation: MathOp::Divide,
                    value_type: ValueType::Float,
                },
            ),
            Node::new("zero", NodeOp::Float { value: 0.0 }),
            Node::new("one", NodeOp::Float { value: 1.0 }),
            Node::new("color", NodeOp::CombineColor),
            Node::new("out", NodeOp::ColorOutput),
        ],
        connections: vec![
            Connection::new("time", "value", "color", "r"),
            Connection::new("frame", "value", "div", "a"),
            Connection::new("fps", "value", "div", "b"),
            Connection::new("div", "value", "color", "g"),
            Connection::new("zero", "value", "color", "b"),
            Connection::new("one", "value", "color", "a"),
            Connection::new("color", "color", "out", "color"),
        ],
    };
    let (p, mut r, t) = setup(&graph, &dir.path().join("clock.msfx"));
    let result = render(&p, &mut r, &t, 15.5);
    let value = pixel(&result, 32, 32);
    assert!(
        value[0].abs_diff(132) <= 1 && value[1].abs_diff(132) <= 1,
        "fractional clock={value:?}"
    );
}

#[test]
fn sdk6_last_public_slot_is_rendered_and_the_33rd_parameter_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut graph = demo_graph();
    while graph.public_parameters.len() < 31 {
        let id = format!("param{}", graph.public_parameters.len());
        graph.public_parameters.push(PublicParameter {
            id: id.clone(),
            name: id.clone(),
            english_name: id,
            value_type: ValueType::Float,
            default: [0.0; 4],
            min: 0.0,
            max: 1.0,
            step: 0.1,
            units: String::new(),
            animatable: true,
        });
    }
    graph.public_parameters.push(PublicParameter {
        id: "last_color".into(),
        name: "最后颜色".into(),
        english_name: "Last Color".into(),
        value_type: ValueType::Color,
        default: [0.2, 0.4, 0.6, 0.8],
        min: 0.0,
        max: 1.0,
        step: 0.01,
        units: String::new(),
        animatable: true,
    });
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "color")
        .unwrap()
        .operation = NodeOp::Parameter {
        parameter: "last_color".into(),
    };
    let compiled = compile(&graph, &CompileOptions::default()).unwrap();
    assert_eq!(compiled.report.parameter_slots["last_color"], 31);
    assert!(compiled.source.contains("fx.params[31]"));
    let (p, mut r, t) = setup(&graph, &dir.path().join("slot31.msfx"));
    let pixels = render(&p, &mut r, &t, 0.0);
    let c = pixel(&pixels, 32, 32);
    assert!(
        c[0].abs_diff(51) <= 2
            && c[1].abs_diff(102) <= 2
            && c[2].abs_diff(153) <= 2
            && c[3].abs_diff(204) <= 1,
        "slot 31={c:?}"
    );
    graph.public_parameters.push(PublicParameter {
        id: "too_many".into(),
        name: "过多".into(),
        english_name: "Too Many".into(),
        value_type: ValueType::Float,
        default: [0.0; 4],
        min: 0.0,
        max: 1.0,
        step: 0.1,
        units: String::new(),
        animatable: true,
    });
    assert_eq!(
        compile(&graph, &CompileOptions::default())
            .err()
            .unwrap()
            .diagnostic()
            .code,
        ErrorCode::Budget
    );
}
