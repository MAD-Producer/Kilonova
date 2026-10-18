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
            multiview_mask: None,
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
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
    ).expect("UI frame must fit GPU buffers");
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
    device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
    receive.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().expect("GPU mapped range unavailable");
    let mut pixels = vec![];
    for data in bytes.chunks(row as usize) {
        pixels.extend_from_slice(&data[..width as usize * 4]);
    }
    pixels
}
#[test]
fn procedural_patterns_and_fractional_dpi_borders_render_actual_pixels() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("No UI test GPU adapter");
        return;
    };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
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

#[test]
fn mixed_paint_order_covers_old_glyphs_and_keeps_new_glyphs_after_clip_and_append() {
    use motion_ui::paint::{Glyph, PaintOp};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        panic!("UI paint-order regression requires a GPU adapter");
    };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .unwrap();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    // A deterministic coverage atlas exercises the actual glyph pipeline without
    // depending on platform font shape, hinting or font installation.
    painter.upload_atlas(&device, &queue, &[255; 4], 1, 1);
    let glyph = |position, color| Glyph {
        position,
        size: [8., 8.],
        uv: [0., 0., 1., 1.],
        color,
    };
    let background = Color::rgb(20, 40, 60);
    let mut reference = PaintList::default();
    paint::rect(&mut reference, Rect::new(0., 0., 32., 32.), background);
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        let expected = capture(&device, &queue, &mut painter, &reference, [32., 32.], ratio);
        let mut covered = PaintList::default();
        covered.push_glyph(glyph([4., 4.], Color::rgb(255, 0, 0)));
        paint::rect(&mut covered, Rect::new(0., 0., 32., 32.), background);
        assert_eq!(covered.order, [PaintOp::Glyph(0), PaintOp::Shape(0)]);
        assert_eq!(
            capture(&device, &queue, &mut painter, &covered, [32., 32.], ratio),
            expected
        );
        assert_eq!(painter.last_draw_calls, 2);
        let mut foreground = PaintList::default();
        paint::rect(&mut foreground, Rect::new(0., 0., 32., 32.), background);
        foreground.push_glyph(glyph([4., 4.], Color::rgb(255, 0, 0)));
        let above = capture(
            &device,
            &queue,
            &mut painter,
            &foreground,
            [32., 32.],
            ratio,
        );
        assert_ne!(above, expected);
        assert!(above.chunks_exact(4).any(|pixel| pixel == [255, 0, 0, 255]));
    }
    let mut parent = PaintList::default();
    parent.push_glyph(glyph([4., 4.], Color::rgb(255, 0, 0)));
    let mut popup = PaintList::default();
    popup.push_glyph(glyph([100., 100.], Color::rgb(255, 0, 0)));
    paint::rect(&mut popup, Rect::new(100., 100., 8., 8.), background);
    paint::rect(&mut popup, Rect::new(0., 0., 32., 32.), background);
    popup.push_glyph(glyph([4., 4.], Color::rgb(0, 255, 0)));
    popup.clip_to(Rect::new(0., 0., 32., 32.));
    assert_eq!(popup.order, [PaintOp::Shape(0), PaintOp::Glyph(0)]);
    parent.append(popup);
    assert_eq!(
        parent.order,
        [PaintOp::Glyph(0), PaintOp::Shape(0), PaintOp::Glyph(1)]
    );
    let pixels = capture(&device, &queue, &mut painter, &parent, [32., 32.], 1.);
    assert!(!pixels
        .chunks_exact(4)
        .any(|pixel| pixel == [255, 0, 0, 255]));
    assert!(pixels
        .chunks_exact(4)
        .any(|pixel| pixel == [0, 255, 0, 255]));
    assert_eq!(painter.last_draw_calls, 3);
    parent.clear();
    assert!(parent.order.is_empty());
    paint::rect(&mut parent, Rect::new(0., 0., 32., 32.), background);
    assert_eq!(parent.order, [PaintOp::Shape(0)]);
    assert_eq!(
        capture(&device, &queue, &mut painter, &parent, [32., 32.], 1.),
        capture(&device, &queue, &mut painter, &reference, [32., 32.], 1.)
    );
}

#[test]
fn stylesheet_radius_and_shadow_render_distinct_clipped_pixels_at_fractional_dpi() {
    use motion_ui::style::{box_style, default_stylesheet, States};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("UI pixel regression requires GPU");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let square = default_stylesheet()
        .with_overrides(
            br##"{"styles":{"button":{"background":"#ffffff","border_width":0,"radius":0}}}"##,
        )
        .unwrap()
        .resolve("button", States::default())
        .unwrap();
    let rounded=default_stylesheet().with_overrides(br##"{"styles":{"button":{"background":"#ffffff","border_width":0,"radius":8,"shadow_color":"#00000080","shadow_blur":6,"shadow_offset_y":3}}}"##).unwrap().resolve("button",States::default()).unwrap();
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        let area = Rect::new(8., 8., 24., 20.);
        let mut a = PaintList::default();
        box_style(&mut a, area, square);
        let mut b = PaintList::default();
        box_style(&mut b, area, rounded);
        let flat = capture(&device, &queue, &mut painter, &a, [40., 40.], ratio);
        let raised = capture(&device, &queue, &mut painter, &b, [40., 40.], ratio);
        assert_ne!(flat, raised);
        let width = (40. * ratio) as usize;
        let pixel = |x: f32, y: f32| (((y * ratio) as usize) * width + (x * ratio) as usize) * 4;
        assert_eq!(flat[pixel(20., 32.) + 3], 0);
        assert!(
            raised[pixel(20., 32.) + 3] > 0,
            "shadow is visible at {ratio} DPI"
        );
        b.clip_to(Rect::new(0., 0., 40., 28.));
        let clipped = capture(&device, &queue, &mut painter, &b, [40., 40.], ratio);
        assert_eq!(clipped[pixel(20., 32.) + 3], 0);
        // The real Motion sample and the renderer must preserve the configured
        // curve at the same hover time, rather than silently using one easing.
        let mut hover_pixels = vec![];
        for easing in ["linear", "cubic-out"] {
            let sheet = default_stylesheet()
                .with_overrides(
                    format!("{{\"tokens\":{{\"motion\":{{\"easing\":\"{easing}\"}}}}}}").as_bytes(),
                )
                .unwrap();
            let mut motion = motion_ui::motion::Motion::default();
            motion.set_easing(sheet.easing());
            motion.value_at((1, 1), 0., 1., 100., false, 0.);
            let amount = motion.value_at((1, 1), 0., 1., 100., false, 0.05);
            let mut hover = PaintList::default();
            paint::rect(
                &mut hover,
                area,
                sheet.color("focus").with_alpha(amount * 0.07),
            );
            hover_pixels.push(capture(
                &device,
                &queue,
                &mut painter,
                &hover,
                [40., 40.],
                ratio,
            ));
        }
        assert_ne!(
            hover_pixels[0], hover_pixels[1],
            "easing must change intermediate hover pixels at {ratio} DPI"
        );
    }
}
