use super::*;

#[test]
fn pending_timestamp_mapping_owns_the_buffer_until_its_actual_callback() {
    let budget = crate::AllocationBudget::new(8 << 20).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = pollster::block_on(crate::Renderer::new_profiled_with_allocation_budget(
        &instance, None, wgpu::TextureFormat::Rgba8UnormSrgb, true, budget.clone(),
    )).unwrap();
    let mut timer = GpuTimer::new(&renderer).unwrap().unwrap();
    let slot = timer.begin(91).unwrap();
    let words = [11u64, 22, 33, 44].into_iter().flat_map(u64::to_le_bytes).collect::<Vec<_>>();
    renderer.queue.write_buffer(&timer.slots[slot].buffer, 0, &words);
    renderer.queue.submit([]);
    let state = timer.slots[slot].state.clone();
    timer.map(slot);
    drop(timer);
    // Keep the pending raw buffer and exactly its capacity, not the timer's
    // resolve buffer or three unrelated readback slots.
    assert_eq!(state.load(Ordering::Acquire), 1);
    assert_eq!(budget.info().unwrap().admitted_bytes, crate::FIXED_RENDERER_ALLOCATION_BYTES + 32);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(state.load(Ordering::Acquire), 2);
    assert_eq!(budget.info().unwrap().admitted_bytes, crate::FIXED_RENDERER_ALLOCATION_BYTES);
    assert!(renderer.gpu_error().is_none());
}
