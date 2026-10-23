use super::*;

fn scene(size: u32) -> Scene {
    let mut project = Project::new(size, size, 30, 60).unwrap();
    project.background = [0.; 4];
    project.camera.created = false;
    let center = size as f32 * 0.5;
    let extent = size as f32 * 0.5;
    project.layers = vec![
        Layer::solid(77, "coverage", [extent; 2], [center, center, 0.], [1.; 4]),
        Layer::solid(76, "back", [extent; 2], [center, center, 0.], [0.2, 0.2, 0.2, 1.]),
        Layer::solid(78, "front", [extent; 2], [center, center, 0.], [0.7, 0.7, 0.7, 0.5]),
    ];
    project.layers[0].visible = false;
    project.layers[2].track_matte = Some(motion_core::compositing::TrackMatte {
        source: 77, mode: motion_core::compositing::MatteMode::Alpha, hide_source: true,
    });
    project.layers[2].blend = motion_core::compositing::LayerBlend {
        mode: motion_core::compositing::BlendMode::Multiply,
        space: motion_core::compositing::BlendSpace::Srgb,
    };
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    scene
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn input_snapshot_refusals_retain_layer_extent_typed_causes_old_texture_and_pixels() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let small = renderer.capture_target(32, 32).unwrap();
    let pixels = renderer.capture(&scene(32), &small).unwrap().0;
    assert_eq!(pixels[(16 * 32 + 16) * 4 + 3], 255);
    assert!(pixels[(16 * 32 + 16) * 4].abs_diff(43) <= 3);
    assert_eq!(pixels[3], 0);
    let key = TextureKey::EffectInput(77);
    let old = renderer.images[&key]._texture.clone();
    assert_eq!(renderer.images[&key].size, (16, 16));
    let large = renderer.capture_target(64, 64).unwrap();
    // Isolate snapshot replacement from the earlier effect-scratch admission.
    renderer.preflight_effects(&scene(64), 64, 64).unwrap();
    let before = budget.info().unwrap();
    let booked = renderer.texture_bytes;
    budget.set_limit(before.admitted_bytes).unwrap();
    let error = renderer.capture(&scene(64), &large).unwrap_err();
    assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
        scope: ResourceBudgetScope::RenderAllocations,
        required_bytes: before.admitted_bytes + 32 * 32 * 4,
        budget_bytes: before.admitted_bytes,
    }));
    for context in ["effects frame=0 phase=input_snapshots", "input snapshot layer=77 size=32x32", "phase=allocate"] {
        assert!(error.to_string().contains(context), "missing {context}: {error}");
    }
    assert_eq!(&*renderer.images[&key]._texture, &*old);
    assert_eq!(renderer.images[&key].size, (16, 16));
    assert_eq!(renderer.texture_bytes, booked);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(renderer.read_target(&small).unwrap(), pixels);

    // Exercise the distinct category guard on the already prepared larger plan.
    // Its rejection must preserve SourceTextures rather than become a combined
    // allocation error, and must leave the same prior snapshot usable.
    budget.set_limit(8 << 20).unwrap();
    let category_limit = renderer.texture_bytes + renderer.video_plane_bytes()
        + renderer.effect_gpu.state.resource_bytes;
    let prior_limit = renderer.effect_gpu.builder.source_texture_budget;
    renderer.effect_gpu.builder.source_texture_budget = category_limit;
    let order = crate::effect_plan::image_input_order_for(&renderer.effect_gpu.builder.frame, &[]).unwrap();
    let error = renderer.prepare_input_snapshots(&order).unwrap_err();
    assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
        scope: ResourceBudgetScope::SourceTextures,
        required_bytes: category_limit + 32 * 32 * 4, budget_bytes: category_limit,
    }));
    for context in ["effect image snapshots exceed source texture budget", "input snapshot layer=77 size=32x32 phase=source_budget"] {
        assert!(error.to_string().contains(context), "missing {context}: {error}");
    }
    assert_eq!(&*renderer.images[&key]._texture, &*old);
    assert_eq!(renderer.images[&key].size, (16, 16));
    assert_eq!(renderer.texture_bytes, booked);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(renderer.read_target(&small).unwrap(), pixels);
    renderer.effect_gpu.builder.source_texture_budget = prior_limit;
    let recovered = renderer.capture(&scene(64), &large).unwrap().0;
    assert_eq!(renderer.images[&key].size, (32, 32));
    assert_ne!(&*renderer.images[&key]._texture, &*old);
    assert_eq!(recovered[(32 * 64 + 32) * 4 + 3], 255);
    assert!(recovered[(32 * 64 + 32) * 4].abs_diff(43) <= 3);
    assert_eq!(recovered[3], 0);
    renderer.check_health().unwrap();
}
