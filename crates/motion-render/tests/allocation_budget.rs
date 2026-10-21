use motion_core::Project;
use motion_render::{
    AllocationBudget, Presenter, Renderer, ResourceBudgetScope, ResourceRejection, Scene,
};

// The renderer-owned 1x1 white source is now admitted as well.
const SYSTEM_BYTES: u64 = 4;

fn renderer(budget: &AllocationBudget) -> Renderer {
    pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap()
}

#[test]
fn offscreen_views_and_multiple_renderers_share_actual_capacity_and_capture_failure_rolls_back() {
    let budget = AllocationBudget::new(1024 + 2 * SYSTEM_BYTES).unwrap();
    let first = renderer(&budget);
    let second = renderer(&budget);
    assert_eq!(budget.info().unwrap().admitted_bytes, 2 * SYSTEM_BYTES);
    let target = first.render_target(8, 8).unwrap();
    let view = target.view.clone();
    assert_eq!(target.texture_bytes(), 256);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        256 + 2 * SYSTEM_BYTES
    );
    drop(target);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        256 + 2 * SYSTEM_BYTES
    );

    // A second texture fits, but its 256-byte-aligned readback does not. Both
    // temporary texture references must retire when capture_target returns Err.
    let error = second.capture_target(8, 8).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: 256 + 256 + 8 * 256 + 2 * SYSTEM_BYTES,
            budget_bytes: 1024 + 2 * SYSTEM_BYTES,
        })
    );
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        256 + 2 * SYSTEM_BYTES
    );
    assert_eq!(budget.info().unwrap().live_allocations, 3);
    let smaller = second.render_target(4, 4).unwrap();
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        320 + 2 * SYSTEM_BYTES
    );
    drop(smaller);
    drop(view);
    assert_eq!(budget.info().unwrap().admitted_bytes, 2 * SYSTEM_BYTES);
    let invalid = second.render_target(0, 8).err().unwrap();
    assert!(invalid.resource_rejection().is_none());
    assert_eq!(budget.info().unwrap().admitted_bytes, 2 * SYSTEM_BYTES);
}

#[test]
fn unsubmitted_finished_and_submitted_commands_hold_admission_without_diagnostics() {
    let budget = AllocationBudget::new(256 + SYSTEM_BYTES).unwrap();
    let renderer = renderer(&budget);
    let mut encoder = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    // Admit after opening the encoder: retaining only a creation-time snapshot
    // would miss this allocation and incorrectly permit its replacement.
    let target = renderer.render_target(8, 8).unwrap();
    {
        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
    assert_eq!(budget.info().unwrap().admitted_bytes, 256 + SYSTEM_BYTES);
    assert!(renderer.render_target(1, 1).is_err());
    let commands = encoder.finish();
    assert_eq!(budget.info().unwrap().admitted_bytes, 256 + SYSTEM_BYTES);
    // This empty, unrelated encoder has no resource handle for the old target.
    // It must not renew a reservation held only by the finished command. The
    // explicit GPU wait keeps this independent of callback timing.
    let unfinished = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    renderer.submit_allocated(commands, |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let completed = budget.info().unwrap();
    assert_eq!(completed.admitted_bytes, SYSTEM_BYTES, "{completed:?}");
    drop(unfinished);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    assert_eq!(budget.info().unwrap().encoder_scopes, 0);

    let target = renderer.render_target(8, 8).unwrap();
    let encoder = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    drop(target);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
}

#[test]
fn abandoned_commands_and_cross_budget_submission_release_without_gpu_submission() {
    let budget = AllocationBudget::new(256 + SYSTEM_BYTES).unwrap();
    let first = renderer(&budget);
    let other_budget = AllocationBudget::new(256 + SYSTEM_BYTES).unwrap();
    let second = renderer(&other_budget);
    let target = first.render_target(8, 8).unwrap();
    let encoder = first
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    drop(target);
    let error = second
        .submit_allocated(encoder.finish(), |_| {
            panic!("must refuse before submission")
        })
        .err()
        .unwrap();
    assert!(error.to_string().contains("another allocation budget"));
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    assert_eq!(other_budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    let target = first.render_target(8, 8).unwrap();
    let encoder = first
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    drop(target);
    drop(encoder);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);

    let same_budget_other_queue = renderer(&budget);
    budget.set_limit(256 + 2 * SYSTEM_BYTES).unwrap();
    let target = first.render_target(8, 8).unwrap();
    let encoder = first
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    drop(target);
    let error = same_budget_other_queue
        .submit_allocated(encoder.finish(), |_| panic!("must refuse another queue"))
        .err()
        .unwrap();
    assert!(error.to_string().contains("another render queue"));
    assert_eq!(budget.info().unwrap().admitted_bytes, 2 * SYSTEM_BYTES);
}

#[test]
fn encoder_metadata_has_a_hard_bound_and_recovers_after_abandonment() {
    let budget = AllocationBudget::new(256 + SYSTEM_BYTES).unwrap();
    let renderer = renderer(&budget);
    let mut scopes = Vec::new();
    for _ in 0..64 {
        scopes.push(
            renderer
                .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
                .unwrap(),
        );
    }
    let error = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .err()
        .unwrap();
    assert!(error.to_string().contains("too many pending"));
    assert_eq!(budget.info().unwrap().encoder_scopes, 64);
    scopes.pop();
    let replacement = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    assert_eq!(budget.info().unwrap().encoder_scopes, 64);
    drop(replacement);
    drop(scopes);
    assert_eq!(budget.info().unwrap().encoder_scopes, 0);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
}

#[test]
fn lowered_limit_keeps_presenter_source_and_allows_retained_presentation_without_new_allocations() {
    let budget = AllocationBudget::new(256 + SYSTEM_BYTES).unwrap();
    let mut renderer = renderer(&budget);
    let target = renderer.render_target(8, 8).unwrap();
    let mut project = Project::new(8, 8, 30, 1).unwrap();
    project.background = [0., 1., 0., 1.];
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.draw(&scene, &target.view, 8, 8).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let presenter = Presenter::new_admitted(&renderer, &target.view, renderer.target_format);
    drop(target);
    budget.set_limit(128 + SYSTEM_BYTES).unwrap();
    assert!(budget.check().is_err());
    assert_eq!(budget.info().unwrap().admitted_bytes, 256 + SYSTEM_BYTES);
    assert!(renderer.render_target(1, 1).is_err());
    // Retaining/presenting an already-admitted front does not allocate another
    // target. A lowered budget must not prevent chrome from using that front.
    // An inspection target stands in for the external swapchain. Neither it
    // nor the test-only readback is an admitted application offscreen target.
    let inspection = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test external presentation target"),
        size: wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: renderer.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let readback = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test retained frame inspection"),
        size: 8 * 256,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    presenter.encode(
        &mut encoder,
        &inspection.create_view(&Default::default()),
        None,
        None,
        [0., 0., 0., 1.],
    );
    encoder.copy_texture_to_buffer(
        inspection.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
        },
        wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (send, recv) = std::sync::mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    recv.recv().unwrap().unwrap();
    {
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        for row in mapped.chunks_exact(256) {
            assert!(row[..32]
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255]));
        }
    }
    readback.unmap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 256 + SYSTEM_BYTES);
    drop(presenter);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    budget.check().unwrap();
}

#[test]
fn admitted_targets_use_the_actual_sixteen_bit_float_format_capacity() {
    let budget = AllocationBudget::new(256 + SYSTEM_BYTES).unwrap();
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = pollster::block_on(Renderer::new_profiled_with_allocation_budget(
        &instance,
        None,
        wgpu::TextureFormat::Rgba16Float,
        false,
        budget.clone(),
    ))
    .unwrap();
    let error = renderer.render_target(8, 8).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: 512 + SYSTEM_BYTES,
            budget_bytes: 256 + SYSTEM_BYTES,
        })
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    let target = renderer.render_target(4, 4).unwrap();
    assert_eq!(target.texture_bytes(), 128);
    assert_eq!(
        target.view.texture().format(),
        wgpu::TextureFormat::Rgba16Float
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, 128 + SYSTEM_BYTES);
    drop(target);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
}

#[test]
fn admitted_capture_counts_row_aligned_readback_and_preserves_actual_pixels() {
    let budget = AllocationBudget::new(256 + 8 * 256 + SYSTEM_BYTES).unwrap();
    let mut renderer = renderer(&budget);
    let target = renderer.capture_target(8, 8).unwrap();
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        256 + 8 * 256 + SYSTEM_BYTES
    );
    assert!(renderer.render_target(1, 1).is_err());
    let mut project = Project::new(8, 8, 30, 1).unwrap();
    project.background = [0., 1., 0., 1.];
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let (pixels, _) = renderer.capture(&scene, &target).unwrap();
    assert!(pixels
        .chunks_exact(4)
        .all(|pixel| pixel == [0, 255, 0, 255]));
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        256 + 8 * 256 + SYSTEM_BYTES
    );
    drop(target);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
}
