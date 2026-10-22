use super::*;
use unicode_segmentation::UnicodeSegmentation;

// Independent strings exercise extended clusters, including unsupported-font
// advances. This tests truncation boundaries, not complex-script shaping.
const CLUSTERS: &[&str] = &[
    "e\u{301}",
    "a\u{308}\u{301}",
    "\u{1100}\u{1161}\u{11a8}",
    "\u{915}\u{94d}\u{937}",
    "👩🏽\u{200d}🚀",
    "🇯🇵",
    "👨\u{200d}👩\u{200d}👧",
    "1\u{fe0f}\u{20e3}",
    "\r\n",
];

#[test]
fn every_ellipsized_prefix_ends_at_an_extended_grapheme_boundary_and_fits() {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    for cluster in CLUSTERS {
        let text = format!("A{cluster}B long label");
        let total = atlas.measure(&text);
        let boundaries: Vec<_> = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect();
        for step in 0..=((total * 4.).ceil() as u32 + 1) {
            let width = step as f32 * 0.25;
            let actual = atlas.ellipsize(&text, width);
            assert!(
                atlas.measure(&actual) <= width + 0.001,
                "overrun: {actual:?}, width={width}"
            );
            if width >= total {
                assert_eq!(actual, text);
            } else if let Some(prefix) = actual.strip_suffix('…') {
                assert!(text.starts_with(prefix));
                assert!(
                    boundaries.contains(&prefix.len()),
                    "split cluster {cluster:?}: {actual:?}, width={width}"
                );
            } else {
                assert!(actual.is_empty());
            }
        }
    }
}

#[test]
fn a_cluster_that_cannot_fit_is_omitted_whole_without_losing_the_previous_cluster() {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    let first = "A🇯";
    let full = "A🇯🇵";
    let width = (atlas.measure(first) + atlas.measure(full)) * 0.5 + atlas.measure("…");
    assert!(width < atlas.measure("A🇯🇵BBBBBBBB"));
    assert_eq!(atlas.ellipsize("A🇯🇵BBBBBBBB", width), "A…");
    let width = atlas.measure("A🇯🇵…") + 0.01;
    assert_eq!(atlas.ellipsize("A🇯🇵BBBBBBBB", width), "A🇯🇵…");
    assert_eq!(atlas.ellipsize("ASCII 中文", f32::INFINITY), "ASCII 中文");
    assert_eq!(atlas.ellipsize("ASCII 中文", f32::NAN), "");
    assert_eq!(atlas.ellipsize("ASCII 中文", -1.), "");
}

#[test]
fn warmed_cluster_labels_keep_plain_and_styled_drawing_allocation_free() {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    let label = "Ae\u{301} very long label";
    assert!(
        label.chars().all(|c| atlas.supports(c)),
        "fixture needs visible supported combining glyphs"
    );
    let style = default_stylesheet()
        .resolve("label", States::default())
        .unwrap();
    let mut list = PaintList::default();
    for width in [32., 240.] {
        let area = Rect::new(0., 0., width, 22.);
        let mut draw = |count| {
            list.clear();
            for _ in 0..count {
                ui::text(
                    &mut list,
                    &mut atlas,
                    area,
                    label,
                    style.foreground,
                    Align::Left,
                );
                style::text_style(&mut list, &mut atlas, area, label, style, Align::Right);
            }
        };
        draw(200);
        assert_eq!(allocations(|| draw(1)), 0);
        assert_eq!(allocations(|| draw(200)), 0);
    }
}

#[test]
fn actual_gpu_draws_the_complete_prefix_and_ellipsis_at_every_alignment() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut painter = motion_ui::paint::Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    assert!("A…".chars().all(|c| atlas.supports(c)));
    let base = default_stylesheet()
        .resolve("label", States::default())
        .unwrap();
    let logical_width = (atlas.measure("A🇯") + atlas.measure("A🇯🇵")) * 0.5 + atlas.measure("…");
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        atlas.set_pixel_ratio(ratio);
        for size in [12., 28.] {
            let style = motion_ui::style::ResolvedStyle {
                font_size: size,
                ..base
            };
            let area = Rect::new(10.25, 12.5, logical_width * size / atlas.font_size(), 36.);
            for align in [Align::Left, Align::Center, Align::Right] {
                let draw = |atlas: &mut TextAtlas, label| {
                    let mut list = PaintList::default();
                    style::text_style(&mut list, atlas, area, label, style, align);
                    list
                };
                let actual = draw(&mut atlas, "A🇯🇵BBBBBBBB");
                let expected = draw(&mut atlas, "A…");
                painter.upload_atlas(
                    &device,
                    &queue,
                    atlas.pixel_data(),
                    atlas.config().width,
                    atlas.config().height,
                ).unwrap();
                let actual_pixels =
                    gpu::capture(&device, &queue, &mut painter, &actual, [160., 64.], ratio);
                let expected_pixels =
                    gpu::capture(&device, &queue, &mut painter, &expected, [160., 64.], ratio);
                assert!(
                    expected_pixels.chunks_exact(4).any(|p| p[3] > 0),
                    "empty glyph control"
                );
                assert!(
                    actual_pixels == expected_pixels,
                    "prefix pixels at {ratio}x, font={size}, align={align:?}"
                );
                if size == 28. && matches!(align, Align::Right) {
                    if let Some(directory) = std::env::var_os("MOTION_UI_GRAPHEME_ARTIFACTS") {
                        let directory = std::path::Path::new(&directory);
                        std::fs::create_dir_all(directory).unwrap();
                        image::save_buffer(
                            directory.join(format!("prefix-{ratio}x.png")),
                            &actual_pixels,
                            (160. * ratio).round() as u32,
                            (64. * ratio).round() as u32,
                            image::ColorType::Rgba8,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}
