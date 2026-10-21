use super::*;

#[test]
fn matte_and_transfer_system_resources_share_renderer_admission() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let before = budget.info().unwrap();
    let gpu = crate::compositing_gpu::CompositingGpu::new(&renderer.device,
        &renderer.queue, &renderer.uniform_layout, &renderer.image_layout,
        &renderer.sampler, renderer.target_format);
    let actual_bytes = gpu.extract_buffer.size() + gpu.blend_buffer.size() + 1;
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + actual_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations + 3);
    drop(gpu);
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.submit_tracked(encoder.finish(), |_| {});
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
    renderer.check_health().unwrap();
}
