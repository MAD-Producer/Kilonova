use motion_core::{Asset, Content, EffectInstance, Layer, Project};
use motion_render::{
    effect_plan::{scratch_capacity_bytes, PlanBuilder},
    image_resources::Resolution,
    RenderError, Renderer, ResourceBudgetScope, ResourceRejection, Scene,
};

fn project(size: [f32; 2]) -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "gaussian_blur")
        .unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "blur", size, [32., 32., 0.], [1.; 4]);
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        size,
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project
}
fn sample(project: &Project) -> Scene {
    let mut scene = Scene::new(project);
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
fn typed_plan_preserves_messages_and_distinguishes_capacity_from_dimensions() {
    let mut builder =
        PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    let scene = sample(&project([3840., 2160.]));
    let error = builder
        .build(&scene, &[0], 64, 64, true)
        .err()
        .unwrap();
    let required = scratch_capacity_bytes(&builder.last_scratch_request.unwrap().sizes);
    assert_eq!(error.to_string(), format!("GPU validation: layer 1, effect 1 (gaussian_blur): effect scratch textures require {:.2} MiB; budget is 64 MiB (full layer bounds, including effect padding)", required as f64 / 1048576.));
    let rejected = ResourceRejection::Budget {
        scope: ResourceBudgetScope::EffectScratch,
        required_bytes: scratch_capacity_bytes(&builder.last_scratch_request.unwrap().sizes),
        budget_bytes: builder.scratch_budget(),
    };
    assert_eq!(error.resource_rejection(), Some(&rejected));
    let json = serde_json::to_value(rejected).unwrap();
    assert_eq!(json["kind"], "budget");
    assert_eq!(json["scope"], "effectScratch");
    assert_eq!(json["budgetBytes"], 64 << 20);
    builder.device_dimension = 32;
    let error = builder
        .build(&scene, &[0], 64, 64, true)
        .err()
        .unwrap();
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::TextureDimensions {
            max_dimension: 32,
            ..
        })
    ));
}

#[test]
fn diagnostics_success_cache_and_unrelated_errors_cannot_reuse_a_budget_refusal() {
    let registry = motion_effects::Registry::new_with_builtins().unwrap();
    let mut builder = PlanBuilder::new(registry.clone()).unwrap();
    let scene = sample(&project([3840., 2160.]));
    let error = builder
        .build(&scene, &[0], 64, 64, true)
        .err()
        .unwrap();
    let rejection = *error.resource_rejection().unwrap();
    let preview = builder.build_preview(&scene, &[0], 64, 64).unwrap();
    assert!(!preview.diagnostics.is_empty());
    assert_eq!(builder.last_resource_rejection, None);
    let small = sample(&project([64.; 2]));
    builder.build_preview(&small, &[0], 64, 64).unwrap();
    builder.last_resource_rejection = Some(rejection);
    builder.build_preview(&small, &[0], 64, 64).unwrap();
    assert_eq!(builder.preview_cache_hits, 1);
    assert_eq!(builder.last_resource_rejection, None);
    builder.last_resource_rejection = Some(rejection);
    let error = builder
        .build(&small, &[], 64, 64, true)
        .err()
        .unwrap();
    assert!(matches!(error, RenderError::Invalid(_)));
    assert_eq!(error.resource_rejection(), None);
    builder.last_resource_rejection = Some(rejection);
    builder.set_registry(registry);
    assert_eq!(builder.last_resource_rejection, None);
}

#[test]
fn active_images_refuse_before_upload_and_a_smaller_proxy_can_be_prepared() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let path = root.path().join("assets/source.png");
    image::save_buffer(
        &path,
        &vec![255; 2048 * 2048 * 4],
        2048,
        2048,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    for id in 1..=9 {
        project.assets.push(Asset {
            id,
            path: "assets/source.png".into(),
            width: 2048,
            height: 2048,
        });
        let mut layer = Layer::solid(id, "image", [64.; 2], [32., 32., 0.], [1.; 4]);
        layer.content = Content::Image { asset: id };
        project.layers.push(layer);
    }
    let scene = sample(&project);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let before = renderer.texture_memory_breakdown();
    let error = renderer
        .prepare_scene_assets(&scene, Resolution::Full, false)
        .unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::ActiveImages,
            required_bytes: 4 + 9 * 2048 * 2048 * 4,
            budget_bytes: 128 << 20,
        })
    );
    assert_eq!(before, renderer.texture_memory_breakdown());
    assert!(renderer
        .prepare_scene_assets(&scene, Resolution::Preview(64), false)
        .unwrap());
    for id in 1..=9 {
        assert_eq!(renderer.image_dimensions(id), Some((64, 64)));
    }
    let mut mismatched = project.clone();
    mismatched.assets[0].id = 10; // Exercise an uncached source's metadata check.
    mismatched.assets[0].width = 1024;
    let error = renderer
        .synchronize_assets(&mismatched, root.path())
        .unwrap_err();
    assert!(error.to_string().contains("texture budget"));
    assert!(error.resource_rejection().is_none());
    assert!(renderer
        .upload_image(10, 1, 1, &[])
        .unwrap_err()
        .resource_rejection()
        .is_none());
    let mut missing = scene.clone();
    missing.layers[0].asset = Some(999);
    assert!(renderer
        .prepare_scene_assets(&missing, Resolution::Full, false)
        .unwrap_err()
        .resource_rejection()
        .is_none());
    renderer.check_health().unwrap();
}
