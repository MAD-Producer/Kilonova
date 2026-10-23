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
    capture_size(device, queue, painter, list, [8; 2])
}

fn capture_size(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    painter: &mut Painter,
    list: &PaintList,
    [width, height]: [u32; 2],
) -> Result<Vec<u8>, PaintError> {
    let row_bytes = (width * 4 + 255) & !255;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("UI capacity regression"),
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
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row_bytes) * u64::from(height),
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
    painter.render(device, queue, &mut encoder, &view, list, [width as f32, height as f32])?;
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
                bytes_per_row: Some(row_bytes),
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
            send.send(result).unwrap()
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    Ok(mapped
        .chunks(row_bytes as usize)
        .flat_map(|row| row[..width as usize * 4].iter().copied())
        .collect())
}

#[test]
fn atlas_growth_keeps_emitted_clipped_and_appended_text_pixels_at_multiple_dpis() {
    use crate::{text::TextAtlas, ui::{self, Align}};
    let (device, queue) = gpu(None);
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut atlas = TextAtlas::from_system_language(14., true)
            .expect("installed UI font required for text lifetime regression");
        atlas.set_pixel_ratio(ratio);
        let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let mut list = PaintList::default();
        ui::text_sized(&mut list, &mut atlas, Rect::new(16., 8., 480., 48.),
            "Atlas retained: AaWg 0123456789", Color::grey(255), Align::Left, 24.);
        list.clip_to(Rect::new(24., 0., 480., 128.));
        assert!(!list.glyphs.is_empty());
        let initial = atlas.config();
        painter.upload_atlas(&device, &queue, atlas.pixel_data(), initial.width, initial.height).unwrap();
        let before = capture_size(&device, &queue, &mut painter, &list, [512, 128]).unwrap();
        assert!(before.iter().any(|v| *v != 0));
        for size in 48..512 {
            atlas.glyph_at('W', size as f32 / 2.);
            if atlas.config().width > initial.width { break; }
        }
        assert!(atlas.config().width > initial.width, "must really exhaust the initial atlas");
        let mut added = PaintList::default();
        ui::text_sized(&mut added, &mut atlas, Rect::new(16., 72., 480., 48.),
            "After growth: new glyphs + old cache", Color::grey(255), Align::Left, 24.);
        list.append(added);
        let grown = atlas.config();
        painter.upload_atlas(&device, &queue, atlas.pixel_data(), grown.width, grown.height).unwrap();
        let after = capture_size(&device, &queue, &mut painter, &list, [512, 128]).unwrap();
        assert_eq!(&after[..512 * 64 * 4], &before[..512 * 64 * 4],
            "retained/clipped glyphs changed at pixel ratio {ratio}");
        assert!(after[512 * 64 * 4..].iter().any(|v| *v != 0), "new text must also render");
        // Prove this case detects stale normalized UVs, rather than merely
        // asserting that two empty or uniformly colored captures are equal.
        let mut stale = PaintList::default();
        for glyph in &list.glyphs {
            let mut glyph = *glyph;
            glyph.atlas_size = None;
            stale.push_glyph(glyph);
        }
        let stale = capture_size(&device, &queue, &mut painter, &stale, [512, 128]).unwrap();
        assert_ne!(&stale[..512 * 64 * 4], &before[..512 * 64 * 4]);
        assert!(pollster::block_on(scope.pop()).is_none());
        if let Some(directory) = std::env::var_os("MOTION_UI_ATLAS_EVIDENCE") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            for (name, pixels) in [("before", before), ("stale-uv", stale), ("after", after)] {
                image::save_buffer(directory.join(format!("atlas-{ratio}-{name}.png")),
                    &pixels, 512, 128, image::ColorType::Rgba8).unwrap();
            }
        }
    }
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
    painter.upload_atlas(&device, &queue, &[255; 4], 1, 1).unwrap();
    let mut list = PaintList::default();
    for _ in 0..21_846 {
        list.push_glyph(Glyph {
            atlas_size: None,
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
        atlas_size: None,
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
