use motion_ui::{
    paint::{self, PaintList, Painter},
    Color, Rect,
};

fn capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    painter: &mut Painter,
    list: &PaintList,
    logical: [f32; 2],
    ratio: f32,
) -> Vec<u8> {
    let width = (logical[0] * ratio).round() as u32;
    let height = (logical[1] * ratio).round() as u32;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("analytic UI test"),
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
    let view = texture.create_view(&Default::default());
    let row = (width * 4 + 255) & !255;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: row as u64 * height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
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
    painter.render_scaled(
        device,
        queue,
        &mut encoder,
        &view,
        list,
        logical,
        [ratio, ratio],
    );
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
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    device.poll(wgpu::Maintain::Wait);
    receive.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range();
    let mut pixels = vec![];
    for data in bytes.chunks(row as usize) {
        pixels.extend_from_slice(&data[..width as usize * 4]);
    }
    pixels
}
#[test]
fn procedural_patterns_and_fractional_dpi_borders_render_actual_pixels() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let Some(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("No UI test GPU adapter");
        return;
    };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
            .unwrap();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut list = PaintList::default();
    paint::checkerboard(
        &mut list,
        Rect::new(0., 0., 32., 32.),
        Color::grey(255),
        Color::grey(0),
        8.,
    );
    list.clip_to(Rect::new(3., 4., 23., 20.));
    let pixels = capture(&device, &queue, &mut painter, &list, [32., 32.], 1.);
    let at = |x: usize, y: usize| &pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
    assert_eq!(at(1, 1), [0, 0, 0, 0]);
    assert_eq!(at(4, 5), [255, 255, 255, 255]);
    assert_eq!(at(10, 5), [0, 0, 0, 255]);
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        let mut list = PaintList::default();
        paint::stroke(
            &mut list,
            Rect::new(5.3, 5.3, 20.4, 15.4),
            Color::rgba(200, 200, 200, 0.5),
            1.,
        );
        let pixels = capture(&device, &queue, &mut painter, &list, [32., 32.], ratio);
        assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
        assert!(
            pixels.chunks_exact(4).all(|p| p[3] <= 129),
            "overlapping border corners at {ratio}"
        );
    }
    let mut grid = PaintList::default();
    paint::grid(
        &mut grid,
        Rect::new(0., 0., 64., 64.),
        Color::rgba(255, 255, 255, 0.3),
        Color::rgba(255, 255, 255, 0.8),
        [0., 0.],
        20.,
        40.,
    );
    assert_eq!(grid.shapes.len(), 1);
    let pixels = capture(&device, &queue, &mut painter, &grid, [64., 64.], 1.);
    assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
    assert!(pixels.chunks_exact(4).any(|p| p[3] == 0));
}
