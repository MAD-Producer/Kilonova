//! Actual composition renderer, not a standalone static-scene demo.
use motion_core::{Content, Layer, ModelAsset, ModelClip, Project};
use motion_render::{Renderer, Scene};
use motion_scene::{AssetResolver, ImportContext, ImportLimits, ResourceKind};
use sha2::{Digest, Sha256};
use std::io;

#[path = "model_layers/exposure.rs"]
mod exposure;
#[path = "model_layers/project_shared.rs"]
mod project_shared;
#[path = "model_layers/source_catalog.rs"]
mod source_catalog;
#[path = "model_layers/imported_cameras.rs"]
mod imported_cameras;

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
            animation: None,
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

fn animated_project(root: &std::path::Path) -> Project {
    animated_project_kind(root, false)
}
fn animated_project_kind(root: &std::path::Path, morph: bool) -> Project {
    animated_project_features(root, morph, false)
}
fn animated_project_features(root: &std::path::Path, morph: bool, skin: bool) -> Project {
    struct Bytes(Vec<u8>);
    impl AssetResolver for Bytes {
        fn read(&mut self, path: &str, _: ResourceKind, limit: usize) -> io::Result<Vec<u8>> {
            assert_eq!(path, "owned.bin");
            assert!(self.0.len() <= limit);
            Ok(self.0.clone())
        }
    }
    let mut data: Vec<u8> = [
        -0.8f32, -0.6, 0., 0.7, -0.6, 0., -0.4, 0.8, 0., 0., 2., 0., 0., 0., 0.5, 0., 0., 1., 1.,
        1., 0., 0., 0.,
    ]
    .into_iter()
    .flat_map(f32::to_le_bytes)
    .collect();
    let mut document = serde_json::json!({"asset":{"version":"2.0"},
        "buffers":[{"uri":"owned.bin","byteLength":data.len()}],
        "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},
            {"buffer":0,"byteOffset":36,"byteLength":8},{"buffer":0,"byteOffset":44,"byteLength":24},
            {"buffer":0,"byteOffset":68,"byteLength":24}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[-0.8,-0.6,0],"max":[0.7,0.8,0]},
            {"bufferView":1,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[2]},
            {"bufferView":2,"componentType":5126,"count":2,"type":"VEC3"},
            {"bufferView":3,"componentType":5126,"count":2,"type":"VEC3"}],
        "materials":[{"emissiveFactor":[1,0,0],"pbrMetallicRoughness":{"baseColorFactor":[0,0,0,1],"metallicFactor":0}}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
        "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0,
        "animations":[{"name":"Translate","channels":[{"sampler":0,"target":{"node":0,"path":"translation"}}],"samplers":[{"input":1,"output":2}]},
            {"name":"Hide","channels":[{"sampler":0,"target":{"node":0,"path":"scale"}}],"samplers":[{"input":1,"output":3}]}]});
    if morph {
        data[44..52].copy_from_slice(
            &[0.0f32, 1.]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let offset = data.len();
        data.extend(
            [0.5f32, 0., 0., 0.5, 0., 0., 0.5, 0., 0.]
                .into_iter()
                .flat_map(f32::to_le_bytes),
        );
        document["buffers"][0]["byteLength"] = serde_json::json!(data.len());
        document["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"buffer":0,"byteOffset":offset,"byteLength":36}));
        document["accessors"].as_array_mut().unwrap().push(serde_json::json!({"bufferView":4,"componentType":5126,"count":3,"type":"VEC3","min":[0.5,0,0],"max":[0.5,0,0]}));
        document["accessors"][2]["type"] = serde_json::json!("SCALAR");
        document["meshes"][0]["primitives"][0]["targets"] = serde_json::json!([{"POSITION":4}]);
        document["meshes"][0]["weights"] = serde_json::json!([0.]);
        document["animations"][0]["name"] = serde_json::json!("Displace");
        document["animations"][0]["channels"][0]["target"]["path"] = serde_json::json!("weights");
    }
    if skin {
        use serde_json::json;
        let offset = data.len();
        data.extend_from_slice(&[0u8, 1, 0, 0].repeat(3));
        let views = document["bufferViews"].as_array_mut().unwrap();
        let jv = views.len();
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":12}));
        let offset = data.len();
        data.extend(
            [0.25f32, 0.75, 0., 0.]
                .repeat(3)
                .iter()
                .flat_map(|v| v.to_le_bytes()),
        );
        let wv = views.len();
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":48}));
        let accessors = document["accessors"].as_array_mut().unwrap();
        let j = accessors.len();
        accessors.push(json!({"bufferView":jv,"componentType":5121,"count":3,"type":"VEC4"}));
        let w = accessors.len();
        accessors.push(json!({"bufferView":wv,"componentType":5126,"count":3,"type":"VEC4"}));
        document["meshes"][0]["primitives"][0]["attributes"]["JOINTS_0"] = json!(j);
        document["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_0"] = json!(w);
        document["buffers"][0]["byteLength"] = json!(data.len());
        document["skins"] = json!([{"joints":[1,2],"skeleton":1}]);
        document["nodes"] =
            json!([{"mesh":0,"skin":0,"translation":[100.,0.,0.]},{"children":[2]},{}]);
        document["scenes"][0]["nodes"] = json!([0, 1]);
        document["animations"][0]["channels"][0]["target"]["node"] = json!(1);
    }
    let source = serde_json::to_vec(&document).unwrap();
    let scene = motion_scene::import(
        ImportContext {
            request: 8708,
            source: 7,
        },
        &source,
        &mut Bytes(data),
        &ImportLimits::default(),
    )
    .unwrap();
    let bundle = scene.freeze(&source, 1 << 20).unwrap();
    let mut project = project(root);
    std::fs::write(root.join(&project.model_assets[0].path), &bundle).unwrap();
    project.model_assets[0].bytes = bundle.len() as u64;
    project.model_assets[0].sha256 = format!("{:x}", Sha256::digest(&bundle));
    let Content::Model { model } = &mut project.layers[0].content else {
        unreachable!()
    };
    model.animation = Some(motion_core::ModelAnimation {
        clip: 0,
        offset_seconds: 0.,
        speed: 1.,
    });
    project
}

#[test]
fn imported_clip_time_uses_layer_local_frames_without_rewriting_euler_keys() {
    let root = tempfile::tempdir().unwrap();
    let mut project = animated_project(root.path());
    project.frames = 120;
    project.layers[0].timeline = Some(motion_core::LayerTimeline {
        in_frame: 30,
        out_frame: 120,
        offset_frame: 30,
    });
    project.layers[0]
        .transform
        .rotation
        .set_animated(0, true)
        .unwrap();
    project.layers[0]
        .transform
        .rotation
        .set_at(60, [0., 0., 720.])
        .unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 45., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert_eq!(
        scene.layers[0].model_animation,
        Some(motion_scene::AnimationSample {
            animation: 0,
            seconds: 0.5
        })
    );
    assert_eq!(
        project.layers[0].transform.rotation.sample(60.).unwrap(),
        [0., 0., 720.]
    );
    let Content::Model { model } = &mut project.layers[0].content else {
        unreachable!()
    };
    model.animation.as_mut().unwrap().speed = -2.;
    model.animation.as_mut().unwrap().offset_seconds = 3.;
    scene
        .sample(&project, 45., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert_eq!(scene.layers[0].model_animation.unwrap().seconds, 2.);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_gltf_clips_render_from_saved_owned_data_and_reject_missing_selection() {
    let root = tempfile::tempdir().unwrap();
    let project = animated_project(root.path());
    motion_core::storage::save(root.path(), &project).unwrap();
    let project = motion_core::storage::load(root.path()).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    let first = render(&mut renderer, &project, 0.);
    let last = render(&mut renderer, &project, 60.);
    assert!(
        (center(&last) - center(&first) - 40.).abs() < 0.25,
        "layer moves 30 pixels and imported channel moves 10 pixels"
    );
    assert_eq!(
        first,
        render(&mut renderer, &project, 0.),
        "random seeks must not retain a later pose"
    );
    let mut invalid = project.clone();
    let Content::Model { model } = &mut invalid.layers[0].content else {
        unreachable!()
    };
    model.animation.as_mut().unwrap().clip = 9;
    let error = renderer
        .configure_assets(&invalid, root.path())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("object=1") && error.contains("source=7") && error.contains("animation=9")
    );
    assert_eq!(
        first,
        render(&mut renderer, &project, 0.),
        "catalog refusal must preserve the visible owned source"
    );
    let mut hidden = project.clone();
    let Content::Model { model } = &mut hidden.layers[0].content else {
        unreachable!()
    };
    model.animation = Some(motion_core::ModelAnimation {
        clip: 1,
        offset_seconds: 2.,
        speed: 0.,
    });
    renderer.configure_assets(&hidden, root.path()).unwrap();
    assert!(
        render(&mut renderer, &hidden, 0.)
            .chunks_exact(4)
            .all(|p| p[3] == 0),
        "fully collapsed imported animation is an empty visual"
    );
    renderer.configure_assets(&project, root.path()).unwrap();
    assert_eq!(first, render(&mut renderer, &project, 0.));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_morph_weights_render_from_saved_owned_project_without_reopening_the_asset() {
    let root = tempfile::tempdir().unwrap();
    let project = animated_project_kind(root.path(), true);
    motion_core::storage::save(root.path(), &project).unwrap();
    let reopened = motion_core::storage::load(root.path()).unwrap();
    assert_eq!(reopened, project);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&reopened, root.path()).unwrap();
    std::fs::remove_file(root.path().join(&reopened.model_assets[0].path)).unwrap();
    let first = render(&mut renderer, &reopened, 0.);
    let last = render(&mut renderer, &reopened, 60.);
    assert!(
        (center(&last) - center(&first) - 40.).abs() < 0.25,
        "saved layer moves 30 pixels; owned morph displacement adds 10 pixels"
    );
    assert_eq!(
        first,
        render(&mut renderer, &reopened, 0.),
        "seeking backward must restore the first authored morph sample"
    );
    assert_eq!(
        reopened, project,
        "rendering must not mutate saved model animation or layer keys"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_joint_animation_renders_from_saved_owned_project_and_seeks_without_source_files() {
    let root = tempfile::tempdir().unwrap();
    let project = animated_project_features(root.path(), false, true);
    motion_core::storage::save(root.path(), &project).unwrap();
    let reopened = motion_core::storage::load(root.path()).unwrap();
    assert_eq!(reopened, project);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&reopened, root.path()).unwrap();
    std::fs::remove_file(root.path().join(&reopened.model_assets[0].path)).unwrap();
    let first = render(&mut renderer, &reopened, 0.);
    let last = render(&mut renderer, &reopened, 60.);
    assert!((center(&last)-center(&first)-40.).abs()<0.25,"layer moves 30 pixels and owned joint hierarchy moves 10 pixels, ignoring mesh-node offset");
    assert_eq!(
        first,
        render(&mut renderer, &reopened, 0.),
        "backward seeks restore the authored first skin pose"
    );
    assert_eq!(
        reopened, project,
        "skin rendering must not mutate authored data"
    );
}
