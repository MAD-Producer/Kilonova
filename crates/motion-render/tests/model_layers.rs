//! Actual composition renderer, not a standalone static-scene demo.
use motion_core::{Content, Layer, ModelAsset, ModelClip, Project};
use motion_render::{Renderer, Scene};
use motion_scene::{AssetResolver, ImportContext, ImportLimits, ResourceKind};
use sha2::{Digest, Sha256};
use std::io;

struct Material;
impl AssetResolver for Material {
    fn read(&mut self, path: &str, _: ResourceKind, _: usize) -> io::Result<Vec<u8>> {
        assert_eq!(path, "paint.mtl");
        Ok(b"newmtl Red\nKd 1 0 0\nillum 0\n".to_vec())
    }
}
fn project(root: &std::path::Path) -> Project {
    let source =
        b"mtllib paint.mtl\nv -0.8 -0.6 0\nv 0.7 -0.6 0\nv -0.4 0.8 0\nusemtl Red\nf 1 2 3\n";
    let imported = motion_scene::obj::import_obj(
        ImportContext {
            request: 8704,
            source: 7,
        },
        source,
        &mut Material,
        &Default::default(),
        &ImportLimits::default(),
    )
    .unwrap();
    let frozen = imported
        .freeze(
            source,
            motion_core::model_assets::MAX_MODEL_ASSET_BYTES as usize,
        )
        .unwrap();
    std::fs::create_dir_all(root.join("assets/models")).unwrap();
    std::fs::write(root.join("assets/models/triangle.mscn"), &frozen).unwrap();
    let mut project = Project::new(96, 64, 30, 61).unwrap();
    project.background = [0.; 4];
    project.model_assets.push(ModelAsset {
        id: 7,
        path: "assets/models/triangle.mscn".into(),
        bytes: frozen.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&frozen)),
    });
    let mut layer = Layer::solid(1, "Model triangle", [40.; 2], [35., 32., 0.], [1.; 4]);
    layer.three_d = true;
    layer.content = Content::Model {
        model: ModelClip {
            asset: 7,
            scene: 0,
            pixels_per_metre: 20.,
            pivot: [0.; 3],
            samples: 4,
        },
    };
    layer.transform.position.set_animated(0, true).unwrap();
    layer.transform.position.set_at(60, [65., 32., 0.]).unwrap();
    project.layers.push(layer);
    project.validate().unwrap();
    project
}
fn render(renderer: &mut Renderer, project: &Project, frame: f64) -> Vec<u8> {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.retain_video_instances(&scene);
    let target = renderer.capture_target(96, 64).unwrap();
    renderer.capture(&scene, &target).unwrap().0
}
fn center(pixels: &[u8]) -> f64 {
    let mut weight = 0.;
    let mut sum = 0.;
    for (i, p) in pixels.chunks_exact(4).enumerate() {
        weight += f64::from(p[3]);
        sum += (i % 96) as f64 * f64::from(p[3]);
    }
    assert!(weight > 50_000., "native model must cover actual pixels");
    sum / weight
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn model_layer_uses_saved_keyframes_and_has_no_source_io_during_frames() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path());
    motion_core::storage::save(root.path(), &project).unwrap();
    let restored = motion_core::storage::load(root.path()).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&restored, root.path()).unwrap();
    std::fs::remove_file(root.path().join(&restored.model_assets[0].path)).unwrap();
    let first = render(&mut renderer, &restored, 0.);
    let last = render(&mut renderer, &restored, 60.);
    assert!(
        (center(&last) - center(&first) - 30.).abs() < 0.25,
        "world position must not be applied twice"
    );
    assert!(
        first.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255),
        "explicit 4x MSAA must retain fractional edge coverage"
    );
    assert!(first
        .chunks_exact(4)
        .filter(|p| p[3] == 255)
        .all(|p| p[0] == 255 && p[1] == 0 && p[2] == 0));
    assert!(renderer.texture_memory_breakdown().model_sources > 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalid_model_catalog_does_not_replace_the_visible_owned_source() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let before = render(&mut renderer, &project, 0.);
    let mut corrupt = project.clone();
    corrupt.model_assets[0].sha256 = "0".repeat(64);
    assert!(renderer
        .configure_assets(&corrupt, root.path())
        .unwrap_err()
        .to_string()
        .contains("SHA-256"));
    assert_eq!(before, render(&mut renderer, &project, 0.));
    let mut absent = project.clone();
    if let Content::Model { model } = &mut absent.layers[0].content {
        model.scene = 99;
    }
    assert!(renderer.configure_assets(&absent, root.path()).is_err());
    assert_eq!(before, render(&mut renderer, &project, 0.));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn model_zero_scale_is_empty_and_deleted_layer_releases_its_raster() {
    let root = tempfile::tempdir().unwrap();
    let mut project = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    render(&mut renderer, &project, 0.);
    let before = renderer.texture_memory_breakdown().model_sources;
    project.layers[0].transform.scale.value = [0.; 3];
    assert!(render(&mut renderer, &project, 0.)
        .chunks_exact(4)
        .all(|p| p[3] == 0));
    project.layers.clear();
    render(&mut renderer, &project, 0.);
    assert!(
        renderer.texture_memory_breakdown().model_sources < before,
        "deleted model raster must retire"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn model_materials_obey_shared_source_budget_and_mesh_draws_are_reported() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let target = renderer.capture_target(96, 64).unwrap();
    let (before, stats) = renderer.capture(&scene, &target).unwrap();
    assert_eq!(
        stats.draw_calls, 2,
        "one mesh triangle and one composition raster draw"
    );
    assert!(
        stats.texture_bytes >= renderer.texture_memory_breakdown().model_sources,
        "model material and working-target textures must join the reported texture total"
    );
    let mut policy = renderer.memory_policy();
    let default_policy = policy;
    policy.source_texture_budget_bytes = 96 * 64 * 4 + 4;
    renderer.set_memory_policy(policy).unwrap();
    assert!(
        matches!(
            renderer.capture(&scene, &target).unwrap_err(),
            motion_render::RenderError::Resource {
                rejection: motion_render::ResourceRejection::Budget {
                    scope: motion_render::ResourceBudgetScope::SourceTextures,
                    ..
                },
                ..
            }
        ),
        "resident model material pixels must count beyond the raster and default white pixel"
    );
    renderer.set_memory_policy(default_policy).unwrap();
    assert_eq!(
        before,
        renderer.capture(&scene, &target).unwrap().0,
        "a pool refusal must preserve owned model resources and allow recovery"
    );

    let mut fresh = pollster::block_on(Renderer::headless()).unwrap();
    fresh.set_memory_policy(policy).unwrap();
    fresh.configure_assets(&project, root.path()).unwrap();
    let fresh_target = fresh.capture_target(96, 64).unwrap();
    assert!(
        fresh.capture(&scene, &fresh_target).is_err(),
        "first upload must apply the same shared source-pool allowance"
    );
    fresh.set_memory_policy(default_policy).unwrap();
    assert_eq!(before, fresh.capture(&scene, &fresh_target).unwrap().0);
}
