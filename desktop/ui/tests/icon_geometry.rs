use motion_ui::{
    icons::{self, Icon},
    paint::{PaintList, Shape},
    Color, Rect,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
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
fn warmed_icons_allocate_only_their_owned_triangle_output() {
    let mut list = PaintList::default();
    let mut draw = |count: usize| {
        list.clear();
        for i in 0..count {
            icons::draw(
                &mut list,
                Rect::new(i as f32, 2.25, 24., 20.),
                Icon::Star,
                20.,
                Color::grey(220),
            );
        }
    };
    draw(200);
    let one = allocations(|| draw(1));
    let many = allocations(|| draw(200));
    println!("icon allocations: one={one} two_hundred={many}");
    assert_eq!(
        (one, many),
        (1, 200),
        "only the public owned triangle output remains per icon"
    );
    assert_eq!(list.shapes.len(), 200);
    assert!(list
        .shapes
        .iter()
        .all(|s| matches!(s, Shape::Triangles {points} if !points.is_empty())));
}

#[test]
fn triangle_clip_temporaries_do_not_scale_with_triangle_count() {
    let input = |count: usize| {
        let mut list = PaintList::default();
        list.push_shape(
            Shape::Triangles {
                points: [[-4., -4.], [14., 0.], [4., 14.]]
                    .into_iter()
                    .cycle()
                    .take(count * 3)
                    .collect(),
            },
            Color::grey(220),
        );
        list
    };
    let mut one = input(1);
    let mut many = input(200);
    let area = Rect::new(0., 0., 10., 10.);
    let single = allocations(|| one.clip_to(area));
    let repeated = allocations(|| many.clip_to(area));
    println!("crossing triangle clip allocations: one={single} two_hundred={repeated}");
    assert!(
        repeated <= single + 3,
        "per-triangle temporary allocation growth: one={single} many={repeated}"
    );
    let Shape::Triangles { points: a } = &one.shapes[0] else {
        panic!("triangle output")
    };
    let Shape::Triangles { points: b } = &many.shapes[0] else {
        panic!("triangle output")
    };
    assert!(!a.is_empty());
    assert_eq!(b.len(), a.len() * 200);
    assert!(b.chunks_exact(a.len()).all(|triangle| triangle == a));
}
