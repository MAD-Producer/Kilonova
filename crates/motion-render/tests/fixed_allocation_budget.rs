use motion_render::{AllocationBudget, Renderer, ResourceBudgetScope, ResourceRejection};

// Independently count descriptor capacities: 128 aligned draw uniforms, 65,536
// planar vertices, three clear vertices, two white texels and one identity LUT.
// Instance data belongs to on-demand SDK rendering, never the startup baseline.
const FIXED_BYTES: u64 = 128 * 256 + 65_536 * 20 + 3 * 24 + 2 * 4 + 256 * 4;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn fixed_resources_are_admitted_and_retire_after_renderer_teardown() {
    let budget = AllocationBudget::new(8 << 20).unwrap();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let info = budget.info().unwrap();
    assert_eq!(info.admitted_bytes, FIXED_BYTES);
    assert_eq!(info.live_allocations, 6);
    assert_eq!(motion_render::FIXED_RENDERER_ALLOCATION_BYTES, FIXED_BYTES);
    let device = renderer.device.clone();
    let queue = renderer.queue.clone();
    drop(renderer);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
    assert_eq!(budget.info().unwrap().live_allocations, 0);
    drop(queue);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn each_fixed_resource_refuses_before_upload_and_rolls_back_partial_startup() {
    for (required, context) in [
        (4, "image key=Static(0) size=1x1 phase=allocate"),
        (32_772, "label=Motion Studio reusable layer parameters"),
        (1_343_492, "label=Motion Studio bounded planar geometry"),
        (1_343_564, "label=transparent vector scratch clear"),
        (1_343_568, "effect fallback size=1x1 phase=allocate"),
        (1_344_592, "effect identity LUT size=256x1 phase=allocate"),
    ] {
        let budget = AllocationBudget::new(required - 1).unwrap();
        let error = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone()))
            .err().unwrap();
        assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: required, budget_bytes: required - 1,
        }));
        assert!(error.to_string().contains(context), "{error}");
        // No poll, extra submit or retry may be needed after failed startup.
        assert_eq!(budget.info().unwrap().admitted_bytes, 0, "{context}");
        assert_eq!(budget.info().unwrap().live_allocations, 0, "{context}");
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn abandoned_commands_keep_all_fixed_resources_after_renderer_and_gpu_completion() {
    for finished in [false, true] {
        let budget = AllocationBudget::new(FIXED_BYTES).unwrap();
        let renderer = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
        let encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
        let device = renderer.device.clone();
        let queue = renderer.queue.clone();
        drop(renderer);
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_BYTES);
        assert_eq!(budget.info().unwrap().live_allocations, 6);
        if finished { drop(encoder.finish()); } else { drop(encoder); }
        assert_eq!(budget.info().unwrap().admitted_bytes, 0);
        assert_eq!(budget.info().unwrap().live_allocations, 0);
        drop(queue);
    }
}
