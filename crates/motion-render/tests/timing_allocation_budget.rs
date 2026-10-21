use motion_render::{AllocationBudget, GpuTimer, Renderer, FIXED_RENDERER_ALLOCATION_BYTES};

#[test]
fn timer_buffers_share_renderer_admission_and_retire_after_teardown() {
    let budget = AllocationBudget::new(8 << 20).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = pollster::block_on(Renderer::new_profiled_with_allocation_budget(
        &instance, None, wgpu::TextureFormat::Rgba8UnormSrgb, true, budget.clone(),
    )).unwrap();
    // Four 256-byte resolve slots and four 32-byte timestamp readbacks.
    // Query-set/driver metadata is not a buffer descriptor capacity.
    let timer = GpuTimer::new(&renderer.device, &renderer.queue)
        .expect("the validation GPU must support timestamp queries");
    assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES + 4 * 256 + 4 * 32);
    assert_eq!(budget.info().unwrap().live_allocations, 11);
    drop(timer);
    assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES);
    let device = renderer.device.clone();
    drop(renderer);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
}
