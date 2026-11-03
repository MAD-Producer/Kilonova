use super::*;
use crate::{image_resources::Resolution, test_allocations};
use motion_core::{Asset, Content, Layer};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn resident_scene_assets_reuse_storage_across_demand_changes() {
    for count in [1, 8, 64] {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("assets")).unwrap();
        let mut project = Project::new(16, 16, 30, 30).unwrap();
        for id in 1..=count {
            let path = format!("assets/image-{id}.png");
            image::save_buffer(
                root.path().join(&path),
                &[id as u8, 80, 170, 255].repeat(16 * 16),
                16,
                16,
                image::ColorType::Rgba8,
            )
            .unwrap();
            project.assets.push(Asset {
                id,
                path,
                width: 16,
                height: 16,
            });
            let mut layer = Layer::solid(id, "Resident image", [16.; 2], [8., 8., 0.], [1.; 4]);
            layer.content = Content::Image { asset: id };
            project.layers.push(layer);
        }
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        renderer.configure_assets(&project, root.path()).unwrap();
        let mut full = Scene::new(&project).unwrap();
        full.sample(&project, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        renderer
            .prepare_scene_assets(&full, Resolution::Full, false)
            .unwrap();
        let mut subset = project.clone();
        subset.layers.retain(|layer| layer.id % 2 == 1);
        let mut subset_scene = Scene::new(&subset).unwrap();
        subset_scene
            .sample(&subset, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let target = renderer.capture_target(16, 16).unwrap();
        let expected = renderer.capture(&full, &target).unwrap().0;
        renderer.set_image_prefetch(vec![count, count, 1]);
        for scene in [&subset_scene, &full] {
            renderer
                .prepare_scene_assets(scene, Resolution::Preview(32), true)
                .unwrap();
        }
        let before = (
            renderer.image_decodes,
            renderer.image_upload_bytes,
            renderer.texture_bytes(),
        );
        let ledger = renderer.texture_resource_ledger();
        for changing in [false, true] {
            let allocations = test_allocations::count(|| {
                for index in 0..120 {
                    let scene = if changing && index % 2 == 0 {
                        &subset_scene
                    } else {
                        &full
                    };
                    assert!(renderer
                        .prepare_scene_assets(scene, Resolution::Preview(32), true)
                        .unwrap());
                }
            });
            println!("resident scene assets={count} changing={changing} calls=120 allocations={allocations}");
            assert_eq!(allocations, 0, "resident demand must reuse its storage");
            assert_eq!(
                (
                    renderer.image_decodes,
                    renderer.image_upload_bytes,
                    renderer.texture_bytes()
                ),
                before
            );
            assert_eq!(renderer.texture_resource_ledger(), ledger);
            assert_eq!(renderer.capture(&full, &target).unwrap().0, expected);
        }
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn scene_asset_storage_survives_refusal_pending_and_decode_errors() {
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    image::save_buffer(
        root.path().join("assets/image.png"),
        &[40, 80, 170, 255],
        1,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut project = Project::new(16, 16, 30, 30).unwrap();
    project.assets.push(Asset {
        id: 7,
        path: "assets/image.png".into(),
        width: 1,
        height: 1,
    });
    let mut layer = Layer::solid(1, "Image", [16.; 2], [8., 8., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 7 };
    project.layers.push(layer);
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let budget = renderer.effect_gpu.builder.source_texture_budget;
    let ledger = renderer.texture_resource_ledger();
    renderer.effect_gpu.builder.source_texture_budget = 4;
    let error = renderer
        .prepare_scene_assets(&scene, Resolution::Full, false)
        .unwrap_err();
    assert!(error.to_string().contains("active image working set"));
    assert!(renderer.image_active.is_empty());
    assert_eq!(renderer.texture_resource_ledger(), ledger);
    assert_eq!(renderer.image_wanted_scratch, [7]);
    let capacity = renderer.image_wanted_scratch.capacity();
    renderer.effect_gpu.builder.source_texture_budget = budget;
    scene.layers[0].asset = Some(99);
    let error = renderer
        .prepare_scene_assets(&scene, Resolution::Full, false)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("image asset 99 is not registered"));
    assert!(renderer.image_active.is_empty());
    assert_eq!(renderer.image_wanted_scratch.capacity(), capacity);
    scene.layers[0].asset = Some(7);
    std::fs::rename(
        root.path().join("assets/image.png"),
        root.path().join("assets/held.png"),
    )
    .unwrap();
    let error = renderer
        .prepare_scene_assets(&scene, Resolution::Full, false)
        .unwrap_err();
    assert!(error.to_string().contains("image 7"));
    assert_eq!(renderer.image_wanted_scratch.capacity(), capacity);
    assert_eq!(renderer.texture_resource_ledger(), ledger);
    std::fs::rename(
        root.path().join("assets/held.png"),
        root.path().join("assets/image.png"),
    )
    .unwrap();
    assert!(!renderer
        .prepare_scene_assets(&scene, Resolution::Full, true)
        .unwrap());
    assert_eq!(renderer.image_wanted_scratch.capacity(), capacity);
    assert_eq!(renderer.image_active, [7]);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !renderer
        .prepare_scene_assets(&scene, Resolution::Full, true)
        .unwrap()
    {
        assert!(
            Instant::now() < deadline,
            "native image decode did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(renderer.image_decodes, 1);
    assert_eq!(renderer.image_dimensions(7), Some((1, 1)));
    renderer.clear_assets();
    assert_eq!(renderer.image_active.capacity(), 0);
    assert_eq!(renderer.image_wanted_scratch.capacity(), 0);
    renderer.check_health().unwrap();
}
