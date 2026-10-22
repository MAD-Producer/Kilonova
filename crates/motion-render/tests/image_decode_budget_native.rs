use motion_core::{Asset, Content, Layer, Project};
use motion_render::{
    Renderer, ResourceBudgetScope, ResourceRejection, Scene, image_resources::Resolution,
};

#[test]
fn actual_cpu_refusal_preserves_target_and_recovers_proxy_then_original_pixels() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    image::save_buffer(
        root.path().join("assets/image.png"),
        &[
            255, 0, 0, 255, 255, 255, 255, 0, 0, 0, 255, 255, 0, 0, 255, 255,
        ],
        4,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut project = Project::new(64, 64, 30, 6).unwrap();
    project.assets.push(Asset {
        id: 17,
        path: "assets/image.png".into(),
        width: 4,
        height: 1,
    });
    let mut layer = Layer::solid(1, "image", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 17 };
    project.layers.push(layer);
    let before = project.clone();
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    renderer
        .prepare_scene_assets(&scene, Resolution::Full, false)
        .unwrap();
    renderer.draw(&scene, &target.view, 64, 64).unwrap();
    let full = renderer.read_target(&target).unwrap();
    assert!(full.iter().any(|byte| *byte != 0));
    renderer.clear_assets();
    let mut policy = renderer.memory_policy();
    let gpu_limit = policy.source_texture_budget_bytes;
    policy.decoded_image_budget_bytes = 8;
    renderer.set_memory_policy(policy).unwrap();
    let error = renderer
        .prepare_scene_assets(&scene, Resolution::Full, false)
        .unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::DecodedImages,
            required_bytes: 16,
            budget_bytes: 8,
        })
    );
    assert!(error.to_string().contains("image 17"));
    assert_eq!(renderer.texture_bytes(), 4);
    assert_eq!(renderer.read_target(&target).unwrap(), full);
    assert!(
        renderer
            .prepare_scene_assets(&scene, Resolution::Preview(2), false)
            .unwrap()
    );
    assert_eq!(renderer.texture_bytes(), 12); // One solid pixel + two proxy pixels.
    renderer.draw(&scene, &target.view, 64, 64).unwrap();

    policy.decoded_image_budget_bytes = 16;
    renderer.set_memory_policy(policy).unwrap();
    assert!(
        renderer
            .prepare_scene_assets(&scene, Resolution::Full, false)
            .unwrap()
    );
    renderer.draw(&scene, &target.view, 64, 64).unwrap();
    assert_eq!(renderer.read_target(&target).unwrap(), full);
    assert_eq!(
        renderer.memory_policy().source_texture_budget_bytes,
        gpu_limit
    );
    assert_eq!(project, before);

    // A real decoder failure must remain a decoder failure after policy update.
    renderer.clear_assets();
    let mut corrupt = std::fs::read(root.path().join("assets/image.png")).unwrap();
    let idat = corrupt
        .windows(4)
        .position(|bytes| bytes == b"IDAT")
        .unwrap();
    let size = u32::from_be_bytes(corrupt[idat - 4..idat].try_into().unwrap()) as usize;
    corrupt[idat + 4 + size] ^= 1;
    std::fs::write(root.path().join("assets/image.png"), corrupt).unwrap();
    let error = renderer
        .prepare_scene_assets(&scene, Resolution::Full, false)
        .unwrap_err();
    assert!(
        error.resource_rejection().is_none(),
        "corrupt PNG became pressure: {error:?}"
    );
    assert_eq!(renderer.read_target(&target).unwrap(), full);
}
