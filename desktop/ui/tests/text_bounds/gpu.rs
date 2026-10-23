use super::*;
use motion_ui::paint::Painter;

pub(super) fn capture(
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

// The clipping regression requires visible Chinese glyphs in its old-path
// control. System UI font selection does not guarantee CJK coverage.
// Select an installed face by actual coverage rather than weakening
// the pixel assertion or treating missing glyphs as a successful render.
fn covered_test_atlas() -> TextAtlas {
    const REQUIRED: &str = "很长的属性名称需要限制在左侧标签区域内，不能覆盖右侧数值中文jW / labelA very long property that overlaps the numeric value25…";
    let atlas = TextAtlas::from_system_language(12., false).expect("system UI font");
    if REQUIRED.chars().all(|c| atlas.supports(c)) {
        return atlas;
    }
    installed_covered_atlas(REQUIRED)
}

fn installed_covered_atlas(required: &str) -> TextAtlas {
    use ab_glyph::Font;
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    for face in database.faces() {
        let candidate = database
            .with_face_data(face.id, |data, index| {
                ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index)
                    .ok()
                    .filter(|font| required.chars().all(|c| font.glyph_id(c).0 != 0))
                    .map(ab_glyph::FontArc::new)
            })
            .flatten();
        if let Some(font) = candidate {
            eprintln!(
                "text bounds GPU fixture selected installed CJK face: {:?}",
                face.families
            );
            return TextAtlas::new(font, 12., vec![]);
        }
    }
    panic!("text bounds GPU fixture requires an installed rasterizable CJK font");
}

#[test]
fn cjk_fixture_rasterizes_visible_chinese_glyphs() {
    let mut atlas = installed_covered_atlas("中文很长的属性名称");
    for c in "中文很长的属性名称".chars() {
        assert!(atlas.supports(c), "fixture must cover {c}");
        let glyph = atlas.glyph(c);
        assert!(
            glyph.uv[2] > glyph.uv[0] && glyph.uv[3] > glyph.uv[1],
            "fixture must visibly rasterize {c}"
        );
    }
}

#[test]
fn actual_gpu_keeps_values_isolated_and_styled_text_pixels_unchanged_at_five_dpi_scales() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("GPU adapter required for text acceptance");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("text acceptance device");
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut atlas = covered_test_atlas();
    let colors = default_stylesheet().colors();
    let logical = [280., 72.];
    let area = Rect::new(16., 16., 248., 30.);
    let right = Rect::new(
        area.min[0] + area.width() * 0.45,
        area.min[1],
        area.width() * 0.55,
        area.height(),
    );
    let output = std::env::var_os("MOTION_TEXT_BOUNDS_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("motion-text-bounds-{}", std::process::id()))
        });
    std::fs::create_dir_all(&output).unwrap();
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        atlas.set_pixel_ratio(ratio);
        let mut text_style = default_stylesheet()
            .resolve("label", States::default())
            .unwrap();
        text_style.font_size = 28.;
        let clipped = Rect::new(16.25, 20.5, 100.5, 9.);
        for align in [Align::Left, Align::Center, Align::Right] {
            let mut old = PaintList::default();
            ui::text_sized(
                &mut old,
                &mut atlas,
                clipped,
                "jW / 中文 label",
                text_style.foreground,
                align,
                text_style.font_size,
            );
            old.clip_to(clipped);
            let mut actual = PaintList::default();
            style::text_style(
                &mut actual,
                &mut atlas,
                clipped,
                "jW / 中文 label",
                text_style,
                align,
            );
            painter.upload_atlas(
                &device,
                &queue,
                atlas.pixel_data(),
                atlas.config().width,
                atlas.config().height,
            ).unwrap();
            let expected = capture(&device, &queue, &mut painter, &old, logical, ratio);
            assert!(
                expected.iter().any(|v| *v != 0),
                "GPU control text must be visible"
            );
            let rendered = capture(&device, &queue, &mut painter, &actual, logical, ratio);
            image::save_buffer(
                output.join(format!("style-{ratio}-{align:?}.png")),
                &rendered,
                (logical[0] * ratio).round() as u32,
                (logical[1] * ratio).round() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            assert_eq!(
                rendered, expected,
                "style pixels changed: ratio={ratio} {align:?}"
            );
        }
    }
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        atlas.set_pixel_ratio(ratio);
        for (language, label) in [
            (
                "en",
                "A very long property label that overlaps the numeric value",
            ),
            (
                "zh",
                "很长的属性名称需要限制在左侧标签区域内，不能覆盖右侧数值",
            ),
        ] {
            let mut actual = PaintList::default();
            ui::scrub_field(
                &mut actual,
                &mut atlas,
                area,
                label,
                25.,
                &mut ScrubField::default(),
                &Input::default(),
            );
            let mut value = PaintList::default();
            paint::rect(&mut value, area, colors.SUNKEN);
            let mut value_style = default_stylesheet()
                .resolve("label", States::default())
                .unwrap();
            value_style.foreground = colors.TEXT;
            value_style.font_size = atlas.font_size();
            style::text_style(
                &mut value,
                &mut atlas,
                right,
                "25",
                value_style,
                Align::Right,
            );
            // Independent old full-row label rendering is the overlap control.
            let mut old = PaintList::default();
            paint::rect(&mut old, area, colors.SUNKEN);
            ui::text(
                &mut old,
                &mut atlas,
                area,
                label,
                colors.TEXT_MUTED,
                Align::Left,
            );
            ui::text(&mut old, &mut atlas, right, "25", colors.TEXT, Align::Right);
            painter.upload_atlas(
                &device,
                &queue,
                atlas.pixel_data(),
                atlas.config().width,
                atlas.config().height,
            ).unwrap();
            let actual = capture(&device, &queue, &mut painter, &actual, logical, ratio);
            let value = capture(&device, &queue, &mut painter, &value, logical, ratio);
            let old = capture(&device, &queue, &mut painter, &old, logical, ratio);
            let width = (logical[0] * ratio).round() as u32;
            let height = (logical[1] * ratio).round() as u32;
            image::save_buffer(
                output.join(format!("{language}-{ratio}-actual.png")),
                &actual,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            image::save_buffer(
                output.join(format!("{language}-{ratio}-old-control.png")),
                &old,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            let mut control_difference = 0;
            for y in (area.min[1] * ratio).ceil() as u32..(area.max[1] * ratio).floor() as u32 {
                for x in (right.min[0] * ratio).ceil() as u32..(right.max[0] * ratio).floor() as u32
                {
                    let offset = ((y * width + x) * 4) as usize;
                    assert_eq!(
                        actual[offset..offset + 4],
                        value[offset..offset + 4],
                        "value contaminated: {language} ratio={ratio} ({x},{y})"
                    );
                    control_difference +=
                        usize::from(old[offset..offset + 4] != value[offset..offset + 4]);
                }
            }
            assert!(
                control_difference > 5,
                "old overlap control must visibly fail: {language} ratio={ratio}"
            );
        }
    }
}
