use super::{capture, gpu};
use motion_ui::{
    paint::{self, PaintList, Painter},
    Color, Rect,
};

#[test]
fn invalid_checker_sizes_are_empty_and_tiny_cells_use_one_command() {
    for size in [0., -0., -1., f32::NEG_INFINITY, f32::INFINITY, f32::NAN] {
        let mut list = PaintList::default();
        paint::checkerboard(
            &mut list,
            Rect::new(0., 0., 64., 64.),
            Color([1.; 4]),
            Color([0., 0., 0., 1.]),
            size,
        );
        assert!(list.shapes.is_empty(), "invalid size {size}");
        assert!(list.colors.is_empty());
        assert!(list.order.is_empty());
    }
    for extent in [64., 1_000_000.] {
        for size in [
            f32::from_bits(1),
            f32::MIN_POSITIVE,
            0.125,
            0.75,
            1.,
            8.,
            f32::MAX,
        ] {
            let mut list = PaintList::default();
            paint::checkerboard(
                &mut list,
                Rect::new(0., 0., extent, extent),
                Color([1.; 4]),
                Color([0., 0., 0., 1.]),
                size,
            );
            assert_eq!(list.shapes.len(), 1, "extent={extent} size={size}");
            assert_eq!(list.colors.len(), 1);
            assert_eq!(list.order.len(), 1);
        }
    }
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires gpu-tests; run with --features gpu-tests"
)]
fn checker_size_guard_and_shader_lower_bound_preserve_real_pixels() {
    let (device, queue) = gpu();
    let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let area = Rect::new(4., 4., 48., 48.);
    let pattern = |size| {
        let mut list = PaintList::default();
        paint::checkerboard(
            &mut list,
            area,
            Color([1.; 4]),
            Color([0., 0., 0., 1.]),
            size,
        );
        list
    };
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        for size in [0., -0., -1., f32::NEG_INFINITY, f32::INFINITY, f32::NAN] {
            let pixels = capture(
                &device,
                &queue,
                &mut painter,
                &pattern(size),
                [64.; 2],
                ratio,
            );
            assert!(
                pixels.iter().all(|channel| *channel == 0),
                "size={size} ratio={ratio}"
            );
        }
        let minimum = capture(&device, &queue, &mut painter, &pattern(1.), [64.; 2], ratio);
        assert!(minimum
            .chunks_exact(4)
            .any(|pixel| pixel == [255, 255, 255, 255]));
        assert!(minimum.chunks_exact(4).any(|pixel| pixel == [0, 0, 0, 255]));
        for size in [f32::from_bits(1), f32::MIN_POSITIVE, 0.125, 0.75] {
            assert_eq!(
                capture(
                    &device,
                    &queue,
                    &mut painter,
                    &pattern(size),
                    [64.; 2],
                    ratio
                ),
                minimum,
                "size={size} ratio={ratio}"
            );
        }
        // At integer DPI every cell edge lies on a pixel boundary. Compare the
        // whole image with an independent CPU raster, including transparent
        // pixels outside the requested area, rather than another shader path.
        if ratio == 1. || ratio == 2. {
            let width = (64. * ratio) as usize;
            for size in [1., 2., 8., f32::MAX] {
                let pixels = capture(
                    &device,
                    &queue,
                    &mut painter,
                    &pattern(size),
                    [64.; 2],
                    ratio,
                );
                let mut expected = Vec::with_capacity(width * width * 4);
                for y in 0..width {
                    for x in 0..width {
                        let lx = (x as f32 + 0.5) / ratio;
                        let ly = (y as f32 + 0.5) / ratio;
                        let pixel = if (4. ..52.).contains(&lx) && (4. ..52.).contains(&ly) {
                            let cell_x = ((lx - 4.) / size).floor() as u32;
                            let cell_y = ((ly - 4.) / size).floor() as u32;
                            if (cell_x + cell_y) % 2 == 0 {
                                [255; 4]
                            } else {
                                [0, 0, 0, 255]
                            }
                        } else {
                            [0; 4]
                        };
                        expected.extend_from_slice(&pixel);
                    }
                }
                assert_eq!(pixels, expected, "size={size} ratio={ratio}");
            }
        }
        assert!(pollster::block_on(scope.pop()).is_none());
    }
}
