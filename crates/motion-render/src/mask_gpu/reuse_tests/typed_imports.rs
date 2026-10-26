use super::*;
use crate::effect_plan::PlanBuilder;
use motion_effects::{image_contract::ChannelPrecision, Registry};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn checked_mask_import_refuses_stale_precision_and_wrong_cached_format_then_recovers() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let mut p = project();
    let package = motion_effects::builtin::package().unwrap();
    let def = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "tint")
        .unwrap();
    p.layers[0].effects.push(motion_core::EffectInstance::new(
        3,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        def,
        [64.; 2],
    ));
    p.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&p).unwrap();
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    builder.build(&scene, &[0], 64, 64, true).unwrap();
    let mut images = HashMap::new();
    let mut bytes = 0;
    let prepare = |gpu: &mut MaskGpu,
                   builder: &PlanBuilder,
                   images: &mut HashMap<TextureKey, GpuImage>,
                   bytes: &mut u64| {
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        let changed = gpu.prepare_checked(
            &renderer.device,
            &renderer.queue,
            &mut encoder,
            &layout,
            &scene,
            &builder.frame.masks,
            images,
            bytes,
            0,
            128 << 20,
            builder.effect_execution(),
        )?;
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        gpu.allocation_uploads.submitted();
        Ok::<_, RenderError>(changed)
    };
    assert!(prepare(&mut gpu, &builder, &mut images, &mut bytes).unwrap());
    let key = TextureKey::Mask(1);
    let original_pixels = read(&renderer, &images[&key]._texture);
    let original_view = images[&key].view.clone();
    let original_bytes = bytes;
    builder.set_precision(ChannelPrecision::Float16);
    builder.build(&scene, &[0], 64, 64, true).unwrap();
    let error = prepare(&mut gpu, &builder, &mut images, &mut bytes).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("mask coverage differs from its checked execution value"),
        "{error}"
    );
    assert_eq!(bytes, original_bytes);
    assert_eq!(images[&key].view, original_view);
    assert_eq!(read(&renderer, &images[&key]._texture), original_pixels);
    builder.set_precision(ChannelPrecision::Unorm8);
    builder.build(&scene, &[0], 64, 64, true).unwrap();
    assert!(!prepare(&mut gpu, &builder, &mut images, &mut bytes).unwrap());

    // Same fingerprint, size and cache key; only the actual native format is
    // wrong. Validation must run before the healthy fingerprint shortcut.
    let original = images.remove(&key).unwrap();
    let wrong = texture(
        &renderer.device,
        &layout,
        &gpu.sampler,
        64,
        64,
        wgpu::TextureFormat::Rgba8Unorm,
        renderer.allocation_budget(),
    )
    .unwrap();
    images.insert(
        key,
        GpuImage {
            _texture: wrong.texture,
            view: wrong.view,
            bind_group: wrong.composite,
            size: (64, 64),
            bytes: 64 * 64 * 4,
            video_stamp: None,
        },
    );
    let error = prepare(&mut gpu, &builder, &mut images, &mut bytes).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("phase=frame_import source=Mask(0)"),
        "{error}"
    );
    assert!(error.to_string().contains("Rgba8Unorm"), "{error}");
    assert_eq!(bytes, original_bytes);
    images.insert(key, original);
    assert!(!prepare(&mut gpu, &builder, &mut images, &mut bytes).unwrap());
    assert_eq!(images[&key].view, original_view);
    assert_eq!(read(&renderer, &images[&key]._texture), original_pixels);
    renderer.check_health().unwrap();
}
