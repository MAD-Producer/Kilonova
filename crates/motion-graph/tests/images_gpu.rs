use motion_core::{EffectInstance, Layer, Project};
use motion_effects::{EffectPackage, Registry};
use motion_graph::*;
use motion_render::{Renderer, Scene};
use std::sync::Arc;
#[path = "support/image_fixture.rs"]
mod fixture;

fn render(compiled: CompiledGraph) -> Vec<u8> {
    let package = Arc::new(
        EffectPackage::from_bytes(compiled.package_bytes(&compiled.manifest).unwrap()).unwrap(),
    );
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.background = [0.; 4];
    let mut layer = Layer::solid(1, "image", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [64.; 2],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(registry);
    let target = renderer.capture_target(64, 64).unwrap();
    renderer.capture(&scene, &target).unwrap().0
}
fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 64 + x) * 4..(y * 64 + x + 1) * 4]
        .try_into()
        .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn portable_baked_editable_and_image_preview_match_independent_bilinear_pixels() {
    let graph = fixture::graph(fixture::image());
    let reopened = GraphDocument::from_json(&graph.to_json().unwrap()).unwrap();
    let options = CompileOptions::default();
    let baked = render(compile(&reopened, &options).unwrap());
    assert_eq!(
        baked,
        render(compile_editable(&reopened, &options).unwrap())
    );
    assert_eq!(
        baked,
        render(compile_preview(&reopened, &options, &PortRef::new("image", "image")).unwrap())
    );
    let corners = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [1., 1., 1.]];
    for y in 0..64 {
        for x in 0..64 {
            let u = ((x as f64 + 0.5) / 32. - 0.5).clamp(0., 1.);
            let v = ((y as f64 + 0.5) / 32. - 0.5).clamp(0., 1.);
            let actual = pixel(&baked, x, y);
            for c in 0..3 {
                let value = corners[0][c] * (1. - u) * (1. - v)
                    + corners[1][c] * u * (1. - v)
                    + corners[2][c] * (1. - u) * v
                    + corners[3][c] * u * v;
                let expected = (value * 255.).round() as u8;
                assert!(
                    actual[c].abs_diff(expected) <= 2,
                    "({x},{y}) {actual:?} channel={c} expected={expected}"
                );
            }
            assert_eq!(actual[3], 255);
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn image_pixels_use_asset_dimensions_and_transparent_color_cannot_bleed() {
    let mut graph = fixture::graph(fixture::image());
    graph
        .nodes
        .iter_mut()
        .find(|node| node.id == "sample")
        .unwrap()
        .operation = NodeOp::SampleImage {
        coordinates: SampleCoordinates::Pixels,
    };
    graph
        .nodes
        .iter_mut()
        .find(|node| node.id == "uv")
        .unwrap()
        .operation = NodeOp::Vec2 { value: [0.5; 2] };
    let pixels = render(compile(&graph, &CompileOptions::default()).unwrap());
    assert!(
        pixels
            .chunks_exact(4)
            .all(|p| p[0] > 253 && p[1] < 2 && p[2] < 2 && p[3] == 255)
    );
    let colored =
        GraphImage::from_png(fixture::png(&[255, 0, 0, 0, 0, 255, 0, 255], 2, 1)).unwrap();
    let clear = GraphImage::from_png(fixture::png(&[0, 0, 0, 0, 0, 255, 0, 255], 2, 1)).unwrap();
    let original = render(compile(&fixture::graph(colored), &CompileOptions::default()).unwrap());
    let control = render(compile(&fixture::graph(clear), &CompileOptions::default()).unwrap());
    assert_eq!(
        original, control,
        "hidden RGB in transparent texels must not contaminate filtered pixels"
    );
    let center = pixel(&original, 32, 32);
    assert_eq!(center[0], 0);
    assert!(center[1] > 0 && center[3] > 0 && center[3] < 255);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn all_four_resources_and_numeric_overflow_are_uploaded_in_the_declared_order() {
    let options = CompileOptions::default();
    let graph = fixture::many_images(4);
    for compiled in [
        compile(&graph, &options).unwrap(),
        compile_editable(&graph, &options).unwrap(),
    ] {
        let pixels = render(compiled);
        assert!(pixels.chunks_exact(4).all(|p| p[0].abs_diff(3) <= 2
            && p[1].abs_diff(100) <= 2
            && p[2].abs_diff(200) <= 2
            && p[3] == 255));
    }
    let mut graph = fixture::many_images(3);
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
    let compiled = compile_editable(&graph, &options).unwrap();
    assert_eq!(compiled.report.package_resource_textures, 4);
    assert!(
        compiled.manifest.effects[0]
            .shader_data
            .as_ref()
            .unwrap()
            .overflow
            .is_some()
    );
    let pixels = render(compiled);
    assert!(pixels.chunks_exact(4).all(|p| p[0].abs_diff(2) <= 2
        && p[1].abs_diff(100) <= 2
        && p[2].abs_diff(200) <= 2
        && p[3] == 255));
}
