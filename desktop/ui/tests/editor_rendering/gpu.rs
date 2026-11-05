//! Actual offscreen output of the native field, compared with a whole-line
//! text reference and independently derived run advances. No caret API is used
//! to construct the expected line. The old prefix metric is a negative control.
use super::*;
use motion_ui::{
    paint::{self, Painter, Shape},
    style::{box_style, default_stylesheet, ResolvedStyle},
    ui::{self, Align},
};

#[path = "../support/editor_capture.rs"]
mod capture;

fn reference(
    atlas: &mut TextAtlas,
    text: &str,
    area: Rect,
    style: ResolvedStyle,
    caret: f32,
) -> PaintList {
    let mut list = PaintList::default();
    box_style(&mut list, area, style);
    let body = area.shrink(style.padding);
    let width = atlas.layout_line(text).unwrap().advance * style.font_size;
    let scroll = (caret - body.width()).max(0.);
    let origin = body.min[0] - scroll;
    let mut glyphs = PaintList::default();
    ui::text_sized(
        &mut glyphs,
        atlas,
        Rect::new(origin, body.min[1], width + style.font_size, body.height()),
        text,
        style.foreground,
        Align::Left,
        style.font_size,
    );
    glyphs.clip_to(body);
    list.append(glyphs);
    let x = origin + caret;
    paint::line(
        &mut list,
        vec![[x, body.min[1]], [x, body.max[1]]],
        style.foreground,
    );
    list
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires gpu-tests")]
fn native_field_pixels_follow_whole_runs_at_five_dpis_and_two_widths() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("native field pixel acceptance requires GPU");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let sheet = default_stylesheet();
    let style = sheet
        .resolve("field", States::default().with(State::Focused))
        .unwrap();
    let overflow = "AV".repeat(32);
    let cases = [
        ("AV", 1),
        ("שלום", 0),
        ("שלום", "שלום".len()),
        (&overflow, overflow.len()),
    ];
    let output = std::env::var_os("MOTION_UI_EDITOR_GPU_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    let mut comparisons = 0;
    let mut controls = 0;
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        atlas.set_pixel_ratio(ratio);
        for width in [96., 360.] {
            let logical = [width + 32., 64.];
            let area = Rect::new(16., 12., width, 40.);
            for (index, (text, byte)) in cases.iter().enumerate() {
                let layout = atlas.layout_line(text).unwrap();
                assert_eq!(layout.missing_glyphs, 0, "fixture needs visible glyphs");
                let caret = match index {
                    0 => {
                        assert_eq!(layout.glyphs.len(), 2);
                        assert_eq!(layout.glyphs[0].cluster, 0);
                        assert_eq!(layout.glyphs[1].cluster, 1);
                        layout.glyphs[0].advance[0]
                    }
                    1 => {
                        assert!(layout
                            .glyphs
                            .windows(2)
                            .all(|g| g[0].cluster > g[1].cluster));
                        layout.advance
                    }
                    2 => 0.,
                    _ => layout.advance,
                } * style.font_size;
                let mut editor = TextEditor::default();
                editor.begin_at_end(text);
                editor.select_bytes(*byte, *byte);
                let mut actual = PaintList::default();
                editor.draw(&mut actual, &mut atlas, area);
                let expected = reference(&mut atlas, text, area, style, caret);
                let mut old = reference(&mut atlas, text, area, style, caret);
                if index < 3 {
                    let prefix =
                        atlas.layout_line(&text[..*byte]).unwrap().advance * style.font_size;
                    assert!((prefix - caret).abs() > 0.01, "old control must differ");
                    match old.shapes.last_mut().unwrap() {
                        Shape::Line { points, .. } => {
                            for point in points {
                                point[0] = area.min[0] + style.padding + prefix;
                            }
                        }
                        _ => panic!("reference must end with caret line"),
                    }
                }
                painter
                    .upload_atlas(
                        &device,
                        &queue,
                        atlas.pixel_data(),
                        atlas.config().width,
                        atlas.config().height,
                    )
                    .unwrap();
                let rendered =
                    capture::capture(&device, &queue, &mut painter, &actual, logical, ratio);
                let expected =
                    capture::capture(&device, &queue, &mut painter, &expected, logical, ratio);
                assert!(
                    rendered.iter().any(|b| *b != 0),
                    "real field must be visible"
                );
                assert_eq!(rendered, expected, "width={width} dpi={ratio} case={index}");
                comparisons += 1;
                if index < 3 {
                    let old_pixels =
                        capture::capture(&device, &queue, &mut painter, &old, logical, ratio);
                    assert_ne!(
                        rendered, old_pixels,
                        "old caret must visibly fail: width={width} dpi={ratio} case={index}"
                    );
                    controls += 1;
                    if let Some(output) = &output {
                        capture::save(
                            output.join(format!("old-caret-{width}-{ratio}-{index}.png")),
                            &old_pixels,
                            logical,
                            ratio,
                        );
                    }
                }
                if let Some(output) = &output {
                    capture::save(
                        output.join(format!("native-field-{width}-{ratio}-{index}.png")),
                        &rendered,
                        logical,
                        ratio,
                    );
                }
            }
        }
    }
    assert_eq!(comparisons, 40);
    assert_eq!(controls, 30);
    eprintln!("native GPU field comparisons={comparisons}, visible old-caret negative controls={controls}");
}
