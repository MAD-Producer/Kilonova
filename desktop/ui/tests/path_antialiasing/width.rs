use super::{capture, gpu};
use motion_ui::{
    paint::{self, PaintList, Painter, Shape},
    Color, Rect,
};

const SCALES: [f32; 5] = [1., 1.25, 1.5, 1.75, 2.];

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires gpu-tests; run with --features gpu-tests")]
fn explicit_one_pixel_strokes_match_existing_lines_including_clipping_and_paint_order() {
    let (device, queue) = gpu();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    for scale in SCALES {
        for clipped in [false, true] {
            let scene = |explicit: bool| {
                let mut list = PaintList::default();
                for (offset, alpha) in [(0., 1.), (3.25, 0.5)] {
                    let points = vec![
                        [-8., 9.25 + offset],
                        [24.25, 9.25 + offset],
                        [24.25, 9.25 + offset],
                        [41.25, 47.25],
                        [8.25, 47.25],
                    ];
                    let color = Color::rgba(80, 160, 240, alpha);
                    if explicit {
                        paint::line_with_width(&mut list, points, color, 1.);
                    } else {
                        paint::line(&mut list, points, color);
                    }
                    paint::rect(
                        &mut list,
                        Rect::new(22., 8., 4., 6.),
                        Color::rgba(240, 80, 60, 0.4),
                    );
                }
                if clipped {
                    list.clip_to(Rect::new(4., 4., 52., 52.));
                    list.clip_to(Rect::new(8., 8., 40., 40.));
                }
                list
            };
            let expected = capture(
                &device,
                &queue,
                &mut painter,
                &scene(false),
                [64.; 2],
                scale,
            );
            let actual = capture(&device, &queue, &mut painter, &scene(true), [64.; 2], scale);
            assert_eq!(
                actual, expected,
                "one-pixel compatibility at {scale}x / clip={clipped}"
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires gpu-tests; run with --features gpu-tests")]
fn requested_width_controls_gpu_coverage_across_dpi_angles_and_subpixel_offsets() {
    let (device, queue) = gpu();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    for scale in SCALES {
        for width in [0.5, 1., 2.5, 8.] {
            for (angle, offset) in [(0_f32, 0.), (0., 0.5), (0.35, 0.25), (0.8, 0.4)] {
                let length = 40.;
                let mut list = PaintList::default();
                paint::line_with_width(
                    &mut list,
                    vec![
                        [12. + offset, 12. + offset],
                        [
                            12. + offset + length * angle.cos(),
                            12. + offset + length * angle.sin(),
                        ],
                    ],
                    Color::grey(255),
                    width,
                );
                let pixels = capture(&device, &queue, &mut painter, &list, [64.; 2], scale);
                let area = pixels
                    .chunks_exact(4)
                    .map(|p| f64::from(p[3]) / 255.)
                    .sum::<f64>()
                    / f64::from(scale * scale);
                let expected = f64::from(length * width);
                assert!((area - expected).abs() < 2.5,
                    "width={width}, {scale}x, angle={angle}, offset={offset}: coverage={area}, expected={expected}");
                // An axis-aligned rectangle whose edges fall on physical
                // pixel boundaries correctly needs no fractional pixels.
                let edges = [
                    12. + offset,
                    52. + offset,
                    12. + offset - width * 0.5,
                    12. + offset + width * 0.5,
                ];
                if angle != 0. || edges.iter().any(|e| (e * scale).fract().abs() > 0.001) {
                    assert!(
                        pixels.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255),
                        "no AA at width={width}, {scale}x"
                    );
                }
                assert_eq!(painter.last_solid_vertices, 6);
                assert_eq!(painter.last_draw_calls, 1);
            }
        }
        if let Some(directory) = std::env::var_os("MOTION_UI_STROKE_ARTIFACTS") {
            let directory = std::path::Path::new(&directory);
            std::fs::create_dir_all(directory).unwrap();
            let mut list = PaintList::default();
            let styles = list.styles();
            paint::rect(
                &mut list,
                Rect::new(0., 0., 288., 160.),
                styles.color("panel"),
            );
            for (row, width) in [0.5, 1., 2.5, 8.].into_iter().enumerate() {
                let y = 20.25 + row as f32 * 32.;
                paint::line_with_width(
                    &mut list,
                    vec![[16., y], [80., y], [96., y + 12.], [144., y + 12.]],
                    styles.color("keyframe"),
                    width,
                );
                paint::line_with_width(
                    &mut list,
                    vec![
                        [184., y],
                        [260., y],
                        [260., y + 16.],
                        [184., y + 16.],
                        [184., y],
                    ],
                    styles.color("focus").with_alpha(0.5),
                    width,
                );
            }
            let pixels = capture(&device, &queue, &mut painter, &list, [288., 160.], scale);
            image::save_buffer(
                directory.join(format!("strokes-{scale}x.png")),
                &pixels,
                (288. * scale).round() as u32,
                (160. * scale).round() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires gpu-tests; run with --features gpu-tests")]
fn thick_open_and_closed_joins_do_not_double_blend_and_clipping_preserves_coverage() {
    let (device, queue) = gpu();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    for scale in SCALES {
        for width in [2.5, 8.] {
            for points in [
                vec![[8., 16.], [32., 16.], [32., 48.]],
                vec![[12., 12.], [48., 12.], [48., 48.], [12., 48.], [12., 12.]],
            ] {
                let mut list = PaintList::default();
                paint::line_with_width(&mut list, points, Color::rgba(255, 190, 70, 0.5), width);
                let pixels = capture(&device, &queue, &mut painter, &list, [64.; 2], scale);
                assert!(pixels.chunks_exact(4).any(|p| p[3] >= 127));
                assert!(
                    pixels.chunks_exact(4).all(|p| p[3] <= 129),
                    "overlapping join at width={width}, {scale}x"
                );
                // The center and both sides of the right-angle join are filled.
                let physical = (64. * scale).round() as usize;
                for [x, y] in [[32., 16.], [31.5, 16.5], [32.5, 15.5]] {
                    // Only the open path has a corner here.
                    if matches!(&list.shapes[0], Shape::Stroke { points, .. } if points.len() == 3)
                    {
                        let index = ((y * scale).floor() as usize * physical
                            + (x * scale).floor() as usize)
                            * 4;
                        assert!(
                            pixels[index + 3] >= 120,
                            "join hole at {x},{y}, width={width}, {scale}x"
                        );
                    }
                }
            }
            let scene = || {
                let mut list = PaintList::default();
                paint::line_with_width(
                    &mut list,
                    vec![[-20., 4.], [80., 48.]],
                    Color::grey(255),
                    width,
                );
                list
            };
            let mut list = scene();
            list.clip_to(Rect::new(8., 4., 40., 48.));
            list.clip_to(Rect::new(16., 8., 16., 32.));
            let clipped = capture(&device, &queue, &mut painter, &list, [64.; 2], scale);
            let original = capture(&device, &queue, &mut painter, &scene(), [64.; 2], scale);
            let physical = (64. * scale).round() as usize;
            for (i, pixel) in clipped.chunks_exact(4).enumerate() {
                let x = (i % physical) as f32 + 0.5;
                let y = (i / physical) as f32 + 0.5;
                if x < 16. * scale || x > 32. * scale || y < 8. * scale || y > 40. * scale {
                    assert_eq!(
                        pixel[3], 0,
                        "clip leak at width={width}, {scale}x ({x},{y})"
                    );
                } else {
                    assert!(
                        pixel
                            .iter()
                            .zip(&original[i * 4..i * 4 + 4])
                            .all(|(a, b)| a.abs_diff(*b) <= 1),
                        "clipping changed coverage at width={width}, {scale}x ({x},{y})"
                    );
                }
            }
        }
    }
}

#[test]
fn malformed_strokes_are_noops_and_repeated_points_preserve_a_single_connected_path() {
    for width in [0., -1., f32::NAN, f32::INFINITY] {
        let mut list = PaintList::default();
        paint::line_with_width(
            &mut list,
            vec![[1., 1.], [10., 10.]],
            Color::grey(255),
            width,
        );
        assert!(list.shapes.is_empty() && list.order.is_empty() && list.colors.is_empty());
    }
    for points in [
        vec![],
        vec![[1., 1.]],
        vec![[1., 1.]; 8],
        vec![[1., 1.], [f32::NAN, 2.]],
        vec![[f32::INFINITY, 1.], [2., 2.]],
    ] {
        let mut list = PaintList::default();
        paint::line_with_width(&mut list, points, Color::grey(255), 8.);
        assert!(list.shapes.is_empty() && list.order.is_empty() && list.colors.is_empty());
    }
    for color in [
        Color([1., 1., 1., 0.]),
        Color([f32::NAN, 1., 1., 1.]),
        Color([1., 1., 1., f32::NAN]),
    ] {
        let mut list = PaintList::default();
        paint::line_with_width(&mut list, vec![[1., 1.], [10., 10.]], color, 8.);
        assert!(list.shapes.is_empty());
    }
    let mut list = PaintList::default();
    paint::line_with_width(
        &mut list,
        vec![[1., 1.], [1., 1.], [10., 10.], [10., 10.]],
        Color::grey(255),
        8.,
    );
    assert!(
        matches!(&list.shapes[..], [Shape::Stroke {points, width: 8., clip: None}] if points == &[[1., 1.], [10., 10.]])
    );
}
