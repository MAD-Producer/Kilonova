use super::*;

fn gpu(limit: Option<u64>) -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("GPU required for UI buffer regression");
    let mut descriptor = wgpu::DeviceDescriptor::default();
    if let Some(limit) = limit {
        descriptor.required_limits.max_buffer_size = limit;
    }
    pollster::block_on(adapter.request_device(&descriptor)).unwrap()
}

fn capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    painter: &mut Painter,
    list: &PaintList,
) -> Result<Vec<u8>, PaintError> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("UI capacity regression"),
        size: wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * 8,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            multiview_mask: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    painter.render(device, queue, &mut encoder, &view, list, [8.; 2])?;
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
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
    queue.submit(Some(encoder.finish()));
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap()
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    Ok(mapped
        .chunks(256)
        .flat_map(|row| row[..32].iter().copied())
        .collect())
}

fn dense_solids() -> PaintList {
    let mut list = PaintList::default();
    for _ in 0..11_091 {
        rect(
            &mut list,
            Rect::new(0., 0., 1., 1.),
            Color([1., 0., 0., 1.]),
        );
    }
    // This final quad must survive, rather than being truncated at the old cap.
    rect(
        &mut list,
        Rect::new(0., 0., 8., 8.),
        Color([0., 1., 0., 1.]),
    );
    list
}

#[test]
fn reported_solid_overrun_grows_and_retains_every_vertex_and_pixel() {
    assert_eq!(std::mem::size_of::<SolidVertex>(), 88);
    let (device, queue) = gpu(None);
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(painter.solid_buffer.size(), 5_767_168);
    let list = dense_solids();
    // Reproduce the original write on a buffer of exactly the reported size.
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    queue.write_buffer(&painter.solid_buffer, 0, &vec![0; 5_856_576]);
    let original = pollster::block_on(scope.pop())
        .expect("original write must overrun")
        .to_string();
    assert!(
        original.contains("5856576") && original.contains("5767168"),
        "{original}"
    );
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let expected = [0, 255, 0, 255].repeat(64);
    assert_eq!(
        capture(&device, &queue, &mut painter, &list).unwrap(),
        expected
    );
    assert_eq!(painter.last_solid_vertices, 66_552);
    assert!(painter.solid_buffer.size() >= 5_856_576);
    let capacity = painter.solid_buffer.size();
    for _ in 0..3 {
        assert_eq!(
            capture(&device, &queue, &mut painter, &list).unwrap(),
            expected
        );
        assert_eq!(painter.solid_buffer.size(), capacity);
    }
    capture(&device, &queue, &mut painter, &PaintList::default()).unwrap();
    assert_eq!(painter.solid_buffer.size(), capacity);
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn glyph_growth_keeps_trailing_text_and_interleaved_draw_order() {
    let (device, queue) = gpu(None);
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    painter.upload_atlas(&device, &queue, &[255; 4], 1, 1);
    let mut list = PaintList::default();
    for _ in 0..21_846 {
        list.push_glyph(Glyph {
            uv: [0., 0., 1., 1.],
            position: [-4.; 2],
            size: [1.; 2],
            color: Color([1.; 4]),
        });
    }
    rect(
        &mut list,
        Rect::new(0., 0., 8., 8.),
        Color([0., 1., 0., 1.]),
    );
    list.push_glyph(Glyph {
        uv: [0., 0., 1., 1.],
        position: [0.; 2],
        size: [8.; 2],
        color: Color([1., 0., 0., 1.]),
    });
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let expected = [255, 0, 0, 255].repeat(64);
    assert_eq!(
        capture(&device, &queue, &mut painter, &list).unwrap(),
        expected
    );
    assert_eq!(painter.last_glyph_vertices, 131_082);
    assert_eq!(painter.last_draw_calls, 3);
    let capacity = painter.glyph_buffer.size();
    assert!(capacity > (GLYPH_CAPACITY * std::mem::size_of::<GlyphVertex>()) as u64);
    assert_eq!(
        capture(&device, &queue, &mut painter, &list).unwrap(),
        expected
    );
    assert_eq!(painter.glyph_buffer.size(), capacity);
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn device_limit_refusal_precedes_upload_and_next_small_frame_recovers() {
    let (device, queue) = gpu(Some(5_767_168));
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let failure = capture(&device, &queue, &mut painter, &dense_solids()).unwrap_err();
    assert_eq!(failure.kind, "solid");
    assert_eq!(failure.vertices, 66_552);
    assert_eq!(failure.required_bytes, 5_856_576);
    assert_eq!(failure.limit_bytes, 5_767_168);
    assert_eq!(painter.solid_buffer.size(), 5_767_168);
    assert_eq!(painter.last_draw_calls, 0);
    let mut healthy = PaintList::default();
    rect(
        &mut healthy,
        Rect::new(0., 0., 8., 8.),
        Color([0., 1., 0., 1.]),
    );
    assert_eq!(
        capture(&device, &queue, &mut painter, &healthy).unwrap(),
        [0, 255, 0, 255].repeat(64)
    );
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn geometric_growth_is_clamped_to_a_non_power_of_two_device_limit() {
    let (device, queue) = gpu(Some(5_856_576));
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    assert_eq!(
        capture(&device, &queue, &mut painter, &dense_solids()).unwrap(),
        [0, 255, 0, 255].repeat(64)
    );
    assert_eq!(painter.solid_buffer.size(), 5_856_576);
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn draw_index_and_byte_overflow_are_rejected_before_narrowing() {
    if let Ok(count) = usize::try_from(u64::from(u32::MAX) + 1) {
        assert!(vertex_buffer_size::<SolidVertex>("solid", count, u64::MAX).is_err());
    }
    assert!(vertex_buffer_size::<SolidVertex>("solid", usize::MAX, u64::MAX - 1).is_err());
}
