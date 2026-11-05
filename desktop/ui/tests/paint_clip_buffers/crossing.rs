use super::*;
use motion_ui::paint::{Bitmap, ImageQuad};

#[test]
fn glyph_callback_panic_discards_temporary_maps_and_allows_next_frame() {
    let mut list = PaintList::default();
    for n in 0..4 {
        list.push_glyph(Glyph {
            uv: [0., 0., 1., 1.],
            atlas_size: None,
            position: [n as f32, 2.],
            size: [2., 2.],
            color: Color::grey(220),
        });
    }
    let original = list.order.clone();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        list.retain_glyphs(|glyph| {
            assert_ne!(glyph.position[0], 2., "controlled glyph callback panic");
            glyph.position[0] != 0.
        });
    }));
    assert!(panic.is_err(), "caller panic must propagate");
    assert_eq!(
        list.order, original,
        "temporary map tags escaped during unwind"
    );
    list.clear();
    paint::rect(&mut list, Rect::new(1., 1., 2., 2.), Color::grey(220));
    list.clip_to(Rect::new(0., 0., 10., 10.));
    assert_eq!(list.order, [PaintOp::Shape(0)]);
}

#[test]
fn invalid_glyph_indices_still_panic_with_and_without_remapping() {
    for keep in [false, true] {
        let mut list = PaintList::default();
        for _ in 0..2 {
            list.push_glyph(Glyph {
                uv: [0., 0., 1., 1.],
                atlas_size: None,
                position: [1., 2.],
                size: [2., 2.],
                color: Color::grey(220),
            });
        }
        list.order[1] = PaintOp::Glyph(usize::MAX);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            list.retain_glyphs(|_| keep);
        }));
        assert!(
            panic.is_err(),
            "invalid glyph index must not silently disappear"
        );
        assert!(
            list.order.len() <= 2,
            "temporary maps survived an index panic"
        );
    }
}

#[test]
fn raw_geometry_without_order_keeps_legacy_empty_order_after_remapping() {
    let bitmap = Bitmap::new(1, 1, vec![255; 4]).unwrap();
    let mut list = PaintList::default();
    for x in [20., 1.] {
        paint::rect(&mut list, Rect::new(x, 1., 2., 2.), Color::grey(x as u8));
        list.push_glyph(Glyph {
            uv: [0., 0., 1., 1.],
            atlas_size: None,
            position: [x, 2.],
            size: [2., 2.],
            color: Color::grey(220),
        });
        list.push_image(ImageQuad::new(bitmap.clone(), Rect::new(x, 2., 2., 2.)));
    }
    list.order.clear();
    list.clip_to(Rect::new(0., 0., 10., 10.));
    assert!(list.order.is_empty());
    assert_eq!(list.colors, [Color::grey(1)]);
    assert_eq!(
        (list.shapes.len(), list.glyphs.len(), list.images.len()),
        (1, 1, 1)
    );
}

fn clipped_list(bitmap: &std::sync::Arc<Bitmap>, list: &mut PaintList) {
    list.clear();
    for x in [20., 1.] {
        paint::rect(list, Rect::new(x, 1., 2., 2.), Color::grey(220));
        list.push_glyph(Glyph {
            uv: [0., 0., 1., 1.],
            atlas_size: None,
            position: [x, 2.],
            size: [2., 2.],
            color: Color::grey(220),
        });
        list.push_image(ImageQuad::new(bitmap.clone(), Rect::new(x, 2., 2., 2.)));
    }
    list.clip_to(Rect::new(0., 0., 10., 10.));
}

#[test]
fn removed_commands_reuse_existing_order_storage_for_index_maps() {
    let bitmap = Bitmap::new(1, 1, vec![255; 4]).unwrap();
    let mut observations = vec![];
    for count in [1, 200] {
        let mut lists: Vec<_> = (0..count).map(|_| PaintList::default()).collect();
        for list in &mut lists {
            clipped_list(&bitmap, list);
        }
        let requests = allocations(|| {
            for list in &mut lists {
                clipped_list(&bitmap, list);
            }
        });
        println!("compacted_lists={count} allocations={requests}");
        observations.push(requests);
        for list in &lists {
            assert_eq!(
                list.order,
                [PaintOp::Shape(0), PaintOp::Glyph(0), PaintOp::Image(0)]
            );
            assert_eq!(
                (list.shapes.len(), list.glyphs.len(), list.images.len()),
                (1, 1, 1)
            );
        }
    }
    assert_eq!(
        observations,
        [0, 0],
        "warmed index maps must reuse the order buffer"
    );
}

#[test]
fn glyph_removal_reuses_existing_order_storage_and_preserves_other_ops() {
    let mut list = PaintList::default();
    let frame = |list: &mut PaintList| {
        list.clear();
        for n in 0..200 {
            list.push_glyph(Glyph {
                uv: [0., 0., 1., 1.],
                atlas_size: None,
                position: [n as f32, 2.],
                size: [2., 2.],
                color: Color::grey(220),
            });
        }
        list.retain_glyphs(|glyph| glyph.position[0] % 2. == 1.);
    };
    frame(&mut list);
    let requests = allocations(|| frame(&mut list));
    println!("compacted_glyphs=200 allocations={requests}");
    assert_eq!(
        requests, 0,
        "warmed glyph removal must reuse the order buffer"
    );
    assert_eq!(list.order, (0..100).map(PaintOp::Glyph).collect::<Vec<_>>());
    assert_eq!(
        list.glyphs
            .iter()
            .map(|g| g.position[0])
            .collect::<Vec<_>>(),
        (0..100).map(|n| (n * 2 + 1) as f32).collect::<Vec<_>>()
    );
}

#[test]
fn crossing_owned_triangles_reuse_their_existing_storage() {
    let triangle = [[-2., 2.], [6., 2.], [2., 6.]];
    let expected = [[0., 4.], [0., 2.], [6., 2.], [0., 4.], [6., 2.], [2., 6.]];
    for count in [1, 200] {
        let mut list = PaintList::default();
        for _ in 0..count {
            let mut points = Vec::with_capacity(64);
            points.extend_from_slice(&triangle);
            list.push_shape(Shape::Triangles { points }, Color::grey(220));
        }
        let frame = |list: &mut PaintList| {
            for shape in &mut list.shapes {
                let Shape::Triangles { points } = shape else {
                    panic!("triangle buffer lost")
                };
                points.clear();
                points.extend_from_slice(&triangle);
            }
            list.clip_to(Rect::new(0., 0., 10., 10.));
        };
        frame(&mut list);
        let storage: Vec<_> = list
            .shapes
            .iter()
            .map(|shape| {
                let Shape::Triangles { points } = shape else {
                    panic!("triangle buffer lost")
                };
                (points.as_ptr(), points.capacity())
            })
            .collect();
        let requests = allocations(|| frame(&mut list));
        println!("crossing_owned_shapes={count} allocations={requests}");
        assert_eq!(
            requests, 0,
            "clipping must reuse the owned triangle storage after warmup"
        );
        for (shape, stored) in list.shapes.iter().zip(storage) {
            let Shape::Triangles { points } = shape else {
                panic!("triangle buffer lost")
            };
            assert_eq!(points.as_slice(), expected);
            assert_eq!((points.as_ptr(), points.capacity()), stored);
        }
        assert_eq!(
            list.order,
            (0..count).map(PaintOp::Shape).collect::<Vec<_>>()
        );
    }
}

#[test]
fn retained_image_clips_need_no_index_map_allocations() {
    let bitmap = Bitmap::new(1, 1, vec![255; 4]).unwrap();
    let source = Rect::new(-2., 2., 8., 4.);
    for count in [1, 200] {
        let mut lists: Vec<_> = (0..count)
            .map(|_| {
                let mut list = PaintList::default();
                list.push_image(ImageQuad::new(bitmap.clone(), source));
                list
            })
            .collect();
        let mut frame = || {
            for list in &mut lists {
                list.images[0].area = source;
                list.images[0].uv = [0., 0., 1., 1.];
                list.clip_to(Rect::new(0., 0., 10., 10.));
            }
        };
        frame();
        let requests = allocations(frame);
        println!("retained_image_lists={count} allocations={requests}");
        assert_eq!(
            requests, 0,
            "unchanged image indices must not allocate maps per clipped list"
        );
        for list in &lists {
            assert_eq!(list.order, vec![PaintOp::Image(0)]);
            assert_eq!(list.images[0].area, Rect::new(0., 2., 6., 4.));
            assert_eq!(list.images[0].uv, [0.25, 0., 1., 1.]);
            assert!(std::sync::Arc::ptr_eq(&list.images[0].bitmap, &bitmap));
        }
    }
}

#[test]
fn every_image_removal_position_preserves_identity_order_and_stale_index_handling() {
    for mask in 0..32 {
        let mut list = PaintList::default();
        let bitmaps: Vec<_> = (0..5)
            .map(|n| Bitmap::new(1, 1, vec![n, 0, 0, 255]).unwrap())
            .collect();
        paint::rect(&mut list, Rect::new(1., 1., 2., 2.), Color::grey(220));
        list.push_glyph(Glyph {
            uv: [0., 0., 1., 1.],
            atlas_size: None,
            position: [2., 2.],
            size: [2., 2.],
            color: Color::grey(220),
        });
        for (i, bitmap) in bitmaps.iter().enumerate() {
            list.push_image(ImageQuad::new(
                bitmap.clone(),
                Rect::new(if mask & (1 << i) != 0 { -2. } else { 20. }, 2., 8., 4.),
            ));
        }
        let original = vec![
            PaintOp::Image(4),
            PaintOp::Shape(0),
            PaintOp::Image(1),
            PaintOp::Glyph(0),
            PaintOp::Image(0),
            PaintOp::Image(3),
            PaintOp::Image(2),
            PaintOp::Image(4),
            PaintOp::Image(5),
            PaintOp::Image(usize::MAX),
        ];
        let mut next = 0;
        let mapping: Vec<_> = (0..5)
            .map(|i| {
                if mask & (1 << i) != 0 {
                    let index = next;
                    next += 1;
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        let expected: Vec<_> = original
            .iter()
            .filter_map(|op| match op {
                PaintOp::Image(i) => mapping.get(*i).copied().flatten().map(PaintOp::Image),
                _ => Some(*op),
            })
            .collect();
        list.order = original;
        list.clip_to(Rect::new(0., 0., 10., 10.));
        assert_eq!(list.order, expected, "mask={mask}");
        assert_eq!(list.images.len(), next);
        for (old, new) in mapping.iter().enumerate() {
            if let Some(new) = *new {
                assert!(std::sync::Arc::ptr_eq(
                    &list.images[new].bitmap,
                    &bitmaps[old]
                ));
                assert_eq!(list.images[new].uv, [0.25, 0., 1., 1.]);
                assert_eq!(list.images[new].area, Rect::new(0., 2., 6., 4.));
            }
        }
    }
}
