#[path = "../src/panels/frame_cache.rs"]
mod frame_cache;

use frame_cache::{Cache, MAX_ITEMS};
use motion_ui::dock::{DockId, Edge, Node};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! { static ALLOCATIONS:Cell<Option<usize>>=const {Cell::new(None)}; }
struct Counter;
fn note() {
    let _ = ALLOCATIONS.try_with(|value| {
        if let Some(count) = value.get() {
            value.set(Some(count.saturating_add(1)));
        }
    });
}
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            note();
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            note();
        }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, layout, size) };
        if !p.is_null() {
            note();
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
fn allocations(work: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            let _ = ALLOCATIONS.try_with(|value| value.set(None));
        }
    }
    ALLOCATIONS.with(|value| value.set(Some(0)));
    let reset = Reset;
    work();
    let count = ALLOCATIONS.with(|value| value.get().unwrap());
    drop(reset);
    count
}
fn layout() -> Node {
    Node::Split {
        edge: Edge::Left,
        split: 0.5,
        first: Box::new(Node::Dock {
            id: DockId(1),
            tabs: vec![DockId(1), DockId(2)],
            active: 0,
        }),
        second: Box::new(Node::Fixed {
            edge: Edge::Bottom,
            size: 20.,
        }),
    }
}

#[test]
fn warmed_owned_results_have_only_the_one_required_vector_allocation() {
    assert!(
        allocations(|| {
            black_box(String::from("allocator canary"));
        }) > 0
    );
    let layout = layout();
    let values = (0..8u64).collect::<Vec<_>>();
    let mut cache = Cache::new();
    assert!(cache.store(&layout, [900., 600.], &values));
    let count = allocations(|| {
        for _ in 0..120 {
            let rows = cache.get(black_box(&layout), [900., 600.]).unwrap();
            assert_eq!(rows, values);
            black_box(rows);
        }
    });
    assert_eq!(count, 120);
}

#[test]
fn changing_layouts_reuses_bounded_buffers_without_key_or_value_allocations() {
    let variants = (0..32)
        .map(|index| {
            let mut node = layout();
            if let Node::Split { split, .. } = &mut node {
                *split = 0.2 + index as f32 * 0.01;
            }
            (node, [900. + index as f32, 600.])
        })
        .collect::<Vec<_>>();
    let mut cache = Cache::new();
    assert!(cache.store(&variants[0].0, variants[0].1, &[1u64, 2]));
    let count = allocations(|| {
        for (node, size) in &variants {
            assert!(cache.store(black_box(node), *size, &[3u64, 4, 5]));
        }
    });
    assert_eq!(count, 0);
    assert_eq!(
        cache.get(&variants[31].0, variants[31].1),
        Some(vec![3, 4, 5])
    );
}

#[test]
fn every_node_field_and_window_bit_changes_the_key() {
    let original = layout();
    let mut cache = Cache::new();
    assert!(cache.store(&original, [900., 600.], &[1u64]));
    for case in 0..11 {
        let mut changed = layout();
        let Node::Split {
            edge,
            split,
            first,
            second,
        } = &mut changed
        else {
            unreachable!()
        };
        match case {
            0 => *edge = Edge::Right,
            1 => *split = f32::from_bits(split.to_bits() + 1),
            2 => {
                if let Node::Dock { id, .. } = first.as_mut() {
                    *id = DockId(3);
                }
            }
            3 => {
                if let Node::Dock { tabs, .. } = first.as_mut() {
                    tabs.swap(0, 1);
                }
            }
            4 => {
                if let Node::Dock { tabs, .. } = first.as_mut() {
                    tabs.push(DockId(3));
                }
            }
            5 => {
                if let Node::Dock { active, .. } = first.as_mut() {
                    *active = 1;
                }
            }
            6 => {
                if let Node::Fixed { edge, .. } = second.as_mut() {
                    *edge = Edge::Top;
                }
            }
            7 => {
                if let Node::Fixed { size, .. } = second.as_mut() {
                    *size = 21.;
                }
            }
            8 => **second = Node::dock(DockId(3)),
            9 => std::mem::swap(first, second),
            10 => {
                if let Node::Dock { tabs, active, .. } = first.as_mut() {
                    tabs.clear();
                    *active = 99;
                }
            }
            _ => unreachable!(),
        }
        assert!(cache.get(&changed, [900., 600.]).is_none(), "case {case}");
    }
    for size in [
        [f32::from_bits(900f32.to_bits() + 1), 600.],
        [900., f32::from_bits(600f32.to_bits() + 1)],
    ] {
        assert!(cache.get(&original, size).is_none());
    }
    let zero = Node::Fixed {
        edge: Edge::Top,
        size: 0.,
    };
    assert!(cache.store(&zero, [0., 600.], &[9]));
    assert!(cache.get(&zero, [-0., 600.]).is_none());
    assert!(cache
        .get(
            &Node::Fixed {
                edge: Edge::Top,
                size: -0.
            },
            [0., 600.]
        )
        .is_none());
}

#[test]
fn rejected_admission_and_returned_mutations_never_poison_the_retained_entry() {
    let node = layout();
    let mut cache = Cache::new();
    assert!(cache.store(&node, [900., 600.], &[1u64, 2]));
    let mut returned = cache.get(&node, [900., 600.]).unwrap();
    returned[0] = 99;
    returned.clear();
    assert_eq!(cache.get(&node, [900., 600.]), Some(vec![1, 2]));
    let too_many_tabs = Node::Dock {
        id: DockId(1),
        tabs: vec![DockId(1); MAX_ITEMS],
        active: 0,
    };
    let mut deep = Node::dock(DockId(1));
    for _ in 0..MAX_ITEMS {
        deep = Node::Split {
            edge: Edge::Top,
            split: 0.5,
            first: Box::new(deep),
            second: Box::new(Node::dock(DockId(2))),
        };
    }
    for rejected in [
        too_many_tabs,
        deep,
        Node::Fixed {
            edge: Edge::Top,
            size: f32::NAN,
        },
    ] {
        assert!(!cache.store(&rejected, [900., 600.], &[3]));
        assert_eq!(cache.get(&node, [900., 600.]), Some(vec![1, 2]));
    }
    assert!(!cache.store(&node, [f32::INFINITY, 600.], &[3]));
    assert!(!cache.store(&node, [900., 600.], &vec![3; MAX_ITEMS + 1]));
    assert_eq!(cache.get(&node, [900., 600.]), Some(vec![1, 2]));
    assert!(cache.store(&Node::dock(DockId(7)), [900., 600.], &[7]));
    assert!(cache.get(&node, [900., 600.]).is_none());
}
