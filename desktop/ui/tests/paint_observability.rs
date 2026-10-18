use motion_ui::{
    paint::{self, Glyph, PaintList, PaintOp, Painter},
    Color, Rect,
};
use std::sync::Mutex;

struct Capture(Mutex<Vec<(log::Level, String)>>);
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if text.contains("UI paint degradation ") {
            self.0.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}

fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    painter: &mut Painter,
    list: &PaintList,
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("original paint observation fixture"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
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
        size: 256 * 32,
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
    painter.render(device, queue, &mut encoder, &view, list, [32.; 2]).expect("UI frame must fit GPU buffers");
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
                rows_per_image: Some(32),
            },
        },
        wgpu::Extent3d {
            width: 32,
            height: 32,
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
    device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
    receive.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().expect("GPU mapped range unavailable");
    mapped
        .chunks(256)
        .flat_map(|row| row[..128].iter().copied())
        .collect()
}

#[test]
fn actual_painter_reports_discarded_work_once_per_render_and_recovers_without_pixel_changes() {
    const TEST: &str =
        "actual_painter_reports_discarded_work_once_per_render_and_recovers_without_pixel_changes";
    const CHILD: &str = "MOTION_PAINT_OBSERVATION_TEST";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["order", "atlas"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, mode)
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!String::from_utf8_lossy(&output.stdout).contains("UI paint degradation"));
        }
        return;
    };
    let capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("real GPU required for paint observation acceptance");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut list = PaintList::default();
    paint::rect(&mut list, Rect::new(0., 0., 32., 32.), Color([1.; 4]));
    for _ in 0..120 {
        draw(&device, &queue, &mut painter, &list);
    }
    assert!(capture.0.lock().unwrap().is_empty());
    list.push_glyph(Glyph {
        uv: [0., 0., 1., 1.],
        position: [4.; 2],
        size: [8.; 2],
        color: Color([1., 0., 0., 1.]),
    });
    let order_mode = mode == "order";
    if order_mode {
        painter.upload_atlas(&device, &queue, &[255; 4], 1, 1);
        list.order[1] = PaintOp::Glyph(usize::MAX);
    }
    let expected = draw(&device, &queue, &mut painter, &list);
    assert!(expected.chunks_exact(4).all(|pixel| pixel == [255; 4]));
    for _ in 1..120 {
        assert_eq!(draw(&device, &queue, &mut painter, &list), expected);
    }
    {
        let records = capture.0.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, log::Level::Warn);
        assert_eq!(records[1].0, log::Level::Error);
        assert!(records[0].1.contains("count=1 "));
        assert!(records[1].1.contains("count=60 "));
        let site = if order_mode {
            "site=paint.order "
        } else {
            "site=paint.atlas "
        };
        assert!(records.iter().all(|(_, s)| s.contains(site)));
    }
    // Healthy work in another Painter must not reset this Painter's episode.
    let mut other = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    draw(&device, &queue, &mut other, &PaintList::default());
    draw(&device, &queue, &mut painter, &list);
    assert_eq!(capture.0.lock().unwrap().len(), 2);
    if order_mode {
        list.order[1] = PaintOp::Glyph(0);
        draw(&device, &queue, &mut painter, &list);
        list.order[0] = PaintOp::Shape(usize::MAX);
    } else {
        let mut healthy = PaintList::default();
        paint::rect(&mut healthy, Rect::new(0., 0., 32., 32.), Color([1.; 4]));
        draw(&device, &queue, &mut painter, &healthy);
    }
    for _ in 0..59 {
        draw(&device, &queue, &mut painter, &list);
    }
    assert_eq!(capture.0.lock().unwrap().len(), 3);
    draw(&device, &queue, &mut painter, &list);
    assert_eq!(capture.0.lock().unwrap().len(), 4);
    if order_mode {
        list.order[0] = PaintOp::Shape(0);
    } else {
        painter.upload_atlas(&device, &queue, &[255; 4], 1, 1);
    }
    let recovered = draw(&device, &queue, &mut painter, &list);
    assert_eq!(
        &recovered[(6 * 32 + 6) * 4..(6 * 32 + 6) * 4 + 4],
        &[255, 0, 0, 255]
    );
    // Legacy raw-buffer order remains supported without a false failure.
    list.order.clear();
    for _ in 0..120 {
        assert_eq!(draw(&device, &queue, &mut painter, &list), recovered);
    }
    assert_eq!(capture.0.lock().unwrap().len(), 4);
}
