use motion_ui::{
    paint::{self, Glyph, PaintList, PaintOp, Pattern, Shape},
    theme::Color,
    Rect,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! { static COUNT: Cell<Option<usize>> = const { Cell::new(None) }; }
struct Allocator;
fn note() {
    let _ = COUNT.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value.saturating_add(1)));
        }
    });
}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            note();
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            note();
        }
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        if !pointer.is_null() {
            note();
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn allocations(work: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            let _ = COUNT.try_with(|count| count.set(None));
        }
    }
    COUNT.with(|count| count.set(Some(0)));
    let reset = Reset;
    work();
    let count = COUNT.with(|count| count.get().unwrap());
    drop(reset);
    count
}

#[test]
fn repeated_quad_clips_reuse_shape_and_color_storage() {
    assert!(
        allocations(|| {
            black_box(Box::new(7u64));
        }) > 0
    );
    let color = Color::rgb(20, 30, 40);
    let mut list = PaintList::default();
    for _ in 0..256 {
        paint::rect(&mut list, Rect::new(1., 1., 2., 2.), color);
    }
    let pointers = (list.shapes.as_ptr(), list.colors.as_ptr());
    let capacities = (list.shapes.capacity(), list.colors.capacity());
    let count = allocations(|| {
        for _ in 0..20 {
            black_box(&mut list).clip_to(Rect::new(0., 0., 10., 10.));
        }
    });
    println!("20 clips / 256 retained quads: {count} allocation requests");
    // The temporary shape-index map may allocate once per clip. Retaining
    // existing shapes/colors must not allocate their storage again.
    assert!(count <= 20, "unnecessary shape/color allocations: {count}");
    assert_eq!((list.shapes.as_ptr(), list.colors.as_ptr()), pointers);
    assert_eq!((list.shapes.capacity(), list.colors.capacity()), capacities);
    assert_eq!(list.shapes.len(), 256);
    assert_eq!(list.colors, vec![color; 256]);
    assert_eq!(list.order, (0..256).map(PaintOp::Shape).collect::<Vec<_>>());
}

#[test]
fn mixed_clipping_preserves_colors_glyph_uvs_and_creation_order() {
    let color = |n| Color::rgb(n, n, n);
    let glyph = |x| Glyph {
        uv: [0., 0., 1., 1.],
        atlas_size: Some([64, 64]),
        position: [x, 2.],
        size: [8., 4.],
        color: color(200),
    };
    let area = Rect::new(0., 0., 10., 10.);
    let mut list = PaintList::default();
    paint::rect(&mut list, Rect::new(20., 20., 2., 2.), color(1));
    list.push_glyph(glyph(-2.));
    list.push_shape(
        Shape::Procedural {
            area: Rect::new(-2., -2., 4., 4.),
            clip: Rect::new(-2., -2., 4., 4.),
            kind: Pattern::Diamond,
        },
        color(2),
    );
    list.push_glyph(glyph(30.));
    list.push_shape(
        Shape::Line {
            points: vec![[-4., 2.], [12., 2.]],
            clip: None,
        },
        color(3),
    );
    list.push_shape(
        Shape::Triangles {
            points: vec![[-2., 2.], [4., 2.], [4., 8.]],
        },
        color(4),
    );
    list.push_shape(
        Shape::Stroke {
            points: vec![[1., 1.], [3., 3.]],
            clip: Some(Rect::new(20., 20., 2., 2.)),
            width: 2.,
        },
        color(5),
    );
    paint::rect(&mut list, Rect::new(8., 8., 4., 4.), color(6));
    list.clip_to(area);
    assert_eq!(list.colors, vec![color(2), color(3), color(4), color(6)]);
    assert_eq!(
        list.order,
        vec![
            PaintOp::Glyph(0),
            PaintOp::Shape(0),
            PaintOp::Shape(1),
            PaintOp::Shape(2),
            PaintOp::Shape(3)
        ]
    );
    assert!(
        matches!(&list.shapes[0], Shape::Procedural {clip,..} if *clip == Rect::new(0.,0.,2.,2.))
    );
    assert!(
        matches!(&list.shapes[1], Shape::Line {points,clip} if points == &vec![[-4.,2.],[12.,2.]] && *clip == Some(area))
    );
    for shape in &list.shapes[2..] {
        let Shape::Triangles { points } = shape else {
            panic!("expected clipped triangles")
        };
        assert!(!points.is_empty());
        assert!(points
            .iter()
            .all(|p| (0. ..=10.).contains(&p[0]) && (0. ..=10.).contains(&p[1])));
    }
    assert_eq!(list.glyphs.len(), 1);
    let glyph = &list.glyphs[0];
    assert_eq!(glyph.uv, [0.25, 0., 1., 1.]);
    assert_eq!(glyph.position, [0., 2.]);
    assert_eq!(glyph.size, [6., 4.]);
    assert_eq!(glyph.atlas_size, Some([64, 64]));
    assert_eq!(glyph.color, color(200));
}

#[test]
fn unequal_public_shape_color_buffers_keep_the_existing_zip_truncation() {
    for extra_colors in [false, true] {
        let mut list = PaintList::default();
        for _ in 0..3 {
            paint::rect(&mut list, Rect::new(1., 1., 2., 2.), Color::rgb(1, 2, 3));
        }
        if extra_colors {
            list.colors.push(Color::rgb(9, 9, 9));
        } else {
            list.colors.truncate(2);
        }
        list.order
            .extend([PaintOp::Shape(999), PaintOp::Glyph(999)]);
        list.clip_to(Rect::new(0., 0., 10., 10.));
        let expected = if extra_colors { 3 } else { 2 };
        assert_eq!(list.shapes.len(), expected);
        assert_eq!(list.colors.len(), expected);
        assert_eq!(
            list.order,
            (0..expected).map(PaintOp::Shape).collect::<Vec<_>>()
        );
    }
}

#[test]
fn fully_contained_icon_clips_keep_triangle_storage() {
    use motion_ui::icons::{self, Icon};
    let mut list = PaintList::default();
    for &icon in Icon::ALL {
        icons::draw(
            &mut list,
            Rect::new(10., 10., 24., 24.),
            icon,
            20.,
            Color::grey(220),
        );
    }
    let area = Rect::new(0., 0., 100., 100.);
    let buffers: Vec<_> = list
        .shapes
        .iter()
        .map(|shape| {
            let Shape::Triangles { points } = shape else {
                panic!("icon triangles")
            };
            assert!(!points.is_empty() && points.len() % 3 == 0);
            assert!(points.iter().all(|p| area.contains(*p)));
            (points.as_ptr(), points.capacity())
        })
        .collect();
    let count = allocations(|| {
        for _ in 0..20 {
            black_box(&mut list).clip_to(area);
        }
    });
    println!(
        "20 clips / {} contained icons: {count} allocation requests",
        buffers.len()
    );
    // Only the existing shape-index map may allocate, once per clip.
    assert!(
        count <= 20,
        "unnecessary contained triangle allocations: {count}"
    );
    for (shape, expected) in list.shapes.iter().zip(buffers) {
        let Shape::Triangles { points } = shape else {
            panic!("icon triangles")
        };
        assert_eq!((points.as_ptr(), points.capacity()), expected);
    }
}

#[test]
fn contained_triangles_still_drop_trailing_incomplete_vertices() {
    let triangle = [[1., 1.], [4., 1.], [1., 4.]];
    for trailing in [0, 1, 2] {
        let mut points = triangle.to_vec();
        points.extend([[2., 2.], [3., 3.]][..trailing].iter().copied());
        let mut list = PaintList::default();
        list.push_shape(Shape::Triangles { points }, Color::grey(220));
        list.clip_to(Rect::new(0., 0., 10., 10.));
        let Shape::Triangles { points } = &list.shapes[0] else {
            panic!("triangle output")
        };
        assert_eq!(points, &triangle);
        assert_eq!(list.order, vec![PaintOp::Shape(0)]);
    }
    let mut list = PaintList::default();
    list.push_shape(
        Shape::Triangles {
            points: vec![[1., 1.], [2., 2.]],
        },
        Color::grey(220),
    );
    list.clip_to(Rect::new(0., 0., 10., 10.));
    assert!(list.shapes.is_empty() && list.colors.is_empty() && list.order.is_empty());
}

#[test]
fn unchanged_glyph_retention_does_not_allocate_an_index_map() {
    assert!(
        allocations(|| {
            black_box(Box::new(7u64));
        }) > 0
    );
    let mut list = PaintList::default();
    for i in 0..256 {
        list.push_glyph(Glyph {
            uv: [0., 0., 1., 1.],
            atlas_size: Some([64, 64]),
            position: [i as f32, 1.],
            size: [8., 12.],
            color: Color::grey(220),
        });
    }
    let order = list.order.clone();
    let glyphs = format!("{:?}", list.glyphs);
    let storage = (list.glyphs.as_ptr(), list.glyphs.capacity());
    let mut calls = 0;
    let count = allocations(|| {
        for _ in 0..20 {
            black_box(&mut list).retain_glyphs(|_| {
                calls += 1;
                true
            });
        }
    });
    println!("20 unchanged retentions / 256 glyphs: {count} allocation requests");
    assert_eq!(count, 0, "unchanged glyph indices need no temporary map");
    assert_eq!(calls, 20 * 256);
    assert_eq!(list.order, order);
    assert_eq!(format!("{:?}", list.glyphs), glyphs);
    assert_eq!((list.glyphs.as_ptr(), list.glyphs.capacity()), storage);
}

#[test]
fn clips_without_removal_keep_mixed_order_and_crop_uvs_without_allocating() {
    assert!(allocations(|| { black_box(Box::new(7u64)); }) > 0);
    let color = Color::rgb(20, 30, 40);
    let mut list = PaintList::default();
    paint::rect(&mut list, Rect::new(1., 1., 2., 2.), color);
    list.push_glyph(Glyph {
        uv: [0., 0., 1., 1.], atlas_size: Some([64, 64]),
        position: [-2., 2.], size: [8., 4.], color,
    });
    list.colors.push(Color::grey(99));
    list.order = vec![PaintOp::Glyph(0), PaintOp::Shape(0),
        PaintOp::Glyph(usize::MAX), PaintOp::Shape(1), PaintOp::Glyph(0)];
    let pointers = (list.shapes.as_ptr(), list.colors.as_ptr(), list.glyphs.as_ptr(), list.order.as_ptr());
    let count = allocations(|| {
        for _ in 0..20 {
            black_box(&mut list).clip_to(Rect::new(0., 0., 10., 10.));
        }
    });
    println!("20 clips / retained shape and cropped glyph: {count} allocation requests");
    assert_eq!(count, 0, "unchanged paint indices need no temporary maps");
    assert_eq!(list.colors, vec![color]);
    assert_eq!(list.order, vec![PaintOp::Glyph(0), PaintOp::Shape(0), PaintOp::Glyph(0)]);
    assert_eq!(list.glyphs[0].position, [0., 2.]);
    assert_eq!(list.glyphs[0].size, [6., 4.]);
    assert_eq!(list.glyphs[0].uv, [0.25, 0., 1., 1.]);
    assert_eq!(list.glyphs[0].atlas_size, Some([64, 64]));
    assert_eq!((list.shapes.as_ptr(), list.colors.as_ptr(), list.glyphs.as_ptr(), list.order.as_ptr()), pointers);
}

#[test]
fn every_removal_position_preserves_visible_command_identity_with_raw_buffers() {
    let color = |glyph: bool, index: usize| Color::rgb(if glyph { 101 + index as u8 } else { 1 + index as u8 }, 0, 0);
    let order = vec![PaintOp::Glyph(4), PaintOp::Shape(3), PaintOp::Shape(0),
        PaintOp::Glyph(1), PaintOp::Shape(4), PaintOp::Glyph(0), PaintOp::Shape(1),
        PaintOp::Glyph(3), PaintOp::Shape(2), PaintOp::Glyph(2), PaintOp::Shape(0),
        PaintOp::Glyph(4), PaintOp::Shape(5), PaintOp::Glyph(5),
        PaintOp::Shape(usize::MAX), PaintOp::Glyph(usize::MAX)];
    for shape_bits in 0..32 {
        for glyph_bits in 0..32 {
            for color_count in [0, 2, 5, 7] {
                let mut list = PaintList::default();
                for i in 0..5 {
                    let x = if shape_bits & (1 << i) != 0 { 1. + i as f32 } else { 20. + i as f32 };
                    paint::rect(&mut list, Rect::new(x, 1., 2., 2.), color(false, i));
                    let x = if glyph_bits & (1 << i) != 0 { 1. + i as f32 } else { 20. + i as f32 };
                    list.push_glyph(Glyph { uv: [0., 0., 1., 1.], atlas_size: None,
                        position: [x, 4.], size: [1.; 2], color: color(true, i) });
                }
                list.colors = (0..color_count).map(|i| color(false, i)).collect();
                list.order = order.clone();
                let expected: Vec<_> = order.iter().filter_map(|op| {
                    let (glyph, i, y) = match *op {
                        PaintOp::Shape(i) if i < 5 && i < color_count && shape_bits & (1 << i) != 0 => (false, i, 1.),
                        PaintOp::Glyph(i) if i < 5 && glyph_bits & (1 << i) != 0 => (true, i, 4.),
                        _ => return None,
                    };
                    Some((color(glyph, i), [1. + i as f32, y]))
                }).collect();
                list.clip_to(Rect::new(0., 0., 10., 10.));
                let actual: Vec<_> = list.order.iter().map(|op| match *op {
                    PaintOp::Shape(i) => {
                        let Shape::Quad { points } = &list.shapes[i] else { panic!("visible unit quad changed representation") };
                        (list.colors[i], points[0])
                    }
                    PaintOp::Glyph(i) => (list.glyphs[i].color, list.glyphs[i].position),
                }).collect();
                assert_eq!(actual, expected, "shape_bits={shape_bits} glyph_bits={glyph_bits} colors={color_count}");
                assert_eq!(list.shapes.len(), list.colors.len());
            }
        }
    }
}
