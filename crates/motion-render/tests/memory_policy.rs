use motion_core::masks::LayerMask;
use motion_core::vector::{PathNode, VectorPath};
use motion_core::{Asset, Content, Layer, Project, Track};
use motion_render::effect_plan::PlanBuilder;
use motion_render::resource_policy::MemoryPolicy;
use motion_render::{Renderer, ResourceBudgetScope, ResourceRejection, Scene};

fn policy(source: u64) -> MemoryPolicy {
    MemoryPolicy {
        source_texture_budget_bytes: source,
        ..Default::default()
    }
}
fn scene(project: &Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
fn invalid_policy_is_atomic_and_mask_planning_uses_the_configured_budget() {
    let mut builder =
        PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    let before = builder.memory_policy();
    let invalid = MemoryPolicy {
        scratch_budget_bytes: 1,
        ..policy(16)
    };
    assert!(builder.set_memory_policy(invalid).is_err());
    assert_eq!(builder.memory_policy(), before);
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "mask", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.masks.push(LayerMask::new(
        1,
        VectorPath {
            id: 1,
            closed: true,
            nodes: [[0., 0.], [64., 0.], [64., 64.], [0., 64.]]
                .into_iter()
                .enumerate()
                .map(|(index, p)| PathNode {
                    id: index as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    ));
    project.layers.push(layer);
    let scene = scene(&project);
    builder.set_memory_policy(policy(1024)).unwrap();
    let error = builder
        .build(&scene, &[0], 64, 64, false)
        .unwrap_err();
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::Budget {
            scope: ResourceBudgetScope::MaskOutputs,
            budget_bytes: 1024,
            ..
        })
    ));
    builder.set_memory_policy(policy(16384)).unwrap();
    assert_eq!(
        builder
            .build(&scene, &[0], 64, 64, false)
            .unwrap()
            .masks
            .len(),
        1
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn lowering_budget_rechecks_resident_images_and_does_not_overwrite_the_target() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("assets")).unwrap();
    image::save_buffer(
        directory.path().join("assets/source.png"),
        &[255, 0, 0, 255].repeat(4),
        2,
        2,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.assets.push(Asset {
        id: 1,
        path: "assets/source.png".into(),
        width: 2,
        height: 2,
    });
    let mut layer = Layer::solid(1, "image", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 1 };
    project.layers.push(layer);
    let scene = scene(&project);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer
        .configure_assets(&project, directory.path())
        .unwrap();
    renderer
        .prepare_scene_assets(
            &scene,
            motion_render::image_resources::Resolution::Full,
            false,
        )
        .unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    renderer.draw(&scene, &target.view, 64, 64).unwrap();
    let pixels = renderer.read_target(&target).unwrap();
    renderer.set_memory_policy(policy(16)).unwrap();
    let error = renderer.draw(&scene, &target.view, 64, 64).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::SourceTextures,
            required_bytes: 20,
            budget_bytes: 16
        })
    );
    assert_eq!(renderer.read_target(&target).unwrap(), pixels);
    renderer.set_memory_policy(policy(20)).unwrap();
    renderer.draw(&scene, &target.view, 64, 64).unwrap();
    assert_eq!(renderer.read_target(&target).unwrap(), pixels);
    let old = renderer.memory_policy();
    assert!(renderer.set_memory_policy(policy(0)).is_err());
    assert_eq!(renderer.memory_policy(), old);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn configured_limits_apply_to_uploads_with_budget_details() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_memory_policy(policy(16)).unwrap();
    let error = renderer.upload_image(1, 2, 2, &[255; 16]).unwrap_err();
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::Budget {
            budget_bytes: 16,
            ..
        })
    ));
    assert_eq!(renderer.texture_bytes(), 4);
    renderer.set_memory_policy(policy(20)).unwrap();
    renderer.upload_image(1, 2, 2, &[255; 16]).unwrap();
    assert_eq!(renderer.texture_bytes(), 20);
}
