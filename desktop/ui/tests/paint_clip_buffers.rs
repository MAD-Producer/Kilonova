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
