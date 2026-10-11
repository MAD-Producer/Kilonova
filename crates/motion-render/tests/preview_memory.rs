use motion_core::{EffectInstance, Layer, Project};
use motion_render::{
    effect_plan::PlanBuilder, preview_memory::*, Presenter, PreviewFrameTargets, Renderer,
    ResourceBudgetScope, ResourceRejection, Scene,
};

fn rejection() -> ResourceRejection {
    ResourceRejection::Budget {
        scope: ResourceBudgetScope::SourceTextures,
        required_bytes: 129 << 20,
        budget_bytes: 128 << 20,
    }
}
#[test]
fn exhaustion_survives_repeated_requests_and_cooldown_probes_only_the_floor() {
    let mut policy = PreviewMemoryPolicy::default();
    for level in 0..PREVIEW_MEMORY_LEVELS {
        assert!(policy.begin(0));
        assert_eq!(policy.quality(false).level, level);
        assert_eq!(
            policy.refused(rejection(), 0),
            level + 1 < PREVIEW_MEMORY_LEVELS
        );
    }
    for t in 1..PREVIEW_MEMORY_COOLDOWN_MS {
        assert!(!policy.begin(t));
    }
    assert_eq!(policy.refusal_count, 4);
    assert!(policy.begin(PREVIEW_MEMORY_COOLDOWN_MS));
    assert_eq!(policy.quality(true).level, 3);
    assert!(!policy.refused(rejection(), PREVIEW_MEMORY_COOLDOWN_MS));
    assert_eq!(policy.refusal_count, 5);
    policy.changed();
    assert!(policy.begin(PREVIEW_MEMORY_COOLDOWN_MS + 1));
    assert_eq!(policy.quality(false).level, 3);
}
#[test]
fn recovery_needs_time_and_healthy_frames_and_failed_probe_restores_working_quality() {
    let mut policy = PreviewMemoryPolicy::default();
    assert_eq!(policy.quality(true).image_edge, None);
    policy.refused(rejection(), 0);
    let working = policy.quality(true);
    assert_eq!(working.image_edge, Some(1024));
    assert_eq!(working.video_edge, Some(1024));
    assert_eq!(working.dimensions(101, 1), (51, 1));
    for _ in 0..119 {
        policy.succeeded();
    }
    policy.begin(PREVIEW_MEMORY_COOLDOWN_MS);
    assert_eq!(policy.quality(true), working);
    policy.succeeded();
    policy.begin(PREVIEW_MEMORY_COOLDOWN_MS - 1);
    assert_eq!(policy.quality(true), working);
    policy.begin(PREVIEW_MEMORY_COOLDOWN_MS);
    assert_eq!(policy.quality(true).level, 0);
    assert!(policy.refused(rejection(), PREVIEW_MEMORY_COOLDOWN_MS));
    assert_eq!(policy.quality(true), working);
    assert!(!policy.blocked());
}
fn scene(p: &Project) -> Scene {
    let mut scene = Scene::new(p);
    scene
        .sample(p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn project(size: [f32; 2], color: [f32; 4], blur: bool) -> Project {
    let mut p = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "preview", size, [32., 32., 0.], color);
    if blur {
        let package = motion_effects::builtin::package().unwrap();
        let def = package
            .manifest
            .effects
            .iter()
            .find(|e| e.id == "gaussian_blur")
            .unwrap();
        layer.effects.push(EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            def,
            size,
        ));
    }
    p.layers.push(layer);
    p.rebuild_plugin_dependencies();
    p
}
fn pixels(r: &Renderer, targets: &PreviewFrameTargets) -> Vec<u8> {
    let output = r.capture_target(targets.width, targets.height).unwrap();
    Presenter::new(r, &targets.view, wgpu::TextureFormat::Rgba8UnormSrgb).draw(r, &output.view);
    r.read_target(&output).unwrap()
}
#[test]
fn failed_candidate_preserves_front_pixels_then_lower_resolution_commits_and_reuses_two_targets() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_preview_resource_guard(true);
    let mut targets = PreviewFrameTargets::new(&renderer, 64, 64).unwrap();
    assert!(!targets.valid());
    let first = scene(&project([64.; 2], [1., 0., 0., 1.], false));
    let target = targets.prepare(&renderer, 64, 64).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    renderer
        .encode_preview(&first, &target.view, 64, 64, &mut encoder, None)
        .unwrap();
    renderer.queue.submit(Some(encoder.finish()));
    targets.commit_submitted();
    let saved = pixels(&renderer, &targets);
    assert_eq!(&saved[..4], &[255, 0, 0, 255]);
    let p = project([3840., 2160.], [0., 1., 0., 1.], true);
    let frozen = serde_json::to_string(&p).unwrap();
    let next = scene(&p);
    let target = targets.prepare(&renderer, 64, 64).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let error = renderer
        .encode(&next, &target.view, 64, 64, &mut encoder, None)
        .err()
        .unwrap();
    assert!(error.resource_rejection().is_some());
    drop(encoder);
    targets.discard_candidate();
    renderer.reclaim_preview_working_set().unwrap();
    assert_eq!(saved, pixels(&renderer, &targets));
    // Formal capture still refuses the original full-layer scratch requirement.
    let capture = renderer.capture_target(64, 64).unwrap();
    assert!(renderer
        .capture(&next, &capture)
        .err()
        .unwrap()
        .resource_rejection()
        .is_some());
    for _ in 0..4 {
        let target = targets.prepare(&renderer, 16, 16).unwrap();
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        renderer
            .encode_preview(&next, &target.view, 16, 16, &mut encoder, None)
            .unwrap();
        renderer.queue.submit(Some(encoder.finish()));
        targets.commit_submitted();
    }
    assert_eq!(targets.texture_bytes(), 2 * 16 * 16 * 4);
    assert_eq!(&pixels(&renderer, &targets)[..4], &[0, 255, 0, 255]);
    assert_eq!(serde_json::to_string(&p).unwrap(), frozen);
    let before = targets.texture_bytes();
    assert!(targets
        .prepare(&renderer, u32::MAX, u32::MAX)
        .err()
        .unwrap()
        .resource_rejection()
        .is_some());
    assert_eq!(targets.texture_bytes(), before);
    targets.invalidate();
    assert!(!targets.valid());
}
#[test]
fn guarded_preview_refuses_resource_and_shader_errors_while_legacy_diagnostics_remain_available() {
    let registry = motion_effects::Registry::new_with_builtins().unwrap();
    let mut builder = PlanBuilder::new(registry).unwrap();
    let huge = scene(&project([3840., 2160.], [1.; 4], true));
    assert!(!builder
        .build_preview_checked(&huge, &[0], 64, 64)
        .unwrap()
        .diagnostics
        .is_empty());
    assert!(builder
        .build_preview_guarded(&huge, &[0], 64, 64, false)
        .err()
        .unwrap()
        .resource_rejection()
        .is_some());
    let small = scene(&project([64.; 2], [1.; 4], true));
    builder
        .build_preview_guarded(&small, &[0], 64, 64, true)
        .unwrap();
    let id = builder
        .frame
        .passes
        .iter()
        .find(|p| builder.programs[p.program as usize].package.is_some())
        .unwrap()
        .program;
    builder
        .program_errors
        .insert(id, "actual shader rejection".into());
    let error = builder
        .build_preview_guarded(&small, &[0], 64, 64, true)
        .err()
        .unwrap();
    assert!(error.to_string().contains("shader rejection"));
    assert!(error.resource_rejection().is_none());
}
