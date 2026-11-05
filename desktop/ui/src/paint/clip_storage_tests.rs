use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires gpu-tests")]
fn cropped_images_and_owned_triangles_match_independent_gpu_reference() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let bitmaps: Vec<_> = (0..5)
        .map(|n| {
            Bitmap::new(
                2,
                2,
                vec![
                    32 + n,
                    0,
                    0,
                    128,
                    0,
                    64 + n,
                    0,
                    128,
                    0,
                    0,
                    96 + n,
                    128,
                    64 + n,
                    64 + n,
                    64 + n,
                    128,
                ],
            )
            .unwrap()
        })
        .collect();
    let clip = Rect::new(2., 0., 10., 8.);
    let triangle = [[-2., 1.], [10., 1.], [4., 7.]];
    // Independent analytical result of clipping this triangle at x=2.
    let clipped = [[2., 5.], [2., 1.], [10., 1.], [2., 5.], [10., 1.], [4., 7.]];
    let background = Shape::Quad {
        points: [[2., 0.], [12., 0.], [12., 8.], [2., 8.]],
    };
    let colors = [Color([0.1, 0.2, 0.3, 1.]), Color([0.2, 0.4, 0.8, 0.5])];
    let order = [
        PaintOp::Image(4),
        PaintOp::Shape(0),
        PaintOp::Image(1),
        PaintOp::Shape(1),
        PaintOp::Image(0),
        PaintOp::Image(3),
        PaintOp::Image(2),
    ];
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let mut painter = Painter::new(&device, format);
        for scale in [1, 2] {
            for mask in 0..32 {
                let mut actual = PaintList::default();
                actual.push_shape(background.clone(), colors[0]);
                actual.push_shape(
                    Shape::Triangles {
                        points: triangle.to_vec(),
                    },
                    colors[1],
                );
                for (i, bitmap) in bitmaps.iter().enumerate() {
                    actual.push_image(ImageQuad::new(
                        bitmap.clone(),
                        Rect::new(
                            if mask & (1 << i) != 0 { -2. } else { 20. },
                            1. + i as f32 * 0.25,
                            8.,
                            4.,
                        ),
                    ));
                }
                actual.order = order.to_vec();
                let mut reference = PaintList::default();
                // Assemble already-cropped commands directly; never call clip_to.
                for op in order {
                    match op {
                        PaintOp::Shape(0) => reference.push_shape(background.clone(), colors[0]),
                        PaintOp::Shape(1) => reference.push_shape(
                            Shape::Triangles {
                                points: clipped.to_vec(),
                            },
                            colors[1],
                        ),
                        PaintOp::Image(i) if i < 5 && mask & (1 << i) != 0 => {
                            reference.push_image(ImageQuad {
                                bitmap: bitmaps[i].clone(),
                                area: Rect::new(2., 1. + i as f32 * 0.25, 4., 4.),
                                uv: [0.5, 0., 1., 1.],
                            });
                        }
                        _ => {}
                    }
                }
                let expected =
                    bindings_tests::capture(&device, &queue, &mut painter, &reference, scale);
                if mask == 31 {
                    let uncut =
                        bindings_tests::capture(&device, &queue, &mut painter, &actual, scale);
                    assert_ne!(
                        uncut, expected,
                        "negative control must visibly fail without clipping"
                    );
                }
                actual.clip_to(clip);
                let pixels = bindings_tests::capture(&device, &queue, &mut painter, &actual, scale);
                assert!(expected.chunks_exact(4).any(|p| p[3] > 0));
                assert_eq!(
                    pixels, expected,
                    "mask={mask} scale={scale} format={format:?}"
                );
                if mask == 31 && format == wgpu::TextureFormat::Rgba8Unorm {
                    if let Some(path) = std::env::var_os("MOTION_UI_CLIP_ARTIFACTS") {
                        let path = std::path::Path::new(&path);
                        std::fs::create_dir_all(path).unwrap();
                        for (name, bytes) in [("actual", &pixels), ("reference", &expected)] {
                            image::save_buffer(
                                path.join(format!("{name}-{scale}x.png")),
                                bytes,
                                16 * scale,
                                8 * scale,
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
