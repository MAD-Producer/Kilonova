use super::*;

fn project(effect: &str, extent: u32) -> Project {
    let package = motion_effects::builtin::packages()
        .unwrap()
        .into_iter()
        .find(|package| {
            package
                .manifest
                .effects
                .iter()
                .any(|definition| definition.id == effect)
        })
        .unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|definition| definition.id == effect)
        .unwrap();
    let mut project = Project::new(extent, extent, 30, 1).unwrap();
    project.background = [0.; 4];
    let mut layer = Layer::solid(
        1,
        "Admission test",
        [extent as f32; 2],
        [extent as f32 / 2., extent as f32 / 2., 0.],
        [1., 0., 0., 1.],
    );
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [extent as f32; 2],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project
}
fn scene(project: &Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn build(renderer: &mut Renderer, scene: &Scene, extent: u32) {
    renderer
        .effect_gpu
        .builder
        .build(scene, &renderer.asset_order, extent, extent, true)
        .unwrap();
}
fn prepare(renderer: &mut Renderer, scene: &Scene) -> Result<()> {
    renderer.effect_gpu.state.prepare(
        &renderer.device,
        &renderer.queue,
        &renderer.image_layout,
        &renderer.effect_gpu.builder,
        scene,
        renderer.texture_bytes,
        renderer.effect_gpu.builder.source_texture_budget,
    )
}
fn flush(renderer: &Renderer) {
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
}
fn scratch_ids(renderer: &Renderer) -> Vec<String> {
    renderer
        .texture_resource_ledger()
        .resources()
        .filter(|resource| {
            resource
                .references
                .iter()
                .any(|reference| reference.category == crate::TextureCategory::EffectScratch)
        })
        .map(|resource| resource.id.clone())
        .collect()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn refused_later_effect_pool_slot_preserves_old_allocations_and_pixels_then_recovers() {
    let budget = crate::AllocationBudget::new(SYSTEM_BYTES + (4 << 20)).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let small = scene(&project("gaussian_blur", 32));
    let target = renderer.capture_target(32, 32).unwrap();
    let old = renderer.capture(&small, &target).unwrap().0;
    assert!(old
        .chunks_exact(4)
        .any(|pixel| pixel[0] > 200 && pixel[3] > 200));
    let before = budget.info().unwrap();
    let ids = scratch_ids(&renderer);
    let large = scene(&project("gaussian_blur", 128));
    build(&mut renderer, &large, 128);
    let allocations = &renderer.effect_gpu.builder.frame.scratch_allocations;
    let replacements: Vec<_> = allocations
        .iter()
        .enumerate()
        .filter(|(i, allocation)| {
            renderer.effect_gpu.state.pool.get(*i)
                    .and_then(Option::as_ref)
                    .is_none_or(|texture| {
                        texture.texture.width() != allocation.width
                            || texture.texture.height() != allocation.height
                            || texture.texture.format() != allocation.class.format.native()
                    })
        })
        .map(|(i, allocation)| (i, allocation.bytes()))
        .collect();
    assert!(
        replacements.len() >= 2,
        "fixture must exercise a partial pool replacement"
    );
    let required = before.admitted_bytes + replacements[0].1 + replacements[1].1;
    budget.set_limit(required - 1).unwrap();
    let error = prepare(&mut renderer, &large).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: required,
            budget_bytes: required - 1
        })
    );
    assert!(
        error
            .to_string()
            .contains(&format!("effect scratch slot={} ", replacements[1].0)),
        "{error}"
    );
    assert_eq!(scratch_ids(&renderer), ids);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(
        budget.info().unwrap().live_allocations,
        before.live_allocations
    );
    budget.set_limit(SYSTEM_BYTES + (4 << 20)).unwrap();
    assert_eq!(renderer.capture(&small, &target).unwrap().0, old);
    let large_target = renderer.capture_target(128, 128).unwrap();
    assert!(renderer
        .capture(&large, &large_target)
        .unwrap()
        .0
        .chunks_exact(4)
        .any(|pixel| pixel[0] > 240 && pixel[3] > 240));
    assert_ne!(scratch_ids(&renderer), ids);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn late_effect_parameter_refusal_is_typed_and_reuses_admitted_scratch_on_recovery() {
    let budget = crate::AllocationBudget::new(SYSTEM_BYTES + (1 << 20)).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let scene = scene(&project("tint", 8));
    build(&mut renderer, &scene, 8);
    let scratch = renderer.effect_gpu.builder.frame.scratch_bytes();
    let required = SYSTEM_BYTES + scratch + motion_effects::shader::UNIFORM_BYTES as u64;
    budget.set_limit(required - 1).unwrap();
    let error = prepare(&mut renderer, &scene).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: required,
            budget_bytes: required - 1
        })
    );
    assert!(
        error
            .to_string()
            .contains("effect parameters pass=0 phase=allocate"),
        "{error}"
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + scratch);
    let ids = scratch_ids(&renderer);
    budget.set_limit(SYSTEM_BYTES + (1 << 20)).unwrap();
    renderer.preflight_effects(&scene, 8, 8).unwrap();
    assert_eq!(scratch_ids(&renderer), ids);
    let target = renderer.capture_target(8, 8).unwrap();
    let pixels = renderer.capture(&scene, &target).unwrap().0;
    assert!(pixels
        .chunks_exact(4)
        .all(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2] && pixel[3] == 255));
    drop(target);
    renderer.effect_gpu.state.release_scratch();
    flush(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn queued_curve_uploads_and_abandoned_encoders_retain_admitted_destinations() {
    let budget = crate::AllocationBudget::new(SYSTEM_BYTES + (1 << 20)).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let scene = scene(&project("curves", 8));
    build(&mut renderer, &scene, 8);
    let scratch = renderer.effect_gpu.builder.frame.scratch_bytes();
    budget.set_limit(SYSTEM_BYTES + scratch + 1023).unwrap();
    let error = prepare(&mut renderer, &scene).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("effect curve index=0 phase=allocate"),
        "{error}"
    );
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            ..
        })
    ));
    assert_eq!(renderer.texture_memory_breakdown().curve_luts, 0);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + scratch);
    budget.set_limit(SYSTEM_BYTES + (1 << 20)).unwrap();
    prepare(&mut renderer, &scene).unwrap();
    let held = budget.info().unwrap().admitted_bytes;
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.effect_gpu.state.release_scratch();
    assert_eq!(budget.info().unwrap().admitted_bytes, held);
    drop(encoder.finish());
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        SYSTEM_BYTES + 1024,
        "only the queued LUT destination remains"
    );
    flush(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn package_texture_refusal_keeps_resource_counts_and_recovers_through_real_upload() {
    let budget = crate::AllocationBudget::new(SYSTEM_BYTES + (1 << 20)).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let scene = scene(&project("dither", 8));
    build(&mut renderer, &scene, 8);
    let resources = renderer
        .effect_gpu
        .state
        .pending_resource_bytes(&renderer.effect_gpu.builder);
    assert!(resources > 0, "fixture must include an actual packaged PNG");
    let frame = &renderer.effect_gpu.builder.frame;
    let scratch = frame.scratch_bytes();
    let parameters = frame.passes.len() as u64 * motion_effects::shader::UNIFORM_BYTES as u64;
    let required = SYSTEM_BYTES + scratch + parameters + resources;
    budget.set_limit(required - 1).unwrap();
    let error = prepare(&mut renderer, &scene).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: required,
            budget_bytes: required - 1
        })
    );
    for context in [
        "effect resource package=",
        "path=assets/blue-noise.png phase=allocate",
    ] {
        assert!(error.to_string().contains(context), "{error}");
    }
    assert_eq!(renderer.effect_gpu.state.resource_bytes, 0);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        SYSTEM_BYTES + scratch + parameters
    );
    budget.set_limit(SYSTEM_BYTES + (1 << 20)).unwrap();
    prepare(&mut renderer, &scene).unwrap();
    assert_eq!(renderer.effect_gpu.state.resource_bytes, resources);
    let target = renderer.capture_target(8, 8).unwrap();
    assert!(renderer
        .capture(&scene, &target)
        .unwrap()
        .0
        .chunks_exact(4)
        .all(|pixel| pixel[3] == 255));
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn package_png_runtime_decode_obeys_current_cpu_policy_and_recovers_native_pixels() {
    let project = project("dither", 8);
    let scene = scene(&project);
    let mut reference = pollster::block_on(Renderer::headless()).unwrap();
    let reference_target = reference.capture_target(8, 8).unwrap();
    let expected = reference.capture(&scene, &reference_target).unwrap().0;
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let original = renderer.memory_policy();
    let target = renderer.capture_target(8, 8).unwrap();
    renderer.set_memory_policy(crate::resource_policy::MemoryPolicy {
        decoded_image_budget_bytes: 4, ..original
    }).unwrap();
    let error = renderer.capture(&scene, &target).unwrap_err();
    match error.resource_rejection() {
        Some(ResourceRejection::Budget { scope: ResourceBudgetScope::DecodedImages, required_bytes, budget_bytes }) => {
            assert!(*required_bytes > 4);
            assert_eq!(*budget_bytes, 4);
        }
        other => panic!("expected CPU package refusal, got {other:?}: {error}"),
    }
    for context in ["effect resource package=", "path=assets/blue-noise.png phase=decode"] {
        assert!(error.to_string().contains(context), "{error}");
    }
    assert_eq!(renderer.effect_gpu.state.resource_bytes, 0);
    assert_eq!(renderer.texture_memory_breakdown().plugin_resources, 0);
    renderer.set_memory_policy(original).unwrap();
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, expected);
    assert!(renderer.effect_gpu.state.resource_bytes > 0);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn curve_preflight_shrink_and_growth_rebinds_the_new_lut_without_retaining_old_capacity() {
    let budget = crate::AllocationBudget::new(SYSTEM_BYTES + (1 << 20)).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let original = scene(&project("curves", 8));
    let intermediate = scene(&project("tint", 8));
    let mut changed_project = project("curves", 8);
    changed_project.layers[0].effects[0]
        .params
        .get_mut("p0001")
        .unwrap()
        .curve
        .as_mut()
        .unwrap()
        .value
        .channels[1] = vec![[0., 0.], [1., 0.]];
    let changed = scene(&changed_project);
    let target = renderer.capture_target(8, 8).unwrap();
    let initial = renderer.capture(&original, &target).unwrap().0;
    assert!(initial
        .chunks_exact(4)
        .all(|pixel| pixel[0] >= 250 && pixel[1] <= 1 && pixel[2] <= 1 && pixel[3] == 255));
    let before = budget.info().unwrap().admitted_bytes;
    let ids = scratch_ids(&renderer);
    for _ in 0..3 {
        // These are valid plans with the same scratch slots and number of
        // passes. No draw between them can repair a stale cached LUT binding.
        renderer.preflight_effects(&intermediate, 8, 8).unwrap();
        assert_eq!(scratch_ids(&renderer), ids);
        assert_eq!(renderer.texture_memory_breakdown().curve_luts, 0);
        renderer.preflight_effects(&changed, 8, 8).unwrap();
        assert_eq!(scratch_ids(&renderer), ids);
        let actual = renderer.capture(&changed, &target).unwrap().0;
        assert!(
            actual
                .chunks_exact(4)
                .all(|pixel| pixel[0] <= 1 && pixel[1] <= 1 && pixel[2] <= 1 && pixel[3] == 255),
            "{actual:?}"
        );
        assert_eq!(budget.info().unwrap().admitted_bytes, before);
        assert_eq!(renderer.texture_memory_breakdown().curve_luts, 1024);
    }
    renderer.check_health().unwrap();
}
