use motion_render::{AllocationBudget, Renderer};

// Independently count descriptor capacities: 128 aligned draw uniforms, 65,536
// planar vertices, three clear vertices, two white texels, one identity LUT,
// and 65,536 sprite instances. This must fail if any fixed resource is omitted.
const FIXED_BYTES: u64 = 128 * 256 + 65_536 * 20 + 3 * 24 + 2 * 4 + 256 * 4 + 65_536 * 48;

#[test]
fn fixed_resources_are_admitted_and_retire_after_renderer_teardown() {
    let budget = AllocationBudget::new(8 << 20).unwrap();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let info = budget.info().unwrap();
    assert_eq!(info.admitted_bytes, FIXED_BYTES);
    assert_eq!(info.live_allocations, 7);
    let device = renderer.device.clone();
    let queue = renderer.queue.clone();
    drop(renderer);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
    assert_eq!(budget.info().unwrap().live_allocations, 0);
    drop(queue);
}
