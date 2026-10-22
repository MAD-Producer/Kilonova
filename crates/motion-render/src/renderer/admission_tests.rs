use super::*;

const SYSTEM_BYTES: u64 = FIXED_RENDERER_ALLOCATION_BYTES;
use motion_core::{Asset, Content, EffectInstance, Layer};

mod effect_resources;
mod frame_blend;
mod compositing_system;
mod matte_textures;
mod accumulator_textures;
mod input_snapshot_context;
mod retained_rasters;

#[test]
fn cached_effect_bindings_keep_admitted_sources_until_the_bindings_are_invalidated() {
    let budget = crate::AllocationBudget::new(SYSTEM_BYTES + (1 << 20)).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    renderer
        .upload_image(7, 4, 4, &[0, 255, 0, 255].repeat(16))
        .unwrap();
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|effect| effect.id == "tint")
        .unwrap();
    let mut project = Project::new(8, 8, 30, 1).unwrap();
    project.background = [0.; 4];
    project.assets.push(Asset {
        id: 7,
        path: "assets/test.png".into(),
        width: 4,
        height: 4,
    });
    let mut layer = Layer::solid(1, "Image", [8.; 2], [4., 4., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 7 };
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [8.; 2],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let target = renderer.capture_target(8, 8).unwrap();
    let pixels = renderer.capture(&scene, &target).unwrap().0;
    assert!(pixels
        .chunks_exact(4)
        .all(|pixel| pixel[3] == 255 && pixel[0] == pixel[1] && pixel[1] == pixel[2]));
    let before = budget.info().unwrap().admitted_bytes;
    // Isolate the cached-binding lifetime after real rendering and completion.
    // Ordinary cache eviction immediately invalidates these bindings; omitting
    // that second step here exposes whether they independently retain admission.
    let image = renderer.images.remove(&TextureKey::Static(7)).unwrap();
    renderer.texture_bytes -= image.bytes;
    drop(image);
    assert_eq!(budget.info().unwrap().admitted_bytes, before);
    renderer.effect_gpu.invalidate();
    assert_eq!(budget.info().unwrap().admitted_bytes, before - 64);
    renderer.check_health().unwrap();
}
