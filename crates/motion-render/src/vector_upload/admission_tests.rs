use super::*;

#[test]
fn vector_destination_and_staging_admit_actual_rounded_capacity() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let pool = UploadPool::default();
    let mut destination = ReusableBuffer::default();
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
