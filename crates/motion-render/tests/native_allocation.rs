//! Real native resources, queue writes and command ownership, not counter mocks.
use motion_render::{
    AllocationBudget, AllocationContext, Renderer, ResourceBudgetScope, ResourceRejection,
    FIXED_RENDERER_ALLOCATION_BYTES as FIXED,
};

fn drain(renderer: &Renderer) {
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}
fn texture(
    context: &AllocationContext,
) -> Result<motion_render::AdmittedTexture, motion_render::RenderError> {
    context.texture(&wgpu::TextureDescriptor {
        label: Some("external test pixel"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
fn buffer(context: &AllocationContext) -> motion_render::AdmittedBuffer {
    context
        .buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("external padded buffer"),
            contents: &[1, 2, 3, 4, 5],
            usage: wgpu::BufferUsages::COPY_DST,
        })
        .unwrap()
}
fn write_pixel(context: &AllocationContext, pixel: &motion_render::AdmittedTexture) {
    context
        .write_texture(
            pixel,
            &[0, 255, 0, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        )
        .unwrap();
}

#[test]
fn padded_capacity_and_queued_uploads_share_the_live_allowance() {
    let budget = AllocationBudget::new(FIXED + 12).unwrap();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    drain(&renderer);
    let context = renderer.allocation_context();
    let buffer = buffer(&context);
    assert_eq!(
        buffer.size(),
        8,
        "charge the actual DeviceExt descriptor padding"
    );
    let pixel = texture(&context).unwrap();
    let view = pixel.view().unwrap();
    let alias = view.clone();
    assert_eq!(budget.info().unwrap().admitted_bytes, FIXED + 12);
    let scope = renderer
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let error = texture(&context).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: FIXED + 16,
            budget_bytes: FIXED + 12,
        })
    );
    assert!(pollster::block_on(scope.pop()).is_none());
    context.write_buffer(&buffer, 0, &[8, 7, 6, 5]).unwrap();
    write_pixel(&context, &pixel);
    drop((buffer, pixel, view, alias));
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        FIXED + 12,
        "polling cannot retire unsubmitted writes"
    );
    drain(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, FIXED);
    let pixel = texture(&context).unwrap();
    let view = pixel.view().unwrap();
    let alias = view.clone();
    drop((pixel, view));
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        FIXED + 4,
        "last admitted view owns the reservation"
    );
    drop(alias);
    assert_eq!(budget.info().unwrap().admitted_bytes, FIXED);
}

#[test]
fn new_external_resources_join_open_encoders_and_retire_with_native_work() {
    let budget = AllocationBudget::new(FIXED + 12).unwrap();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    drain(&renderer);
    let context = renderer.allocation_context();
    for mode in 0..3 {
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        let buffer = buffer(&context);
        let pixel = texture(&context).unwrap();
        let view = pixel.view().unwrap();
        encoder.clear_buffer(&buffer, 0, None);
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
        drop((buffer, pixel, view));
        assert_eq!(budget.info().unwrap().admitted_bytes, FIXED + 12);
        assert!(texture(&context).is_err());
        match mode {
            0 => drop(encoder),
            1 => drop(encoder.finish()),
            _ => {
                renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
                renderer
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
            }
        }
        assert_eq!(budget.info().unwrap().admitted_bytes, FIXED, "mode={mode}");
    }
}

#[test]
fn upload_context_retains_destinations_after_renderer_and_resource_owners_exit() {
    let budget = AllocationBudget::default();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    drain(&renderer);
    let context = renderer.allocation_context();
    let alias = context.clone();
    let device = renderer.device.clone();
    let buffer = buffer(&context);
    context.write_buffer(&buffer, 0, &[4, 3, 2, 1]).unwrap();
    drop((buffer, renderer, context));
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 8);
    drop(alias); // Last upload ledger submits outstanding writes, without waiting.
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
}

#[test]
fn shared_budget_does_not_authorize_uploads_from_a_different_native_owner() {
    let budget = AllocationBudget::default();
    let first =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let second =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    drain(&first);
    drain(&second);
    let context = first.allocation_context();
    let other = second.allocation_context();
    let buffer = buffer(&context);
    let pixel = texture(&context).unwrap();
    let scope = second
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let before = budget.info().unwrap().admitted_bytes;
    assert!(other
        .write_buffer(&buffer, 0, &[0; 4])
        .unwrap_err()
        .to_string()
        .contains("another renderer"));
    let error = other
        .write_texture(
            &pixel,
            &[0; 4],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        )
        .unwrap_err();
    assert!(error.to_string().contains("another renderer"));
    assert_eq!(budget.info().unwrap().admitted_bytes, before);
    assert!(pollster::block_on(scope.pop()).is_none());
    context.clone().write_buffer(&buffer, 0, &[1; 4]).unwrap();
    write_pixel(&context.clone(), &pixel);
    drop((buffer, pixel));
    drain(&first);
    drain(&second);
    assert_eq!(budget.info().unwrap().admitted_bytes, 2 * FIXED);
}
