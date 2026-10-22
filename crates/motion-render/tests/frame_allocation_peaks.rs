//! Actual resource and queue-completion coverage for the shared frame windows.
use motion_render::{
    AllocationBudget, AllocationFrameOutcome as Outcome, FrameOperation, Renderer,
    FIXED_RENDERER_ALLOCATION_BYTES as FIXED,
};

#[test]
fn actual_buffers_uploads_and_allocated_submission_contribute_until_retirement() {
    let budget = AllocationBudget::new(FIXED + 16).unwrap();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let context = renderer.allocation_context();
    for (request, capacity) in [(71, 16), (72, 4)] {
        let frame = budget
            .begin_frame(request, FrameOperation::Render)
            .unwrap()
            .unwrap();
        let data = vec![0; capacity];
        let buffer = context
            .buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("frame transient destination"),
                contents: &data,
                usage: wgpu::BufferUsages::COPY_DST,
            })
            .unwrap();
        context.write_buffer(&buffer, 0, &data).unwrap();
        let encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        drop(buffer);
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            FIXED + capacity as u64
        );
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        frame.finish(Outcome::Rendered);
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let snapshot = budget.frame_peaks().unwrap();
        assert!(snapshot.active.is_empty());
        let completed = snapshot.recent.last().unwrap();
        assert_eq!(completed.request, request);
        assert_eq!(completed.peak_admitted_bytes, FIXED + capacity as u64);
        assert_eq!(completed.end_admitted_bytes, Some(FIXED));
        assert_eq!(
            completed.submissions, 1,
            "do not count submit_allocated twice"
        );
        assert_eq!(completed.unconfirmed_submissions, 0);
        assert!(!snapshot.complete_coverage && !snapshot.is_physical_vram);
    }
    assert_eq!(budget.info().unwrap().peak_admitted_bytes, FIXED + 16);
}

#[test]
fn shared_renderers_and_tracked_raw_submission_keep_one_combined_peak() {
    let budget = AllocationBudget::new(2 * FIXED + 8).unwrap();
    let first =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let second =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let frame = budget
        .begin_frame(91, FrameOperation::Export)
        .unwrap()
        .unwrap();
    let context = second.allocation_context();
    let buffer = context
        .buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("other renderer frame buffer"),
            contents: &[0; 8],
            usage: wgpu::BufferUsages::COPY_DST,
        })
        .unwrap();
    context.write_buffer(&buffer, 0, &[0; 8]).unwrap();
    drop(buffer);
    let commands = second
        .device
        .create_command_encoder(&Default::default())
        .finish();
    second.submit_tracked(commands, |_| {});
    frame.finish(Outcome::Rendered);
    second
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    first
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let snapshot = budget.frame_peaks().unwrap();
    assert!(snapshot.active.is_empty());
    assert_eq!(snapshot.recent[0].start_admitted_bytes, 2 * FIXED);
    assert_eq!(snapshot.recent[0].peak_admitted_bytes, 2 * FIXED + 8);
    assert_eq!(snapshot.recent[0].end_admitted_bytes, Some(2 * FIXED));
    assert_eq!(snapshot.recent[0].submissions, 1);
    assert_eq!(snapshot.unconfirmed_submissions, 0);
}
