use super::Presenter;
use crate::{
    AllocationBudgetInfo, AllocationCategory, AllocationFrameOutcome, FrameOperation, Renderer,
    ResourceBudgetScope, ResourceRejection, Scene,
};

fn uniforms(info: &AllocationBudgetInfo) -> u64 {
    info.categories
        .iter()
        .find(|entry| entry.category == AllocationCategory::UniformBuffers)
        .unwrap()
        .admitted_bytes
}
fn source(renderer: &mut Renderer) -> crate::RenderTarget {
    let mut project = motion_core::Project::new(8, 8, 30, 1).unwrap();
    project.background = [0., 1., 0., 1.];
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let target = renderer.render_target(8, 8).unwrap();
    renderer.draw(&scene, &target.view, 8, 8).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    target
}

#[test]
fn native_presenter_refuses_second_uniform_atomically_then_admits_exact_capacity() {
    fn is_send<T: Send>() {}
    is_send::<Presenter>();
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let target = renderer.render_target(1, 1).unwrap();
    let before = budget.info().unwrap();
    budget.set_limit(before.admitted_bytes + 63).unwrap();
    let error = Presenter::new_admitted(&renderer, &target.view, renderer.target_format)
        .err()
        .unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + 64,
            budget_bytes: before.admitted_bytes + 63,
        })
    );
    assert!(error.to_string().contains("canvas navigation"));
    let refused = budget.info().unwrap();
    assert_eq!(refused.admitted_bytes, before.admitted_bytes);
    assert_eq!(refused.live_allocations, before.live_allocations);
    assert_eq!(uniforms(&refused), uniforms(&before));
    budget.set_limit(before.admitted_bytes + 64).unwrap();
    let presenter =
        Presenter::new_admitted(&renderer, &target.view, renderer.target_format).unwrap();
    let created = budget.info().unwrap();
    assert_eq!(created.admitted_bytes, before.admitted_bytes + 64);
    assert_eq!(created.live_allocations, before.live_allocations + 2);
    assert_eq!(uniforms(&created), uniforms(&before) + 64);
    drop(presenter);
    let dropped = budget.info().unwrap();
    assert_eq!(dropped.admitted_bytes, before.admitted_bytes);
    assert_eq!(dropped.live_allocations, before.live_allocations);
}

#[test]
fn actual_navigation_pixels_and_dropped_presenter_share_real_frame_retirement() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = source(&mut renderer);
    let target = renderer.capture_target(16, 8).unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    let frame = budget
        .begin_frame(9981, FrameOperation::Present)
        .unwrap()
        .unwrap();
    let presenter =
        Presenter::new_admitted(&renderer, &source.view, renderer.target_format).unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    presenter
        .encode_view(
            &renderer.queue,
            &mut encoder,
            &target.view,
            None,
            [0., 0., 16., 8.],
            Some([4., 0., 8., 8.]),
            [1., 0., 0., 1.],
        )
        .unwrap();
    let encoded = budget.info().unwrap();
    assert_eq!(encoded.admitted_bytes, before.admitted_bytes + 64);
    drop(presenter);
    drop(source);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        encoded.admitted_bytes,
        "encoded commands and queued navigation upload must retain the buffers and source"
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    frame.finish(AllocationFrameOutcome::Rendered);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    // Readback is a separate operation after this Present window has retired.
    let pixels = renderer.read_target(&target).unwrap();
    for y in 0..8usize {
        for x in 0..16usize {
            assert_eq!(
                &pixels[(y * 16 + x) * 4..(y * 16 + x + 1) * 4],
                if (4..12).contains(&x) {
                    &[0, 255, 0, 255]
                } else {
                    &[255, 0, 0, 255]
                }
            );
        }
    }
    let retired = budget.info().unwrap();
    assert_eq!(retired.admitted_bytes, before.admitted_bytes - 256);
    assert_eq!(retired.live_allocations, before.live_allocations - 1);
    assert_eq!(uniforms(&retired), uniforms(&before));
    let windows = budget.frame_peaks().unwrap();
    assert!(windows.active.is_empty());
    let frame = windows
        .recent
        .iter()
        .find(|frame| frame.request == 9981)
        .unwrap();
    assert_eq!(frame.peak_admitted_bytes, before.admitted_bytes + 64);
    assert_eq!(frame.end_admitted_bytes, Some(before.admitted_bytes - 256));
    let uniform = frame
        .categories
        .iter()
        .find(|entry| entry.category == AllocationCategory::UniformBuffers)
        .unwrap();
    assert_eq!(uniform.start_admitted_bytes, uniforms(&before));
    assert_eq!(uniform.peak_admitted_bytes, uniforms(&before) + 64);
    assert_eq!(uniform.end_admitted_bytes, Some(uniforms(&before)));
}

#[test]
fn unsubmitted_navigation_write_outlives_presenter_without_a_command_scope() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = source(&mut renderer);
    let target = renderer.capture_target(16, 8).unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    let presenter =
        Presenter::new_admitted(&renderer, &source.view, renderer.target_format).unwrap();
    // Abandon a raw encoder so only the queued write journal, rather than the
    // allocated encoder's conservative leases, can retain the navigation buffer.
    let mut raw = renderer.device.create_command_encoder(&Default::default());
    presenter
        .encode_view(
            &renderer.queue,
            &mut raw,
            &target.view,
            None,
            [0., 0., 16., 8.],
            Some([4., 0., 8., 8.]),
            [1., 0., 0., 1.],
        )
        .unwrap();
    drop(raw);
    drop(presenter);
    drop(source);
    let staged = budget.info().unwrap();
    assert_eq!(staged.admitted_bytes, before.admitted_bytes - 256 + 32);
    assert_eq!(uniforms(&staged), uniforms(&before) + 32);
    let empty = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(empty.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let retired = budget.info().unwrap();
    assert_eq!(retired.admitted_bytes, before.admitted_bytes - 256);
    assert_eq!(uniforms(&retired), uniforms(&before));
}
