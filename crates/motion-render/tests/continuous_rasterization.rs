use motion_core::{render_sampling::LayerSampling, vector::*, *};
use motion_render::{effect_plan::PlanBuilder, Renderer, Scene};
use std::time::Instant;

fn fixture() -> Project {
    let mut p = Project::new(256, 256, 30, 60).unwrap();
    p.background = [0.; 4];
    p.version = 21;
    let mut layer = Layer::solid(1, "Authored circle", [24.; 2], [128., 128., 0.], [1.; 4]);
    layer.content = Content::Vector {
        vector: VectorContent::shape(ShapeKind::Ellipse),
    };
    layer.transform.scale.value = [800., 800., 100.];
    p.layers.push(layer);
    p
}
fn scene(project: &Project) -> Scene {
    let mut s = Scene::new(project);
    s.sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    s
}
fn capture(renderer: &mut Renderer, project: &Project) -> Vec<u8> {
    let target = renderer.capture_target(256, 256).unwrap();
    renderer.capture(&scene(project), &target).unwrap().0
}
fn edge_pixels(pixels: &[u8]) -> usize {
    pixels
        .chunks_exact(4)
        .filter(|p| p[3] > 0 && p[3] < 255)
        .count()
}

#[test]
fn projected_vector_raster_changes_real_pixels_and_records_source_cost_without_changing_alpha_area()
{
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = fixture();
    let started = Instant::now();
    let native = capture(&mut renderer, &p);
    let native_us = started.elapsed().as_micros();
    let native_bytes = renderer.texture_bytes();
    let mut native_warm_us = vec![];
    for _ in 0..3 {
        let started = Instant::now();
        assert_eq!(native, capture(&mut renderer, &p));
        native_warm_us.push(started.elapsed().as_micros());
    }
    p.layers[0].sampling.continuous_rasterization = true;
    let started = Instant::now();
    let continuous = capture(&mut renderer, &p);
    let continuous_us = started.elapsed().as_micros();
    let continuous_bytes = renderer.texture_bytes();
    let mut continuous_warm_us = vec![];
    for _ in 0..3 {
        let started = Instant::now();
        assert_eq!(continuous, capture(&mut renderer, &p));
        continuous_warm_us.push(started.elapsed().as_micros());
    }
    assert_ne!(native, continuous);
    assert!(
        edge_pixels(&continuous) * 2 < edge_pixels(&native),
        "native={} continuous={}",
        edge_pixels(&native),
        edge_pixels(&continuous)
    );
    let alpha = |p: &[u8]| p.chunks_exact(4).map(|p| u64::from(p[3])).sum::<u64>();
    assert!(alpha(&continuous).abs_diff(alpha(&native)) < alpha(&continuous) / 20);
    assert_eq!(&continuous[..4], &[0u8; 4]);
    assert!(continuous_bytes > native_bytes);
    assert_eq!(continuous, capture(&mut renderer, &p));
    let frozen: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(continuous, capture(&mut renderer, &frozen));
    p.layers[0].sampling = LayerSampling::default();
    assert_eq!(native, capture(&mut renderer, &p));
    if let Some(folder) = std::env::var_os("MOTION_SAMPLING_ARTIFACT_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        image::RgbaImage::from_raw(256, 256, native.clone())
            .unwrap()
            .save(folder.join("native-vector.png"))
            .unwrap();
        image::RgbaImage::from_raw(256, 256, continuous.clone())
            .unwrap()
            .save(folder.join("continuous-vector.png"))
            .unwrap();
        std::fs::write(folder.join("continuous-vector-cost.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "width":256,"height":256,"layerScale":8,"native":{"first_capture_us":native_us,"warm_capture_us":native_warm_us,"texture_bytes":native_bytes,"edge_pixels":edge_pixels(&native)},
            "continuous":{"first_capture_us":continuous_us,"warm_capture_us":continuous_warm_us,"texture_bytes":continuous_bytes,"edge_pixels":edge_pixels(&continuous)},
            "timingScope":"first capture plus three resident captures per mode; includes GPU readback; not steady-state presentation FPS"
        })).unwrap()).unwrap();
    }
}

#[test]
fn preview_and_formal_plans_share_density_and_resource_refusal_is_explicit() {
    let mut p = fixture();
    p.layers[0].sampling.continuous_rasterization = true;
    let mut builder = PlanBuilder::new(Default::default()).unwrap();
    let s = scene(&p);
    let native = builder.build(&s, &[0], 256, 256, false).unwrap().vectors[0].clone();
    let preview = builder.build_preview(&s, &[0], 256, 256).unwrap().vectors[0].clone();
    assert_eq!(
        (native.width, native.height),
        (preview.width, preview.height)
    );
    assert_eq!(native.fingerprint, preview.fingerprint);
    assert!(native.width >= 192);
    let mut same_scene = s.clone();
    same_scene.layers[0].sampling = Default::default();
    let low = builder
        .build_preview(&same_scene, &[0], 256, 256)
        .unwrap()
        .vectors[0]
        .width;
    // Change only the sampling flag: vector/mask Arcs and transforms stay identical.
    same_scene.layers[0].sampling.continuous_rasterization = true;
    let high = builder
        .build_preview(&same_scene, &[0], 256, 256)
        .unwrap()
        .vectors[0]
        .width;
    assert!(high >= low * 7);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut policy = renderer.memory_policy();
    policy.source_texture_budget_bytes = 4096;
    renderer.set_memory_policy(policy).unwrap();
    let target = renderer.capture_target(256, 256).unwrap();
    let error = renderer.capture(&s, &target).unwrap_err();
    assert!(error.resource_rejection().is_some(), "{error}");
    renderer.check_health().unwrap();
    policy.source_texture_budget_bytes = 128 * 1024 * 1024;
    renderer.set_memory_policy(policy).unwrap();
    renderer.capture(&s, &target).unwrap();
}
