//! Count successful allocations on the calling test thread, outside GPU setup.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
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
// SAFETY: every pointer, layout and size is delegated unchanged to System.
// The observer uses a const-initialized thread-local Cell and never allocates.
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
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() {
            note();
        }
        next
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

pub(crate) fn count(work: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            let _ = COUNT.try_with(|count| count.set(None));
        }
    }
    COUNT.with(|count| {
        assert!(count.get().is_none(), "allocation scopes must not nest");
        count.set(Some(0));
    });
    let reset = Reset;
    work();
    let count = COUNT.with(|count| count.get().unwrap());
    drop(reset);
    count
}
