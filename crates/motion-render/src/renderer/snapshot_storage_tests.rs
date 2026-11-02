use super::*;
use crate::test_allocations;
use motion_core::{EffectImageInput, EffectImageStage, EffectInstance, Layer};

fn project(count: u64) -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "displacement_map")
        .unwrap();
    let mut project = Project::new(32, 24, 30, 30).unwrap();
    for id in 1..=count + 1 {
        let mut layer = Layer::solid(
            id,
            "Snapshot source",
            [32., 24.],
            [16., 12., 0.],
            [0.2, 0.4, 0.6, 1.],
        );
        let mut effect = EffectInstance::new(
            id,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [32., 24.],
        );
        if id > 1 {
            effect.image_input = Some(EffectImageInput::Layer {
                layer: id - 1,
                stage: EffectImageStage::Effects,
            });
        }
        layer.effects.push(effect);
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    project
}
fn sample(project: &Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn resident_snapshot_preparation_does_not_allocate_lists_or_change_pixels() {
    for count in [1, 8, 64] {
        let project = project(count);
        let scene = sample(&project);
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let target = renderer.capture_target(32, 24).unwrap();
        let expected = renderer.capture(&scene, &target).unwrap().0;
        let order = renderer.effect_gpu.builder.effect_snapshot_order().to_vec();
        assert_eq!(order.len(), count as usize);
        let ledger = renderer.texture_resource_ledger();
        let bytes = renderer.texture_bytes();
        let allocations = test_allocations::count(|| {
            for _ in 0..120 {
                renderer.prepare_input_snapshots().unwrap();
            }
        });
        println!("resident snapshots={count} calls=120 allocations={allocations}");
        assert_eq!(
            allocations, 0,
            "snapshot preparation must not collect a new specs list"
        );
        assert_eq!(renderer.texture_resource_ledger(), ledger);
        assert_eq!(renderer.texture_bytes(), bytes);
        assert_eq!(renderer.read_target(&target).unwrap(), expected);
        assert_eq!(renderer.capture(&scene, &target).unwrap().0, expected);
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn snapshot_pruning_and_direct_transition_preserve_other_images_and_output() {
    use motion_core::{
        masks::LayerMask,
        vector::{PathNode, VectorPath},
        Track,
    };
    let mut project = project(8);
    let path = VectorPath {
        id: 1,
        closed: true,
        tangents: Default::default(),
        nodes: [[4., 4.], [28., 4.], [28., 20.], [4., 20.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| PathNode {
                id: i as u64 + 1,
                geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
            })
            .collect(),
    };
    project.layers[0].masks.push(LayerMask::new(1, path));
    project.validate().unwrap();
    let scene = sample(&project);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.upload_image(99, 1, 1, &[30, 60, 90, 255]).unwrap();
    let static_image = renderer.images[&TextureKey::Static(99)]._texture.clone();
    let target = renderer.capture_target(32, 24).unwrap();
    let previous_pixels = renderer.capture(&scene, &target).unwrap().0;
    assert!(renderer.images.contains_key(&TextureKey::Mask(1)));
    for id in 1..=8 {
        assert!(renderer.images.contains_key(&TextureKey::EffectInput(id)));
    }
    project.layers.last_mut().unwrap().effects[0].image_input = None;
    let pruned = sample(&project);
    renderer
        .effect_gpu
        .builder
        .build(&pruned, &renderer.asset_order, 32, 24, false)
        .unwrap();
    renderer.prepare_input_snapshots().unwrap();
    assert!(!renderer.images.contains_key(&TextureKey::EffectInput(8)));
    for id in 1..=7 {
        assert!(renderer.images.contains_key(&TextureKey::EffectInput(id)));
    }
    assert!(renderer.images.contains_key(&TextureKey::Mask(1)));
    assert_eq!(
        &*renderer.images[&TextureKey::Static(99)]._texture,
        &*static_image
    );
    assert_eq!(renderer.read_target(&target).unwrap(), previous_pixels);
    renderer.release_effect_input_images();
    assert!(renderer
        .images
        .keys()
        .all(|key| !matches!(key, TextureKey::Mask(_) | TextureKey::EffectInput(_))));
    assert_eq!(
        &*renderer.images[&TextureKey::Static(99)]._texture,
        &*static_image
    );
    assert_eq!(
        renderer.texture_bytes,
        renderer
            .images
            .values()
            .map(|image| image.bytes)
            .sum::<u64>()
    );
    let allocations = test_allocations::count(|| {
        for _ in 0..120 {
            renderer.release_effect_input_images();
        }
    });
    assert_eq!(
        allocations, 0,
        "empty direct cleanup must remain allocation-free"
    );
    for layer in &mut project.layers {
        layer.effects.clear();
        layer.masks.clear();
    }
    project.rebuild_plugin_dependencies();
    let plain = sample(&project);
    let actual = renderer.capture(&plain, &target).unwrap().0;
    let mut fresh = pollster::block_on(Renderer::headless()).unwrap();
    let fresh_target = fresh.capture_target(32, 24).unwrap();
    assert_eq!(actual, fresh.capture(&plain, &fresh_target).unwrap().0);
    renderer.check_health().unwrap();
}
