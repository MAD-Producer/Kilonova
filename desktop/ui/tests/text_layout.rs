use motion_ui::text::TextAtlas;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};

struct Counter;
thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
fn record() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
struct Scope;
impl Drop for Scope {
    fn drop(&mut self) {
        ALLOCATIONS.with(|c| c.set(None));
    }
}
fn allocations(run: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|c| {
        assert!(c.get().is_none());
        c.set(Some(0));
    });
    let scope = Scope;
    run();
    let count = ALLOCATIONS.with(|c| c.get().unwrap());
    drop(scope);
    count
}

#[test]
fn cached_layouts_for_one_or_two_hundred_controls_allocate_nothing() {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    for label in [
        "Position",
        "cafe\u{301} / 中文",
        "ABC سلام 123",
        "A🇯🇵👩\u{200d}💻",
    ] {
        let initial = atlas.layout_line(label).unwrap();
        assert!(initial.advance > 0.);
        for count in [1, 200] {
            let allocated = allocations(|| {
                for _ in 0..count {
                    let actual = atlas.layout_line(label).unwrap();
                    assert!(Arc::ptr_eq(&initial, &actual));
                    std::hint::black_box(actual);
                }
            });
            assert_eq!(
                allocated, 0,
                "retained single-line layout must not allocate per control"
            );
        }
    }
}

#[test]
fn cached_fitted_layouts_keep_complete_graphemes_without_per_control_allocations() {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    for label in [
        "Position / 中文 long label",
        "cafe\u{301} long label",
        "ABC سلام 123",
        "A🇯🇵👩\u{200d}💻 long label",
    ] {
        for width in [2., 6., 100.] {
            let initial = atlas.fit_line(label, width).unwrap();
            assert!(initial.layout.advance <= width);
            for count in [1, 200] {
                assert_eq!(
                    allocations(|| {
                        for _ in 0..count {
                            let actual = atlas.fit_line(label, width).unwrap();
                            assert!(Arc::ptr_eq(&initial.layout, &actual.layout));
                            std::hint::black_box(actual);
                        }
                    }),
                    0,
                    "fitted layout cache hits may not allocate per control"
                );
            }
        }
    }
}
