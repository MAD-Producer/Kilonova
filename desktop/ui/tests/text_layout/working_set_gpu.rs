use super::*;
use motion_ui::paint::Painter;

fn capture(
    renderer: &motion_render::Renderer,
    painter: &mut Painter,
    atlas: &TextAtlas,
    list: &PaintList,
    ratio: f32,
) -> Vec<u8> {
    let logical = [520., 160.];
    let target = renderer
        .capture_target((logical[0] * ratio) as u32, (logical[1] * ratio) as u32)
        .unwrap();
    painter
        .upload_atlas(
            &renderer.device,
            &renderer.queue,
            atlas.pixel_data(),
            atlas.config().width,
            atlas.config().height,
        )
        .unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    {
        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("working set pixel acceptance"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    painter
        .render_scaled(
            &renderer.device,
            &renderer.queue,
            &mut encoder,
            &target.view,
            list,
            logical,
            [ratio; 2],
        )
        .unwrap();
    renderer.queue.submit(Some(encoder.finish()));
    renderer.read_target(&target).unwrap()
}

fn draw(
    atlas: &mut TextAtlas,
    labels: &[String],
    width: f32,
    align: Align,
    wrong: bool,
) -> PaintList {
    let sheet = default_stylesheet();
    let style = sheet.resolve("label", States::default()).unwrap();
    let mut list = PaintList::default();
    for (row, index) in [0, 1, 31, 127, 128, labels.len() - 1]
        .into_iter()
        .enumerate()
    {
        let label = if wrong {
            "WWWWWWWWWWWWWWWWWWWW"
        } else {
            &labels[index]
        };
        let area = Rect::new(4., 4. + row as f32 * 24., width, 22.);
        ui::text(&mut list, atlas, area, label, style.foreground, align);
        let styled = Rect::new(270., area.min[1], width, 22.);
        motion_ui::style::text_style(&mut list, atlas, styled, label, style, align);
    }
    list
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn warmed_working_set_pixels_match_cold_reference_at_five_dpi_scales() {
    let renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    let mut painter = Painter::new(&renderer.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let output =
        std::env::var_os("MOTION_TEXT_WORKING_SET_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut comparisons = 0;
    for count in [200, 1000] {
        let labels = labels(count);
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        for ratio in [1., 1.25, 1.5, 1.75, 2.] {
            atlas.set_pixel_ratio(ratio);
            for width in [48., 240.] {
                // Populate actual control routes with the entire distinct set.
                let mut warm = PaintList::default();
                for label in &labels {
                    let area = Rect::new(0., 0., width, 22.);
                    ui::text(
                        &mut warm,
                        &mut atlas,
                        area,
                        label,
                        motion_ui::theme::Color::grey(220),
                        Align::Left,
                    );
                }
                for align in [Align::Left, Align::Right] {
                    let actual = draw(&mut atlas, &labels, width, align, false);
                    let rendered = capture(&renderer, &mut painter, &atlas, &actual, ratio);
                    let mut cold = TextAtlas::from_system_language(12., false).unwrap();
                    assert_eq!(cold.selected_family, atlas.selected_family);
                    cold.set_pixel_ratio(ratio);
                    let reference = draw(&mut cold, &labels, width, align, false);
                    let expected = capture(&renderer, &mut painter, &cold, &reference, ratio);
                    assert!(expected.chunks_exact(4).any(|p| p[3] != 0));
                    assert_eq!(
                        rendered, expected,
                        "count={count} dpi={ratio} width={width} {align:?}"
                    );
                    let control = draw(&mut cold, &labels, width, align, true);
                    let wrong = capture(&renderer, &mut painter, &cold, &control, ratio);
                    assert_ne!(wrong, expected, "wrong-label control must visibly differ");
                    if let Some(path) = &output {
                        for (name, pixels) in [
                            ("actual", &rendered),
                            ("reference", &expected),
                            ("wrong-label", &wrong),
                        ] {
                            image::save_buffer(
                                path.join(format!("{count}-{ratio}-{width}-{align:?}-{name}.png")),
                                pixels,
                                (520. * ratio) as u32,
                                (160. * ratio) as u32,
                                image::ColorType::Rgba8,
                            )
                            .unwrap();
                        }
                    }
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 40);
    eprintln!("working_set_gpu_pairs={comparisons} visible_wrong_label_controls={comparisons}");
    renderer.check_health().unwrap();
}
