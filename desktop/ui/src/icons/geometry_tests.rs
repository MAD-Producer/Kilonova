use super::*;
use crate::paint::{self, Painter};

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
        label: Some("text bounds test"),
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
    painter
        .render_scaled(
            device,
            queue,
            &mut encoder,
            &view,
            list,
            logical,
            [ratio, ratio],
        )
        .expect("UI frame must fit GPU buffers");
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
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("GPU poll failed");
    receive.recv().unwrap().unwrap();
    let bytes = buffer
        .slice(..)
        .get_mapped_range()
        .expect("GPU mapped range unavailable");
    let mut pixels = vec![];
    for data in bytes.chunks(row as usize) {
        pixels.extend_from_slice(&data[..width as usize * 4]);
    }
    pixels
}

fn legacy_draw(list: &mut PaintList, area: Rect, mesh: &Mesh, size: f32, color: Color) {
    if !size.is_finite()
        || !area.width().is_finite()
        || !area.height().is_finite()
        || color.0[3] <= 0.
    {
        return;
    }
    let edge = size.min(area.width()).min(area.height());
    if edge <= 0. {
        return;
    }
    let scale = edge / 28.;
    let origin = [
        area.min[0] + (area.width() - edge) * 0.5,
        area.min[1] + (area.height() - edge) * 0.5,
    ];
    let points = mesh
        .iter()
        .flat_map(|t| {
            t.iter()
                .map(|p| [origin[0] + p[0] * scale, origin[1] + p[1] * scale])
        })
        .collect();
    let mut content = PaintList::default();
    content.push_shape(Shape::Triangles { points }, color);
    content.clip_to(Rect::new(origin[0], origin[1], edge, edge));
    list.append(content);
}

#[test]
fn all_icons_keep_legacy_gpu_pixels_through_viewport_host_and_nested_clipping() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("GPU adapter required for icon acceptance");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("icon acceptance device");
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let sources: HashMap<String, Source> =
        serde_json::from_str(include_str!("../../icons/paths.json")).unwrap();
    let original: HashMap<String, Mesh> = sources
        .into_iter()
        .map(|(id, s)| (id, compile(s)))
        .collect();
    let output = std::env::var_os("MOTION_ICON_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("motion-icon-geometry-{}", std::process::id()))
        });
    std::fs::create_dir_all(&output).unwrap();
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        for clips in 0..3 {
            let mut actual = PaintList::default();
            let mut legacy = PaintList::default();
            for (i, icon) in Icon::ALL.iter().enumerate() {
                let x = (i % 8) as f32 * 40. + 2.25;
                let y = (i / 8) as f32 * 40. + 3.5;
                let area = Rect::new(x, y, 35.5, 28.25);
                let color = if i % 2 == 0 {
                    Color::grey(220)
                } else {
                    Color::grey(120).with_alpha(0.55)
                };
                draw(&mut actual, area, *icon, 25.75, color);
                legacy_draw(&mut legacy, area, &original[icon.id()], 25.75, color);
                if i % 5 == 0 {
                    for list in [&mut actual, &mut legacy] {
                        paint::rect(
                            list,
                            Rect::new(x + 7., y + 11., 18., 3.),
                            Color::rgb(30, 70, 100),
                        );
                    }
                }
            }
            for area in [
                Rect::new(9.5, 10.25, 299.75, 293.5),
                Rect::new(13.25, 17.5, 288.5, 281.25),
            ]
            .into_iter()
            .take(clips)
            {
                actual.clip_to(area);
                legacy.clip_to(area);
            }
            let expected = capture(&device, &queue, &mut painter, &legacy, [320., 320.], ratio);
            let rendered = capture(&device, &queue, &mut painter, &actual, [320., 320.], ratio);
            assert!(
                expected.iter().filter(|v| **v != 0).count() > 3000,
                "legacy control must visibly render all icons"
            );
            for (name, pixels) in [("actual", &rendered), ("legacy", &expected)] {
                image::save_buffer(
                    output.join(format!("icons-{ratio}-clips{clips}-{name}.png")),
                    pixels,
                    (320. * ratio).round() as u32,
                    (320. * ratio).round() as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            assert_eq!(
                rendered, expected,
                "icon pixels changed ratio={ratio} clip_count={clips}"
            );
        }
    }
}

#[test]
fn collapsed_icon_viewports_keep_the_legacy_empty_output() {
    let mut sources: HashMap<String, Source> =
        serde_json::from_str(include_str!("../../icons/paths.json")).unwrap();
    let mesh = compile(sources.remove(Icon::Star.id()).unwrap());
    for (area, size) in [
        (Rect::new(1e9, 1e9, 1e8, 1e8), 1.),
        (Rect::new(1., 1., 20., 20.), f32::from_bits(1)),
        (Rect::new(0., 0., 20., 20.), 0.),
        (Rect::new(0., 0., -20., 20.), 12.),
        (Rect::new(0., 0., 20., 20.), f32::NAN),
    ] {
        let mut actual = PaintList::default();
        let mut legacy = PaintList::default();
        draw(&mut actual, area, Icon::Star, size, Color::grey(220));
        legacy_draw(&mut legacy, area, &mesh, size, Color::grey(220));
        assert!(
            legacy.shapes.is_empty(),
            "fixture must have no drawable viewport: {area:?} {size}"
        );
        assert!(
            actual.shapes.is_empty(),
            "collapsed viewport must stay empty: {area:?} {size}"
        );
        assert!(actual.colors.is_empty() && actual.order.is_empty());
    }
}
