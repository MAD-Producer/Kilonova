use motion_core::{Layer, Project};
use motion_render::{
    preview_memory::{PreviewMemoryPolicy, PREVIEW_MEMORY_COOLDOWN_MS, PREVIEW_MEMORY_LEVELS},
    resource_policy::{MemoryPolicy, MAX_PREVIEW_TARGET_BUDGET},
    Presenter, PreviewFrameTargets, Renderer, ResourceBudgetScope, ResourceRejection, Scene,
};

fn set_budget(renderer: &mut Renderer, bytes: u64) {
    renderer
        .set_memory_policy(MemoryPolicy {
            preview_target_budget_bytes: bytes,
            ..renderer.memory_policy()
        })
        .unwrap();
}

fn scene(color: [f32; 4]) -> (Project, Scene) {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.layers.push(Layer::solid(
        1,
        "budget fixture",
        [64.; 2],
        [32., 32., 0.],
        color,
    ));
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    (project, scene)
}

fn commit(renderer: &mut Renderer, targets: &mut PreviewFrameTargets, scene: &Scene, edge: u32) {
    let target = targets.prepare(renderer, edge, edge).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    renderer
        .encode_preview(scene, &target.view, edge, edge, &mut encoder, None)
        .unwrap();
    renderer.queue.submit(Some(encoder.finish()));
    targets.commit_submitted();
}

fn read_front(renderer: &Renderer, targets: &PreviewFrameTargets) -> Vec<u8> {
    let output = renderer
        .capture_target(targets.width, targets.height)
        .unwrap();
    Presenter::new(renderer, &targets.view, wgpu::TextureFormat::Rgba8UnormSrgb)
        .draw(renderer, &output.view).unwrap();
    renderer.read_target(&output).unwrap()
}

fn ledger(targets: &PreviewFrameTargets) -> motion_render::TextureResourceLedger {
    let mut ledger = motion_render::TextureResourceLedger::default();
    targets.collect_texture_resources(&mut ledger);
    ledger
}

#[test]
fn legacy_policy_defaults_and_invalid_updates_preserve_the_live_policy() {
    let legacy: MemoryPolicy = serde_json::from_str(
        r#"{"sourceTextureBudgetBytes":134217728,"scratchBudgetBytes":67108864}"#,
    )
    .unwrap();
    assert_eq!(
        legacy.preview_target_budget_bytes,
        MAX_PREVIEW_TARGET_BUDGET
    );
    assert_eq!(legacy, MemoryPolicy::default());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    set_budget(&mut renderer, 20_480);
    let before = renderer.memory_policy();
    for bytes in [0, 7, MAX_PREVIEW_TARGET_BUDGET + 1, u64::MAX] {
        assert!(renderer
            .set_memory_policy(MemoryPolicy {
                preview_target_budget_bytes: bytes,
                ..before
            })
            .is_err());
        assert_eq!(renderer.memory_policy(), before);
    }
    let round_trip: MemoryPolicy =
        serde_json::from_value(serde_json::to_value(before).unwrap()).unwrap();
    assert_eq!(round_trip, before);
}

#[test]
fn combined_budget_rejects_reused_candidate_preserves_pixels_and_allows_smaller_frame() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut targets = PreviewFrameTargets::new(&renderer, 64, 64).unwrap();
    let (_, red) = scene([1., 0., 0., 1.]);
    commit(&mut renderer, &mut targets, &red, 64);
    let saved = read_front(&renderer, &targets);
    assert!(saved.chunks_exact(4).all(|pixel| pixel == [255, 0, 0, 255]));
    assert_eq!(targets.texture_bytes(), 32_768);
    let before = ledger(&targets);
    set_budget(&mut renderer, 20_480);
    let error = targets.prepare(&renderer, 64, 64).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::PreviewTarget,
            required_bytes: 32_768,
            budget_bytes: 20_480,
        })
    );
    assert_eq!(
        ledger(&targets),
        before,
        "a refused request must not replace either allocation"
    );
    assert!(targets.valid());
    assert_eq!(read_front(&renderer, &targets), saved);

    let (project, green) = scene([0., 1., 0., 1.]);
    let frozen = serde_json::to_vec(&project).unwrap();
    commit(&mut renderer, &mut targets, &green, 32);
    assert_eq!(targets.texture_bytes(), 20_480);
    assert!(read_front(&renderer, &targets)
        .chunks_exact(4)
        .all(|p| p == [0, 255, 0, 255]));
    assert_eq!(
        ledger(&targets).total_known_bytes(),
        targets.texture_bytes()
    );
    assert_eq!(ledger(&targets).resources().count(), 2);

    // Formal capture keeps the original size and never inherits preview target
    // downscaling, even when its target is larger than the preview allowance.
    set_budget(&mut renderer, 8);
    let target = renderer.capture_target(64, 64).unwrap();
    let (full, _) = renderer.capture(&green, &target).unwrap();
    assert_eq!(full.len(), 64 * 64 * 4);
    assert!(full.chunks_exact(4).all(|p| p == [0, 255, 0, 255]));
    assert_eq!(serde_json::to_vec(&project).unwrap(), frozen);
}

#[test]
fn allocation_refusal_uses_combined_capacity_and_replacement_retains_only_two_targets() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    set_budget(&mut renderer, 16_384);
    let mut targets = PreviewFrameTargets::new(&renderer, 32, 32).unwrap();
    let before = ledger(&targets);
    let error = targets.prepare(&renderer, 64, 64).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::PreviewTarget,
            required_bytes: 20_480,
            budget_bytes: 16_384,
        })
    );
    assert!(targets.prepared().is_none());
    assert_eq!(ledger(&targets), before);
    targets.prepare(&renderer, 48, 48).unwrap();
    let old = ledger(&targets);
    targets.prepare(&renderer, 16, 16).unwrap();
    let new = ledger(&targets);
    assert_eq!(new.resources().count(), 2);
    assert_eq!(new.total_known_bytes(), 32 * 32 * 4 + 16 * 16 * 4);
    assert_eq!(
        old.resources()
            .filter(|a| new.resources().any(|b| a.id == b.id))
            .count(),
        1
    );
    let error = targets.prepare(&renderer, 0, 16).err().unwrap();
    assert!(
        error.resource_rejection().is_none(),
        "malformed input cannot trigger memory degradation"
    );
    assert_eq!(ledger(&targets), new);
    assert!(PreviewFrameTargets::new(&renderer, 65, 65).is_err());
}

#[test]
fn float_color_targets_are_charged_their_actual_capacity() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Rgba16Float,
    ))
    .unwrap();
    set_budget(&mut renderer, 16_384);
    let mut targets = PreviewFrameTargets::new(&renderer, 32, 32).unwrap();
    assert_eq!(targets.texture_bytes(), 8_192);
    targets.prepare(&renderer, 32, 32).unwrap();
    assert_eq!(targets.texture_bytes(), 16_384);
    assert_eq!(ledger(&targets).total_known_bytes(), 16_384);
    set_budget(&mut renderer, 8_192);
    let before = ledger(&targets);
    let error = targets.prepare(&renderer, 32, 32).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::PreviewTarget,
            required_bytes: 16_384,
            budget_bytes: 8_192,
        })
    );
    assert_eq!(ledger(&targets), before);
}

#[test]
fn real_target_refusals_exhaust_once_hold_front_and_recover_after_a_budget_change() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut targets = PreviewFrameTargets::new(&renderer, 64, 64).unwrap();
    let (_, red) = scene([1., 0., 0., 1.]);
    commit(&mut renderer, &mut targets, &red, 64);
    let saved = read_front(&renderer, &targets);
    set_budget(&mut renderer, 8);
    let mut policy = PreviewMemoryPolicy::default();
    for level in 0..PREVIEW_MEMORY_LEVELS {
        assert!(policy.begin(0));
        let size = policy.quality(true).dimensions(64, 64);
        let error = targets.prepare(&renderer, size.0, size.1).err().unwrap();
        assert_eq!(
            policy.refused(*error.resource_rejection().unwrap(), 0),
            level + 1 < PREVIEW_MEMORY_LEVELS
        );
        targets.discard_candidate();
        renderer.reclaim_preview_working_set().unwrap();
    }
    assert!(policy.blocked());
    for now in 1..1000 {
        assert!(!policy.begin(now));
    }
    assert_eq!(policy.refusal_count, u64::from(PREVIEW_MEMORY_LEVELS));
    assert_eq!(read_front(&renderer, &targets), saved);
    assert!(targets.valid());
    assert!(policy.begin(PREVIEW_MEMORY_COOLDOWN_MS));
    assert_eq!(policy.quality(true).level, PREVIEW_MEMORY_LEVELS - 1);

    set_budget(&mut renderer, MAX_PREVIEW_TARGET_BUDGET);
    policy.changed();
    let edge = policy.quality(true).dimensions(64, 64).0;
    commit(&mut renderer, &mut targets, &red, edge);
    policy.succeeded();
    assert!(!policy.blocked());
    assert_eq!(policy.failures(), 0);
    assert!(read_front(&renderer, &targets)
        .chunks_exact(4)
        .all(|p| p == [255, 0, 0, 255]));
}
