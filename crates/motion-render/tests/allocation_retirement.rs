//! Completed work must not pin retired targets through unrelated encoders.
use motion_render::{AllocationBudget, Renderer, FIXED_RENDERER_ALLOCATION_BYTES};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn overlapping_frames_do_not_renew_completed_target_reservations() {
    let budget = AllocationBudget::new(FIXED_RENDERER_ALLOCATION_BYTES + 8 * 256).unwrap();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let mut current = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    let mut samples = Vec::new();
    for frame in 0..4 {
        let target = renderer.render_target(8, 8).unwrap();
        {
            let _pass = current.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        drop(target);
        let next = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        renderer.submit_allocated(current.finish(), |_| {}).unwrap();
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        current = next;
        let used = budget.info().unwrap().admitted_bytes;
        samples.push((frame, used));
    }
    drop(current);
    assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES);
    assert!(
        samples.iter().all(|&(_, used)| used == FIXED_RENDERER_ALLOCATION_BYTES),
        "completed transient targets have no user/GPU bindings in the next frame: {samples:?}"
    );
}
