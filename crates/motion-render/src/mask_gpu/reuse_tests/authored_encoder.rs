use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn abandoned_managed_mask_encoders_invalidate_coverage_and_preserve_primary_panic() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let mut p = project();
    let mut scene = Scene::new(&p).unwrap();
    let mut cache = MaskCache::default();
    let mut images = HashMap::new();
    let mut bytes = 0;
    let mut prepare =
        |gpu: &mut MaskGpu, scene: &Scene, submit: bool, early: bool, unwind: bool| {
            let masks = cache.build(scene, 1., false).unwrap();
            let guard = gpu.authored_raster_guard();
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            let changed = gpu
                .prepare(
                    &renderer.device,
                    &renderer.queue,
                    &mut encoder,
                    &layout,
                    scene,
                    &masks,
                    &mut images,
                    &mut bytes,
                    0,
                    128 << 20,
                )
                .unwrap();
            if unwind {
                std::panic::panic_any("original mask encoder panic");
            }
            if submit {
                renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
                guard.commit();
            } else if early {
                guard.commit();
            }
            let pixels = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
            (changed, pixels[32 * 64 + 32])
        };
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert_eq!(prepare(&mut gpu, &scene, true, false, false), (true, 255));
    for (opacity, unwind) in [(50., false), (25., true)] {
        p.layers[0].masks[0].opacity.value = opacity;
        scene
            .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let old = if opacity == 50. { 255 } else { 128 };
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            prepare(&mut gpu, &scene, false, false, unwind)
        }));
        if unwind {
            assert_eq!(
                failed.unwrap_err().downcast_ref::<&str>(),
                Some(&"original mask encoder panic")
            );
        } else {
            assert_eq!(failed.unwrap(), (true, old));
        }
        let (changed, pixel) = prepare(&mut gpu, &scene, true, false, false);
        assert!(changed, "abandoned coverage must not count as ready");
        assert!(pixel.abs_diff((opacity * 2.55).round() as u8) <= 1);
    }
    p.layers[0].masks[0].opacity.value = 75.;
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert!(prepare(&mut gpu, &scene, false, true, false).0);
    assert_eq!(
        prepare(&mut gpu, &scene, true, false, false),
        (false, 64),
        "negative control: committing before submission must retain stale coverage"
    );
    drop(gpu.authored_raster_guard());
    let (changed, pixel) = prepare(&mut gpu, &scene, true, false, false);
    assert!(
        changed && pixel.abs_diff(191) <= 1,
        "recovery must publish new pixels"
    );
}
