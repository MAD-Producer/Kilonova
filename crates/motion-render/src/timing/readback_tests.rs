use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cancelled_mapping_counts_callback_error_once_without_renderer_fault() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = pollster::block_on(crate::Renderer::new_profiled(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        true,
    ))
    .unwrap();
    let mut timer = GpuTimer::new(&renderer)
        .unwrap()
        .expect("validation GPU requires timestamps");
    let slot = timer.begin(11).unwrap();
    timer.map(slot);
    let pending = timer.readback_info();
    assert_eq!(pending.map_requests, 1);
    assert_eq!(pending.map_successes, 0);
    assert_eq!(pending.map_failures, 0);

    // Cancel the actual wgpu mapping before polling. Do not fabricate its
    // callback outcome or the timer's ready/error state.
    timer.slots[slot].buffer.unmap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let cancelled = timer.readback_info();
    assert_eq!(cancelled.map_requests, 1);
    assert_eq!(cancelled.map_successes, 0);
    assert_eq!(cancelled.map_failures, 1);
    assert_eq!(cancelled.mapped_buffer_bytes, 0);
    assert_eq!(cancelled.invalid_samples, 0);
    assert!(timer.collect().into_iter().all(|timing| timing.is_none()));
    let collected = timer.readback_info();
    assert_eq!(collected.invalid_samples, 1);
    assert_eq!(collected.valid_samples, 0);
    assert!(timer.collect().into_iter().all(|timing| timing.is_none()));
    assert_eq!(timer.readback_info(), collected);
    renderer.check_health().unwrap();

    // A real successful map after cancellation keeps the lifetime error count.
    // No timestamp was encoded, so zero-initialized words are not GPU timing.
    let slot = timer.begin(12).unwrap();
    timer.map(slot);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert!(timer.collect().into_iter().all(|timing| timing.is_none()));
    assert_eq!(
        timer.readback_info(),
        crate::TimestampReadbackInfo {
            map_requests: 2,
            map_successes: 1,
            map_failures: 1,
            mapped_buffer_bytes: TIMESTAMP_BUFFER_BYTES as u64,
            invalid_samples: 2,
            ..Default::default()
        }
    );
    renderer.check_health().unwrap();
    assert_eq!(
        GpuTimer::new(&renderer).unwrap().unwrap().readback_info(),
        crate::TimestampReadbackInfo::default()
    );
}
