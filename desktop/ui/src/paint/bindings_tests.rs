use super::*;

fn capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    painter: &mut Painter,
    list: &PaintList,
    scale: u32,
) -> Vec<u8> {
    let (width, height) = (16 * scale, 8 * scale);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cached UI bindings test"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("UI bindings pixels"),
        size: 256 * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            multiview_mask: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
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
    painter
        .render_scaled(
            device,
            queue,
            &mut encoder,
            &view,
            list,
            [16., 8.],
            [scale as f32; 2],
        )
        .unwrap();
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv().unwrap().unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    mapped
        .chunks(256)
        .flat_map(|row| row[..width as usize * 4].iter().copied())
        .collect()
}

#[test]
fn bindings_survive_repeated_frames_and_pixel_updates_but_follow_atlas_replacement() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("UI binding acceptance requires a GPU adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let solid = painter.solid_bind.clone();
    assert_eq!(painter.bind_group_creations, 1);
    // Empty/missing-atlas polling retains its existing normal semantics.
    for _ in 0..4 {
        assert!(
            capture(&device, &queue, &mut painter, &PaintList::default(), 1)
                .iter()
                .all(|b| *b == 0)
        );
    }
    assert_eq!(painter.bind_group_creations, 1);
    let mut list = PaintList::default();
    rect(&mut list, Rect::new(0., 0., 8., 8.), Color::rgb(255, 0, 0));
    list.push_glyph(Glyph {
        atlas_size: None,
        uv: [0., 0., 1., 1.],
        position: [8., 0.],
        size: [8., 8.],
        color: Color::rgb(0, 255, 0),
    });
    painter.upload_atlas(&device, &queue, &[255; 64], 4, 4).unwrap();
    let glyph = painter.glyph_bind.clone().unwrap();
    for scale in [1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2] {
        let pixels = capture(&device, &queue, &mut painter, &list, scale);
        let at = |x: u32, y: u32| {
            &pixels[((y * 16 * scale + x) * 4) as usize..((y * 16 * scale + x) * 4 + 4) as usize]
        };
        assert_eq!(at(2 * scale, 2 * scale), [255, 0, 0, 255]);
        assert_eq!(at(12 * scale, 2 * scale), [0, 255, 0, 255]);
        assert_eq!(painter.solid_bind, solid);
        assert_eq!(painter.glyph_bind.as_ref(), Some(&glyph));
        assert_eq!(painter.bind_group_creations, 2);
    }
    // Uploading new coverage into the same texture must remain visible through
    // the old bind group, without paying for another binding allocation.
    painter.upload_atlas(&device, &queue, &[0; 64], 4, 4).unwrap();
    let pixels = capture(&device, &queue, &mut painter, &list, 1);
    assert_eq!(
        &pixels[(2 * 16 + 12) * 4..(2 * 16 + 12) * 4 + 4],
        [0, 0, 0, 0]
    );
    assert_eq!(painter.glyph_bind.as_ref(), Some(&glyph));
    assert_eq!(painter.bind_group_creations, 2);
    // A new texture generation needs exactly one replacement. A stale binding
    // would still sample the transparent old atlas and fail the pixel check.
    painter.upload_atlas(&device, &queue, &[255; 256], 8, 8).unwrap();
    let pixels = capture(&device, &queue, &mut painter, &list, 1);
    assert_eq!(
        &pixels[(2 * 16 + 12) * 4..(2 * 16 + 12) * 4 + 4],
        [0, 255, 0, 255]
    );
    assert_ne!(painter.glyph_bind.as_ref(), Some(&glyph));
    assert_eq!(painter.solid_bind, solid);
    assert_eq!(painter.bind_group_creations, 3);
    let fresh = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(fresh.bind_group_creations, 1);
}
