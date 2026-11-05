use super::*;
use motion_ui::widgets::{NumberEditor, NumberOptions};

fn measured_readouts(compact: Option<bool>) {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    let mut list = PaintList::default();
    let mut editor = NumberEditor::default();
    editor.compact = compact.unwrap_or(false);
    let mut scrub = ScrubField::default();
    let input = Input::default();
    let values: Vec<_> = (0..20).map(|n| n as f64 * 1.25 - 10.).collect();
    let area = Rect::new(0., 0., 160., 24.);
    let mut frame = |count| {
        list.clear();
        for n in 0..count {
            let value = values[n % values.len()];
            if compact.is_some() {
                editor.draw(&mut list, &mut atlas, area, value, NumberOptions::default());
            } else {
                ui::scrub_field(
                    &mut list,
                    &mut atlas,
                    area,
                    "Position",
                    value as f32,
                    &mut scrub,
                    &input,
                );
            }
        }
    };
    frame(200);
    let one = allocations(|| frame(1));
    let many = allocations(|| frame(200));
    println!("numeric_readouts compact={compact:?} allocations(one)={one} allocations(200)={many}");
    assert_eq!(
        (one, many),
        (0, 0),
        "warmed numeric drawing must not format a new owned string per control"
    );
    assert!(!list.glyphs.is_empty());
}

#[test]
fn fixed_number_readouts_do_not_allocate_per_control() {
    measured_readouts(Some(false));
}

#[test]
fn compact_number_readouts_do_not_allocate_per_control() {
    measured_readouts(Some(true));
}

#[test]
fn scrub_readouts_do_not_allocate_per_control() {
    measured_readouts(None);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires gpu-tests")]
fn numeric_readouts_keep_original_gpu_pixels_at_every_dpi_and_alignment() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut painter = motion_ui::paint::Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    let sheet = default_stylesheet();
    let state = States::default();
    let resolved = sheet.resolve("field.number", state).unwrap();
    let area = Rect::new(8.25, 6.5, 180., 28.);
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        atlas.set_pixel_ratio(ratio);
        for compact in [false, true] {
            let mut editor = NumberEditor::default();
            editor.compact = compact;
            for left in [false, true] {
                editor.align_left = left;
                for (value, decimals) in [
                    (-0., 2),
                    (12.345, 2),
                    (-3.125, 2),
                    (1000., 0),
                    (f64::MAX, 8),
                    (-f64::MAX, usize::MAX),
                    (f64::MIN_POSITIVE, 8),
                    (f64::INFINITY, 2),
                    (f64::NEG_INFINITY, 2),
                    (f64::NAN, 2),
                ] {
                    let mut actual = PaintList::default();
                    let mut expected = PaintList::default();
                    let options = NumberOptions {
                        decimals,
                        ..Default::default()
                    };
                    editor.draw_styled(&mut actual, &mut atlas, area, value, options, sheet, state);
                    // Original owned-string implementation, independent of the new formatter.
                    let field = if compact {
                        let height = sheet.metric("field.height", 18.).min(area.height());
                        Rect::new(
                            area.min[0],
                            area.min[1] + (area.height() - height) * 0.5,
                            area.width(),
                            height,
                        )
                    } else {
                        area
                    };
                    let owned = if compact {
                        motion_ui::widgets::compact_number(value)
                    } else {
                        format!("{:.*}", decimals.min(8), value)
                    };
                    style::box_style(&mut expected, field, resolved);
                    style::text_style(
                        &mut expected,
                        &mut atlas,
                        field.shrink(resolved.padding),
                        &owned,
                        resolved,
                        if left { Align::Left } else { Align::Right },
                    );
                    assert!(
                        !expected.glyphs.is_empty(),
                        "reference must visibly draw a number"
                    );
                    painter
                        .upload_atlas(
                            &device,
                            &queue,
                            atlas.pixel_data(),
                            atlas.config().width,
                            atlas.config().height,
                        )
                        .unwrap();
                    let reference =
                        gpu::capture(&device, &queue, &mut painter, &expected, [200., 40.], ratio);
                    let pixels =
                        gpu::capture(&device, &queue, &mut painter, &actual, [200., 40.], ratio);
                    assert_eq!(pixels, reference, "ratio={ratio} compact={compact} left={left} value={value} decimals={decimals}");
                    if compact && left && value == 12.345 {
                        if let Some(directory) =
                            std::env::var_os("MOTION_UI_NUMERIC_READOUT_ARTIFACTS")
                        {
                            let directory = std::path::Path::new(&directory).join("gpu");
                            std::fs::create_dir_all(&directory).unwrap();
                            for (name, bytes) in [("actual", &pixels), ("reference", &reference)] {
                                image::save_buffer(
                                    directory.join(format!("{name}-{ratio}x.png")),
                                    bytes,
                                    (200. * ratio).round() as u32,
                                    (40. * ratio).round() as u32,
                                    image::ColorType::Rgba8,
                                )
                                .unwrap();
                            }
                        }
                    }
                }
            }
        }
    }
}
