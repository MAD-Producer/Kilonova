use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn vector_destination_and_staging_admit_actual_rounded_capacity() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let pool = UploadPool::new(budget.clone());
    let mut destination = ReusableBuffer::new(budget.clone());
    let target = destination.get(&renderer.device, 24,
        wgpu::BufferUsages::VERTEX, "vector admission control").unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    pool.write(&renderer.device, &mut encoder, &target, &[1; 24], 71, 3.25).unwrap();
    // Each descriptor rounds 24 bytes up to 32: destination plus staging.
    assert_eq!(budget.info().unwrap().admitted_bytes, crate::FIXED_RENDERER_ALLOCATION_BYTES + 64);
    assert_eq!(budget.info().unwrap().live_allocations, 8);
    drop(encoder);
    drop(target);
    destination.clear();
    pool.release();
    assert_eq!(budget.info().unwrap().admitted_bytes, crate::FIXED_RENDERER_ALLOCATION_BYTES);
}

fn assert_budget(error: &RenderError, required: u64, limit: u64) {
    assert_eq!(error.resource_rejection(), Some(&crate::ResourceRejection::Budget {
        scope: crate::ResourceBudgetScope::RenderAllocations,
        required_bytes: required, budget_bytes: limit,
    }));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn refused_destination_growth_preserves_aliases_and_reuse_below_a_lowered_limit() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let base = crate::FIXED_RENDERER_ALLOCATION_BYTES;
    let mut destination = ReusableBuffer::new(budget.clone());
    let old = destination.get(&renderer.device, 4, wgpu::BufferUsages::VERTEX, "old vector").unwrap();
    let id = old.allocation().id();
    budget.set_limit(base + 19).unwrap();
    let error = destination.get(&renderer.device, 12, wgpu::BufferUsages::VERTEX, "growth refused").err().unwrap();
    assert_budget(&error, base + 20, base + 19);
    assert!(error.to_string().contains("buffer size=16 phase=allocate label=growth refused"));
    assert_eq!(destination.buffer.as_ref().unwrap().allocation().id(), id);
    assert_eq!((destination.bytes(), destination.created), (4, 1));
    budget.set_limit(base).unwrap();
    let reused = destination.get(&renderer.device, 4, wgpu::BufferUsages::VERTEX, "reuse").unwrap();
    assert_eq!(reused.allocation().id(), id);
    assert_eq!(destination.created, 1);
    drop(reused);
    budget.set_limit(base + 20).unwrap();
    let grown = destination.get(&renderer.device, 12, wgpu::BufferUsages::VERTEX, "growth allowed").unwrap();
    let alias = grown.clone();
    assert_eq!((grown.size(), destination.created), (16, 2));
    assert_eq!(budget.info().unwrap().admitted_bytes, base + 20);
    destination.clear();
    drop(old);
    assert_eq!(budget.info().unwrap().admitted_bytes, base + 16);
    drop(grown);
    assert_eq!(budget.info().unwrap().admitted_bytes, base + 16);
    drop(alias);
    assert_eq!(budget.info().unwrap().admitted_bytes, base);
}

fn read_destination(renderer: &crate::Renderer, target: &AllocationBuffer) -> Vec<u8> {
    // Test-only readback is external to the pool under examination.
    let readback = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vector admission readback"), size: 24,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(target, 0, &readback, 0, 24);
    renderer.queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |result| { send.send(result).unwrap(); });
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv_timeout(std::time::Duration::from_secs(5)).unwrap().unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap().to_vec();
    readback.unmap();
    bytes
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn staging_refusal_is_atomic_and_cached_reuse_needs_no_new_capacity() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let base = crate::FIXED_RENDERER_ALLOCATION_BYTES;
    let pool = UploadPool::new(budget.clone());
    let mut destination = ReusableBuffer::new(budget.clone());
    let target = destination.get(&renderer.device, 24, wgpu::BufferUsages::COPY_SRC, "vector copy destination").unwrap();
    let id = target.allocation().id();
    budget.set_limit(base + 63).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let failure = pool.write(&renderer.device, &mut encoder, &target, &[7; 24], 72, 3.5).unwrap_err();
    assert_budget(&failure, base + 64, base + 63);
    for context in ["phase=allocate layer=72 frame=3.5 upload_bytes=24", "buffer size=32 phase=allocate label=pooled vector upload staging"] {
        assert!(failure.to_string().contains(context), "{failure}");
    }
    assert_eq!(target.allocation().id(), id);
    let refused = pool.info().unwrap();
    assert_eq!((refused.staging_buffers_created_total, refused.pending_buffers, refused.idle_bytes, refused.encoded_upload_bytes_total), (0, 0, 0, 0));
    assert_eq!(budget.info().unwrap().admitted_bytes, base + 32);
    budget.set_limit(base + 64).unwrap();
    pool.write(&renderer.device, &mut encoder, &target, &[7; 24], 72, 3.5).unwrap();
    renderer.queue.submit([encoder.finish()]);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(read_destination(&renderer, &target), vec![7; 24]);
    assert_eq!(pool.info().unwrap().idle_bytes, 32);
    budget.set_limit(base + 1).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    pool.write(&renderer.device, &mut encoder, &target, &[9; 24], 72, 4.).unwrap();
    renderer.queue.submit([encoder.finish()]);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(read_destination(&renderer, &target), vec![9; 24]);
    let reused = pool.info().unwrap();
    assert_eq!((reused.staging_buffers_created_total, reused.reused_uploads_total, reused.pending_buffers, reused.completed_upload_bytes_total), (1, 1, 0, 48));
    pool.release();
    destination.clear();
    drop(target);
    assert_eq!(budget.info().unwrap().admitted_bytes, base);
    assert!(renderer.gpu_error().is_none());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn raw_abandoned_commands_retain_both_buffers_after_cache_and_pool_drop() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let base = crate::FIXED_RENDERER_ALLOCATION_BYTES;
    for finish in [false, true] {
        let pool = UploadPool::new(budget.clone());
        let mut destination = ReusableBuffer::new(budget.clone());
        let target = destination.get(&renderer.device, 24, wgpu::BufferUsages::VERTEX, "abandoned vector").unwrap();
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        pool.write(&renderer.device, &mut encoder, &target, &[3; 24], 73, 2.).unwrap();
        drop(target);
        destination.clear();
        drop(pool);
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, base + 64);
        if finish { drop(encoder.finish()); } else { drop(encoder); }
        assert_eq!(budget.info().unwrap().admitted_bytes, base);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn completed_uploads_do_not_renew_retired_destinations_in_unrelated_scopes() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let base = crate::FIXED_RENDERER_ALLOCATION_BYTES;
    let pool = UploadPool::new(budget.clone());
    let mut destination = ReusableBuffer::new(budget.clone());
    let target = destination.get(&renderer.device, 24, wgpu::BufferUsages::VERTEX, "retired vector").unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    pool.write(&renderer.device, &mut encoder, &target, &[4; 24], 74, 3.).unwrap();
    drop(target);
    destination.clear();
    pool.release();
    // The mapping callback still owns its staging buffer; the destination is
    // retained only for work already encoded, never for this unrelated scope.
    let unrelated = renderer.allocation_command_encoder(&Default::default()).unwrap();
    renderer.queue.submit([encoder.finish()]);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, base + 32);
    let info = pool.info().unwrap();
    assert_eq!((info.pending_buffers, info.idle_buffers, info.completed_upload_bytes_total), (0, 0, 24));
    let successor = renderer.allocation_command_encoder(&Default::default()).unwrap();
    drop(unrelated);
    assert_eq!(budget.info().unwrap().admitted_bytes, base);
    drop(successor);
    assert!(renderer.gpu_error().is_none());
}
