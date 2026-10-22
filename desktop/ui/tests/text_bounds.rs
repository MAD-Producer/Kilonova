use motion_ui::{
    paint::{self, PaintList, PaintOp},
    style::{self, default_stylesheet, States},
    text::TextAtlas,
    ui::{self, Align, ScrubField},
    Input, MouseButton, Rect,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
#[path = "text_bounds/gpu.rs"]
mod gpu;
#[path = "text_bounds/grapheme.rs"]
mod grapheme;

struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
// Count only allocations on the measuring test thread. All memory still goes
// through System, including allocations by the GPU and other test threads.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct AllocationScope;
impl Drop for AllocationScope {
    fn drop(&mut self) {
        ALLOCATIONS.with(|c| c.set(None));
    }
}
fn allocations(run: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|c| {
        assert!(c.get().is_none());
        c.set(Some(0));
    });
    let scope = AllocationScope;
    run();
    let count = ALLOCATIONS.with(|c| c.get().unwrap());
    drop(scope);
    count
}

#[test]
fn warmed_plain_and_styled_labels_do_not_allocate_per_control() {
    let mut atlas = TextAtlas::from_system_language(12., false).expect("system UI font");
    let sheet = default_stylesheet();
    let style = sheet.resolve("label", States::default()).unwrap();
    let mut list = PaintList::default();
    for label in [
        "Position",
        "A long label that must be ellipsized without creating temporary strings",
    ] {
        for width in [64., 240.] {
            let area = Rect::new(0., 0., width, 22.);
            let mut draw = |count| {
                list.clear();
                for _ in 0..count {
                    ui::text(
                        &mut list,
                        &mut atlas,
                        area,
                        label,
                        style.foreground,
                        Align::Left,
                    );
                    style::text_style(&mut list, &mut atlas, area, label, style, Align::Right);
                }
            };
            draw(200); // Populate glyph caches and retained output capacities.
            let one = allocations(|| draw(1));
            let two_hundred = allocations(|| draw(200));
            println!("label_width={width} chars={} allocations(one)={one} allocations(200)={two_hundred}", label.len());
            assert_eq!(
                (one, two_hundred),
                (0, 0),
                "warmed label drawing must not allocate per control"
            );
            assert!(!list.glyphs.is_empty());
        }
    }
}

#[test]
fn long_scrub_labels_stop_before_the_value_without_changing_interaction() {
    let mut atlas = TextAtlas::from_system_language(12., false).expect("system UI font");
    let colors = default_stylesheet().colors();
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        atlas.set_pixel_ratio(ratio);
        for width in [40., 100., 220.] {
            let area = Rect::new(10., 20., width, 22.);
            let boundary = area.min[0] + area.width() * 0.45;
            for dragging in [false, true] {
                let mut field = ScrubField {
                    dragging,
                    ..Default::default()
                };
                let input = Input {
                    mouse: [area.max[0] - 1., area.min[1] + 1.],
                    pressed: Some((MouseButton::Left, [area.max[0] - 1., area.min[1] + 1.])),
                    ..Default::default()
                };
                let mut list = PaintList::default();
                let response = ui::scrub_field(
                    &mut list,
                    &mut atlas,
                    area,
                    "A very long property label / 很长的属性名称",
                    25.,
                    &mut field,
                    &input,
                );
                assert!(response.hovered && response.clicked);
                assert_eq!(response.active, dragging);
                assert_eq!(field.dragging, dragging);
                let labels: Vec<_> = list
                    .glyphs
                    .iter()
                    .filter(|g| g.color == colors.TEXT_MUTED)
                    .collect();
                assert!(!labels.is_empty());
                for g in labels {
                    assert!(
                        g.position[0] >= area.min[0] && g.position[0] + g.size[0] <= boundary,
                        "label overlaps value: width={width} ratio={ratio} glyph={g:?}"
                    );
                }
                assert!(list.glyphs.iter().any(|g| g.color == colors.TEXT));
            }
        }
    }
}

#[test]
fn styled_text_keeps_legacy_cropping_uvs_and_interleaved_order() {
    let mut atlas = TextAtlas::from_system_language(12., false).expect("system UI font");
    let mut style = default_stylesheet()
        .resolve("label", States::default())
        .unwrap();
    style.font_size = 28.;
    for ratio in [1., 1.25, 1.5, 1.75, 2.] {
        atlas.set_pixel_ratio(ratio);
        for align in [Align::Left, Align::Center, Align::Right] {
            let area = Rect::new(10.25, 20.5, 80.5, 9.);
            let mut legacy = PaintList::default();
            ui::text_sized(
                &mut legacy,
                &mut atlas,
                area,
                "jW / 中文 label",
                style.foreground,
                align,
                style.font_size,
            );
            legacy.clip_to(area);
            let mut actual = PaintList::default();
            paint::rect(&mut actual, Rect::new(0., 0., 120., 80.), style.foreground);
            style::text_style(
                &mut actual,
                &mut atlas,
                area,
                "jW / 中文 label",
                style,
                align,
            );
            assert!(!legacy.glyphs.is_empty());
            assert_eq!(actual.glyphs.len(), legacy.glyphs.len());
            for (a, b) in actual.glyphs.iter().zip(&legacy.glyphs) {
                assert_eq!(a.position, b.position);
                assert_eq!(a.size, b.size);
                assert_eq!(a.uv, b.uv);
                assert_eq!(a.color, b.color);
            }
            assert_eq!(actual.order[0], PaintOp::Shape(0));
            assert!(actual.order[1..]
                .iter()
                .enumerate()
                .all(|(i, op)| *op == PaintOp::Glyph(i)));
        }
    }
}
