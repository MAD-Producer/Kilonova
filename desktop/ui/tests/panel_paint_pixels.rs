use motion_ui::{
    dock::DockId,
    interaction::Router,
    paint::{self, Bitmap, ImageQuad, PaintOp, Painter, Shape},
    panel::{PanelFrame, PanelHost, PanelLayout, PanelView},
    registry::{Language, PanelDescriptor},
    style::default_stylesheet,
    ui::{self, Align},
    widgets::{Dropdown, DropdownItem},
    Color, Input, Key, PaintList, Rect, TextAtlas,
};
use std::sync::Arc;
#[path = "support/editor_capture.rs"]
mod capture;

fn gpu() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("panel paint pixel acceptance requires GPU");
    pollster::block_on(adapter.request_device(&Default::default())).unwrap()
}

fn scene(list: &mut PaintList, atlas: &mut TextAtlas, bounds: Rect, bitmap: &Arc<Bitmap>) {
    let styles = list.styles();
    paint::rect(list, bounds, styles.color("panel"));
    ui::text(
        list,
        atlas,
        Rect::new(bounds.min[0] - 4., bounds.min[1] + 5., 150., 24.),
        "Clipped property",
        styles.color("text"),
        Align::Left,
    );
    list.push_image(ImageQuad::new(
        bitmap.clone(),
        Rect::new(bounds.min[0] + 80., bounds.min[1] + 20., 64., 40.),
    ));
    paint::rect(
        list,
        Rect::new(bounds.min[0] + 90., bounds.min[1] + 28., 35., 12.),
        Color([0.3, 0.6, 0.9, 0.5]),
    );
}
struct Panel(Arc<Bitmap>);
impl PanelView for Panel {
    type Action = ();
    fn draw(&mut self, frame: &mut PanelFrame<'_>) -> Vec<()> {
        scene(frame.paint, frame.atlas, frame.layout.bounds, &self.0);
        vec![]
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires gpu-tests")]
fn reused_panel_buffers_match_independent_ordered_gpu_commands_at_five_dpis() {
    let (device, queue) = gpu();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let bitmap = Bitmap::new(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ],
    )
    .unwrap();
    let mut host = PanelHost::default();
    for id in 1..=2 {
        host.register(
            PanelDescriptor::new(DockId(id), "参数", "Parameters"),
            Panel(bitmap.clone()),
        )
        .unwrap();
    }
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut actual = PaintList::default();
    let mut comparisons = 0;
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        atlas.set_pixel_ratio(ratio);
        for width in [100., 170., 220.] {
            for epoch in 0..2 {
                let styles = if epoch == 0 {
                    default_stylesheet().clone()
                } else {
                    default_stylesheet()
                        .with_overrides(
                            br##"{"tokens":{"colors":{"panel":"#345678","text":"#abcdef"}}}"##,
                        )
                        .unwrap()
                };
                host.set_stylesheet(styles.clone());
                actual.clear();
                let mut expected = PaintList::default();
                for (id, x, y) in [(2, 8., 4.), (1, 54., 26.), (2, 24., 45.)] {
                    let bounds = Rect::new(x, y, width, 64.);
                    let clip = Rect::new(x + 6., y + 3., width - 12., 50.);
                    host.draw(
                        PanelLayout {
                            window: u64::from(id),
                            panel: DockId(id),
                            bounds,
                            clip,
                        },
                        Language::English,
                        &Input::default(),
                        &Router::default(),
                        &mut actual,
                        &mut atlas,
                    );
                    let mut content = PaintList::with_styles(Arc::new(styles.clone()));
                    scene(&mut content, &mut atlas, bounds, &bitmap);
                    content.clip_to(bounds.intersection(clip));
                    // Independent assembly: never call either append method.
                    for op in content.order {
                        match op {
                            PaintOp::Shape(i) => {
                                expected.push_shape(content.shapes[i].clone(), content.colors[i])
                            }
                            PaintOp::Glyph(i) => expected.push_glyph(content.glyphs[i]),
                            PaintOp::Image(i) => expected.push_image(content.images[i].clone()),
                        }
                    }
                }
                let config = atlas.config();
                painter
                    .upload_atlas(
                        &device,
                        &queue,
                        atlas.pixel_data(),
                        config.width,
                        config.height,
                    )
                    .unwrap();
                let expected_pixels = capture::capture(
                    &device,
                    &queue,
                    &mut painter,
                    &expected,
                    [320., 120.],
                    ratio,
                );
                let pixels =
                    capture::capture(&device, &queue, &mut painter, &actual, [320., 120.], ratio);
                assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
                assert_eq!(
                    pixels, expected_pixels,
                    "panel width={width} ratio={ratio} epoch={epoch}"
                );
                expected.order.reverse();
                assert_ne!(
                    capture::capture(
                        &device,
                        &queue,
                        &mut painter,
                        &expected,
                        [320., 120.],
                        ratio
                    ),
                    pixels,
                    "wrong layer order must visibly fail"
                );
                comparisons += 1;
                if width == 170. && epoch == 1 && ratio == 1. {
                    save("panels-reused.png", &pixels, [320., 120.], ratio);
                    save(
                        "panels-reference.png",
                        &expected_pixels,
                        [320., 120.],
                        ratio,
                    );
                }
            }
        }
    }
    println!("panel complete-image comparisons={comparisons}, reversed-order negative controls={comparisons}");
    assert_eq!(comparisons, 30);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires gpu-tests")]
fn dropdown_inline_stroke_matches_old_line_pixels_across_repaint_scroll_and_theme_changes() {
    let (device, queue) = gpu();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let items: Vec<_> = [
        "Normal",
        "Multiply",
        "Screen",
        "Overlay",
        "Darken",
        "Lighten",
        "Difference",
        "Color",
        "Luminosity",
        "Subtract",
    ]
    .into_iter()
    .map(|label| DropdownItem {
        label,
        enabled: true,
    })
    .collect();
    let mut comparisons = 0;
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        atlas.set_pixel_ratio(ratio);
        for width in [190., 320.] {
            let mut dropdown = Dropdown::default();
            let mut actual = PaintList::default();
            for state in 0..4 {
                let styles = if state < 3 {
                    default_stylesheet().clone()
                } else {
                    default_stylesheet().with_overrides(br##"{"tokens":{"colors":{"panel":"#345678","text":"#abcdef","field":"#345678"}}}"##).unwrap()
                };
                actual.stylesheet = Some(Arc::new(styles));
                actual.clear();
                dropdown.open = state != 0;
                let area = Rect::new(8.25, 8.5, width - 30., 24.);
                let input = Input {
                    mouse: if state == 2 { [30., 75.] } else { [-1., -1.] },
                    wheel: if state == 2 { [0., 1.] } else { [0., 0.] },
                    keys: if state == 3 {
                        vec![(Key::Down, Default::default())]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                };
                assert!(dropdown
                    .draw(
                        &mut actual,
                        &mut atlas,
                        area,
                        Rect::new(0., 0., width, 260.),
                        &items,
                        0,
                        &input,
                        true,
                        true
                    )
                    .is_none());
                let cx = area.max[0] - 11.;
                let cy = area.min[1] + area.height() * 0.5;
                let expected_arrow = [[cx - 3., cy - 1.], [cx, cy + 2.], [cx + 3., cy - 1.]];
                let mut arrows = 0;
                let mut expected = PaintList {
                    stylesheet: actual.stylesheet.clone(),
                    colors: actual.colors.clone(),
                    glyphs: actual.glyphs.clone(),
                    images: actual.images.clone(),
                    order: actual.order.clone(),
                    shapes: actual
                        .shapes
                        .iter()
                        .map(|shape| {
                            if let Shape::Line3 { points, clip } = shape {
                                assert_eq!(*points, expected_arrow);
                                arrows += 1;
                                Shape::Line {
                                    points: expected_arrow.to_vec(),
                                    clip: *clip,
                                }
                            } else {
                                shape.clone()
                            }
                        })
                        .collect(),
                };
                assert_eq!(arrows, 1);
                let clip = Rect::new(1., 1., width - 2., 258.);
                actual.clip_to(clip);
                expected.clip_to(clip);
                let config = atlas.config();
                painter
                    .upload_atlas(
                        &device,
                        &queue,
                        atlas.pixel_data(),
                        config.width,
                        config.height,
                    )
                    .unwrap();
                let pixels =
                    capture::capture(&device, &queue, &mut painter, &actual, [width, 260.], ratio);
                let reference = capture::capture(
                    &device,
                    &queue,
                    &mut painter,
                    &expected,
                    [width, 260.],
                    ratio,
                );
                assert_eq!(
                    pixels, reference,
                    "dropdown ratio={ratio} width={width} state={state}"
                );
                for (shape, color) in expected.shapes.iter().zip(&mut expected.colors) {
                    if matches!(shape, Shape::Line { .. }) {
                        *color = Color([0.; 4]);
                    }
                }
                assert_ne!(
                    capture::capture(
                        &device,
                        &queue,
                        &mut painter,
                        &expected,
                        [width, 260.],
                        ratio
                    ),
                    pixels,
                    "missing arrow must visibly fail"
                );
                comparisons += 1;
                if ratio == 1. && width == 190. {
                    save(
                        &format!("dropdown-state-{state}.png"),
                        &pixels,
                        [width, 260.],
                        ratio,
                    );
                }
            }
        }
    }
    println!("dropdown complete-image comparisons={comparisons}, missing-arrow negative controls={comparisons}");
    assert_eq!(comparisons, 40);
}

fn save(name: &str, pixels: &[u8], logical: [f32; 2], ratio: f32) {
    if let Some(path) = std::env::var_os("MOTION_PANEL_PAINT_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        capture::save(path.join(name), pixels, logical, ratio);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires gpu-tests")]
fn inline_three_point_strokes_keep_partial_and_empty_clip_pixels() {
    let (device, queue) = gpu();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let points = [[-2.25, 30.5], [30.25, 50.5], [62.25, 30.5]];
    let color = Color([0.3, 0.6, 0.9, 0.5]);
    let clips = [
        Rect::new(0., 0., 64., 64.),
        Rect::new(8., 24., 40., 25.),
        Rect::new(12., 39., 22., 12.),
        Rect::new(0., 0., 64., 20.),
    ];
    let mut comparisons = 0;
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        for clip in clips {
            let mut actual = PaintList::default();
            actual.push_shape(Shape::Line3 { points, clip: None }, color);
            let mut expected = PaintList::default();
            paint::line(&mut expected, points.to_vec(), color);
            actual.clip_to(clip);
            expected.clip_to(clip);
            let pixels = capture::capture(&device, &queue, &mut painter, &actual, [64.; 2], ratio);
            assert_eq!(
                pixels,
                capture::capture(&device, &queue, &mut painter, &expected, [64.; 2], ratio)
            );
            if clip.max[1] == 20. {
                assert!(
                    pixels.iter().all(|byte| *byte == 0),
                    "outside strokes must leave no edge ghosts"
                );
            } else {
                assert!(pixels.iter().any(|byte| *byte != 0));
            }
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 20);
    println!("inline stroke partial/empty-clip comparisons={comparisons}");
}
