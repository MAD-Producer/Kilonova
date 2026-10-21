use super::*;

#[test]
fn feathered_mask_mesh_and_filter_uniforms_share_renderer_admission() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    let layout = layout(&renderer.device);
    let mut gpu = MaskGpu::new(&renderer.device, &layout).unwrap();
    let p = project();
    let mut scene = Scene::new(&p);
    scene.sample(&p, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let masks = MaskCache::default().build(&scene, 1., false).unwrap();
    assert_eq!(masks.len(), 1);
    let mesh_bytes = (masks[0].vertices.len() as u64 * 24).max(24).next_multiple_of(4);
    // Two feather axes and one combine pass have three distinct 624-byte
    // uniforms. Texture admission is a separate pending part of #61.
    let expected = mesh_bytes + 3 * 624;
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let mut images = HashMap::new();
    let mut bytes = 0;
    assert!(gpu.prepare(&renderer.device, &renderer.queue, &mut encoder, &layout,
        &scene, &masks, &mut images, &mut bytes, 0, 128 << 20).unwrap());
    assert_eq!(gpu.uniforms.len(), 3);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + expected);
    renderer.submit_tracked(encoder.finish(), |_| {});
    let pixels = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
    assert_eq!(pixels[32 * 64 + 32], 255);
    assert_eq!(pixels[32 * 64 + 2], 0);
    drop(gpu);
    assert_eq!(budget.info().unwrap(), before);
}
