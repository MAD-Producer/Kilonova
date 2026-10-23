const SYSTEM_BYTES: u64 = motion_render::FIXED_RENDERER_ALLOCATION_BYTES;

use motion_core::Project;
use motion_render::{AllocationBudget, Renderer, ResourceBudgetScope, ResourceRejection, Scene};

fn renderer(budget: &AllocationBudget) -> Renderer {
    pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap()
}
fn wait(renderer: &Renderer) {
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
}
fn background(color: [f32; 4]) -> Scene {
    let mut project = Project::new(2, 2, 30, 1).unwrap();
    project.background = color;
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn sample_mean_and_uniform_refusals_roll_back_both_budgets_then_recover() {
    let budget = AllocationBudget::new(SYSTEM_BYTES + 28).unwrap();
    let mut renderer = renderer(&budget);
    // A 1x1 exposure adds sample4 + mean8 + uniform16 to fixed renderer resources.
    for (limit, required) in [(SYSTEM_BYTES + 3, SYSTEM_BYTES + 4), (SYSTEM_BYTES + 11, SYSTEM_BYTES + 12), (SYSTEM_BYTES + 27, SYSTEM_BYTES + 28)] {
        budget.set_limit(limit).unwrap();
        let error = renderer.exposure_target(1, 1, &[1.]).err().unwrap();
        assert_eq!(
            error.resource_rejection(),
            Some(&ResourceRejection::Budget {
                scope: ResourceBudgetScope::RenderAllocations,
                required_bytes: required,
                budget_bytes: limit,
            })
        );
        assert!(
            error.to_string().contains("exposure 1x1 phase=allocate"),
            "{error}"
        );
        if required == SYSTEM_BYTES + 28 {
            assert!(
                error
                    .to_string()
                    .contains("buffer size=16 phase=allocate label=exposure phase weight"),
                "{error}"
            );
        }
        assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
        assert_eq!(budget.info().unwrap().live_allocations, 6);
        assert_eq!(renderer.exposure_texture_bytes(), 0);
    }
    budget.set_limit(SYSTEM_BYTES + 28).unwrap();
    let target = renderer.exposure_target(1, 1, &[1.]).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 28);
    assert_eq!(budget.info().unwrap().live_allocations, 9);
    assert_eq!(target.texture_resource_ledger().total_known_bytes(), 12);
    assert_eq!(
        target.texture_bytes(),
        12,
        "texture subtotal must exclude the uniform"
    );
    assert_eq!(renderer.exposure_texture_bytes(), 12);
    drop(target);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn abandoned_exposure_encoders_keep_all_resources_until_their_actual_owner_drops() {
    let budget = AllocationBudget::new(SYSTEM_BYTES + 28).unwrap();
    let mut renderer = renderer(&budget);
    for finished in [false, true] {
        let target = renderer.exposure_target(1, 1, &[1.]).unwrap();
        let encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        drop(target);
        assert_eq!(
            renderer.exposure_texture_bytes(),
            0,
            "caller no longer owns a target"
        );
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            SYSTEM_BYTES + 28,
            "encoder still owns all allocations"
        );
        if finished {
            let buffer = encoder.finish();
            assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 28);
            drop(buffer);
        } else {
            drop(encoder);
        }
        assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    }
    let target = renderer.exposure_target(1, 1, &[1.]).unwrap();
    let device = renderer.device.clone();
    drop(renderer);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 28);
    drop(target);
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
    assert_eq!(budget.info().unwrap().live_allocations, 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn failed_overlapping_exposure_preserves_completed_phase_pixels_and_real_retirement() {
    let budget = AllocationBudget::new(SYSTEM_BYTES + 2044).unwrap();
    let mut renderer = renderer(&budget);
    let mut target = renderer.exposure_target(2, 2, &[0.5, 0.5]).unwrap();
    renderer
        .accumulate_exposure(&mut target, Some(&background([1., 0., 0., 1.])), false)
        .unwrap();
    wait(&renderer);
    assert_eq!(target.phase_index(), 1);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 64);
    budget.set_limit(SYSTEM_BYTES + 127).unwrap();
    let error = renderer.exposure_target(2, 2, &[1.]).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: SYSTEM_BYTES + 128,
            budget_bytes: SYSTEM_BYTES + 127,
        })
    );
    assert_eq!(target.phase_index(), 1);
    assert_eq!(renderer.exposure_texture_bytes(), 48);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 64);
    budget.set_limit(SYSTEM_BYTES + 2044).unwrap();
    renderer
        .accumulate_exposure(&mut target, Some(&background([0., 0., 1., 1.])), false)
        .unwrap();
    let pixels = renderer.capture_exposure(&target).unwrap();
    assert!(target.complete());
    assert!(
        pixels.chunks_exact(4).all(|pixel| pixel
            .iter()
            .zip([188u8, 0, 188, 255])
            .all(|(a, b)| a.abs_diff(b) <= 1)),
        "{pixels:?}"
    );
    wait(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 64);
    drop(target);
    wait(&renderer);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    assert_eq!(budget.info().unwrap().live_allocations, 6);
}
