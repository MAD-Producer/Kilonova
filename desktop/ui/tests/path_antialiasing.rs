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
        label: Some("path AA regression"),
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
        size: u64::from(row) * u64::from(height),
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
        [ratio; 2],
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
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().expect("GPU mapped range unavailable");
    mapped
        .chunks(row as usize)
        .flat_map(|r| r[..width as usize * 4].iter().copied())
        .collect()
}
fn gpu() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("Path antialiasing regression requires a GPU adapter");
    pollster::block_on(adapter.request_device(&Default::default())).unwrap()
}

#[test]
fn diagonal_strokes_have_partial_coverage_and_stable_weight_at_five_dpi_scales() {
    let (device, queue) = gpu();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        for angle in [0.03f32, 0.35, 0.8, 1.4] {
            let length = 43.;
            let mut list = PaintList::default();
            paint::line(
                &mut list,
                vec![
                    [8.2, 8.3],
                    [8.2 + length * angle.cos(), 8.3 + length * angle.sin()],
                ],
                Color::grey(255),
            );
            let pixels = capture(&device, &queue, &mut painter, &list, [64.; 2], ratio);
            let fractional = pixels
                .chunks_exact(4)
                .filter(|p| p[3] > 0 && p[3] < 255)
                .count();
            assert!(
                fractional > 20,
                "hard edges at {ratio}x / {angle}: {fractional}"
            );
            let area = pixels
                .chunks_exact(4)
                .map(|p| f64::from(p[3]) / 255.)
                .sum::<f64>()
                / f64::from(ratio * ratio);
            assert!(
                (area - f64::from(length)).abs() < 3.,
                "stroke weight {area} at {ratio}x / {angle}"
            );
            assert_eq!(painter.last_draw_calls, 1);
            assert_eq!(painter.last_solid_vertices, 6);
        }
    }
}

#[test]
fn joined_transparent_lines_and_closed_paths_do_not_double_blend_or_leak_through_clip() {
    let (device, queue) = gpu();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        for points in [
            vec![[8.25, 16.25], [32.25, 16.25], [32.25, 47.25]],
            vec![
                [8.25, 8.25],
                [48.25, 8.25],
                [48.25, 48.25],
                [8.25, 48.25],
                [8.25, 8.25],
            ],
        ] {
            let mut list = PaintList::default();
            paint::line(&mut list, points, Color::rgba(255, 190, 70, 0.5));
            let pixels = capture(&device, &queue, &mut painter, &list, [64.; 2], ratio);
            assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
            assert!(
                pixels.chunks_exact(4).all(|p| p[3] <= 129),
                "overlapping joins at {ratio}x"
            );
        }
        let mut list = PaintList::default();
        paint::line(&mut list, vec![[-20., 4.], [80., 48.]], Color::grey(255));
        list.clip_to(Rect::new(8., 4., 40., 48.));
        list.clip_to(Rect::new(16., 8., 16., 32.));
        let clipped = capture(&device, &queue, &mut painter, &list, [64.; 2], ratio);
        let width = (64. * ratio).round() as usize;
        for (i, pixel) in clipped.chunks_exact(4).enumerate() {
            let point = [(i % width) as f32 + 0.5, (i / width) as f32 + 0.5];
            if point[0] < 16. * ratio
                || point[0] > 32. * ratio
                || point[1] < 8. * ratio
                || point[1] > 40. * ratio
            {
                assert_eq!(pixel[3], 0, "clip leak at {ratio}x {point:?}");
            }
        }
        let mut unbounded = PaintList::default();
        paint::line(
            &mut unbounded,
            vec![[-20., 4.], [80., 48.]],
            Color::grey(255),
        );
        let original = capture(&device, &queue, &mut painter, &unbounded, [64.; 2], ratio);
        for (i, pixel) in clipped.chunks_exact(4).enumerate() {
            if pixel[3] > 0 {
                // Retriangulation changes floating-point interpolation by a
                // fraction of an Rgba8Unorm quantization step. Preserve the
                // same coverage to within one channel unit, including alpha.
                assert!(
                    pixel
                        .iter()
                        .zip(&original[i * 4..i * 4 + 4])
                        .all(|(a, b)| a.abs_diff(*b) <= 1),
                    "clipping changed distance coordinates at {ratio}x: {pixel:?}"
                );
            }
        }
    }
}

#[test]
fn diamonds_and_subpixel_anchor_boxes_have_covered_edges_and_emit_preview_artifact() {
    let (device, queue) = gpu();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let yellow = Color::rgb(255, 190, 70);
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        let mut list = PaintList::default();
        paint::diamond(&mut list, [16.25, 16.4], 3., yellow);
        paint::box_rect(
            &mut list,
            Rect::new(36.2, 13.4, 6., 6.),
            yellow,
            Color([0.; 4]),
            0.,
            0.,
        );
        let pixels = capture(&device, &queue, &mut painter, &list, [64.; 2], ratio);
        assert!(pixels.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
        assert!(pixels.chunks_exact(4).any(|p| p[3] == 255));
    }
    if let Ok(directory) = std::env::var("MOTION_UI_AA_ARTIFACTS") {
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        let mut list = PaintList::default();
        paint::rect(
            &mut list,
            Rect::new(0., 0., 291., 363.),
            Color::rgb(14, 17, 23),
        );
        let nodes = vec![
            [41., 46.],
            [156., 51.],
            [239., 154.],
            [27., 108.],
            [38., 191.],
            [87., 276.],
            [193., 314.],
            [242., 183.],
            [33., 73.],
            [41., 46.],
        ];
        paint::line(&mut list, nodes.clone(), yellow);
        let curve: Vec<_> = (0..=160)
            .map(|i| {
                let t = i as f32 / 160.;
                let u = 1. - t;
                [
                    u * u * u * 83.
                        + 3. * u * u * t * 68.
                        + 3. * u * t * t * 162.
                        + t * t * t * 213.,
                    u * u * u * 88.
                        + 3. * u * u * t * 291.
                        + 3. * u * t * t * 84.
                        + t * t * t * 231.,
                ]
            })
            .collect();
        paint::line(&mut list, curve, yellow);
        for p in nodes.into_iter().take(9) {
            paint::box_rect(
                &mut list,
                Rect::new(p[0] - 3., p[1] - 3., 6., 6.),
                yellow,
                Color([0.; 4]),
                0.,
                0.,
            );
        }
        for (a, b) in [([83., 88.], [68., 291.]), ([213., 231.], [162., 84.])] {
            paint::line(&mut list, vec![a, b], yellow.with_alpha(0.7));
            paint::diamond(&mut list, b, 3., yellow);
        }
        let pixels = capture(&device, &queue, &mut painter, &list, [291., 363.], 1.);
        image::save_buffer(
            directory.join("path-antialiasing.png"),
            &pixels,
            291,
            363,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
