use super::*;
use motion_ui::paint::{Bitmap, ImageQuad};

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
