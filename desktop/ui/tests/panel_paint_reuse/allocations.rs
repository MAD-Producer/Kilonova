use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! { static COUNT: Cell<Option<(usize, usize)>> = const { Cell::new(None) }; }
struct Allocator;
fn note(bytes: usize) {
    let _ = COUNT.try_with(|count| {
        if let Some((requests, total)) = count.get() {
            count.set(Some((
                requests.saturating_add(1),
                total.saturating_add(bytes),
            )));
        }
    });
}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            note(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            note(layout.size());
        }
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        if !pointer.is_null() {
            note(size);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

pub(super) fn measure(work: impl FnOnce()) -> (usize, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            let _ = COUNT.try_with(|count| count.set(None));
        }
    }
    COUNT.with(|count| count.set(Some((0, 0))));
    let reset = Reset;
    work();
    let result = COUNT.with(|count| count.get().unwrap());
    drop(reset);
    result
}
