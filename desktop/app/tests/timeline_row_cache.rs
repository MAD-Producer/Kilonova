#[path = "../src/panels/timeline/row_cache.rs"]
mod row_cache;

use serde_json::json;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::Arc;

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
struct CountAllocations;
unsafe impl GlobalAlloc for CountAllocations {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
            }
        });
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
            }
        });
        System.realloc(ptr, layout, size)
    }
}
#[global_allocator]
static ALLOCATOR: CountAllocations = CountAllocations;

fn count_allocations(work: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(Some(0)));
    work();
    ALLOCATIONS.with(|count| count.replace(None).unwrap())
}

#[test]
fn warmed_row_lookup_reuses_rows_without_allocating_keys() {
    assert!(
        count_allocations(|| {
            black_box("allocation canary".to_owned());
        }) > 0
    );
    let state = json!({"root":"project-a", "composition":"composition-a", "revision":7});
    let mut cache = None;
    let warm = row_cache::get(&mut cache, &state, || (0..200).collect::<Vec<u64>>());
    let allocations = count_allocations(|| {
        for _ in 0..120 {
            let rows = row_cache::get(&mut cache, black_box(&state), || {
                panic!("warm cache rebuilt rows")
            });
            assert!(Arc::ptr_eq(&warm, &rows));
            assert_eq!(black_box(rows.iter().sum::<u64>()), 19900);
        }
    });
    println!("200 rows x 120 lookups: {allocations} cache-hit allocations");
    assert_eq!(allocations, 0);
}

#[test]
fn each_identity_component_invalidates_and_old_rows_are_released() {
    let mut cache = None;
    let mut state = json!({"root":"project-a", "composition":"composition-a", "revision":7});
    let first = row_cache::get(&mut cache, &state, || vec![1, 2]);
    let mut previous = Arc::downgrade(&first);
    drop(first);
    for (field, value, expected) in [
        ("root", json!("project-b"), vec![3, 4]),
        ("composition", json!("composition-b"), vec![5, 6]),
        ("revision", json!(8), vec![7, 8]),
    ] {
        state[field] = value;
        let rows = row_cache::get(&mut cache, &state, || expected.clone());
        assert_eq!(*rows, expected);
        assert!(
            previous.upgrade().is_none(),
            "old rows retained after {field} change"
        );
        let same = row_cache::get(&mut cache, &state, || {
            panic!("unchanged identity rebuilt rows")
        });
        assert!(Arc::ptr_eq(&rows, &same));
        previous = Arc::downgrade(&rows);
    }
    assert!(
        previous.upgrade().is_some(),
        "current rows lost cache ownership"
    );
    drop(cache);
    assert!(previous.upgrade().is_none());
}

#[test]
fn non_identity_ui_changes_and_missing_or_malformed_identity_keep_existing_semantics() {
    let mut cache = None;
    let first = row_cache::get(&mut cache, &json!({}), Vec::<u64>::new);
    assert!(first.is_empty());
    for state in [
        json!({"root":"", "composition":"", "revision":0}),
        json!({"root":null, "composition":42, "revision":-1}),
        json!({"root":false, "composition":[], "revision":"invalid", "frame":22.5}),
    ] {
        let rows = row_cache::get(&mut cache, &state, || {
            panic!("default identity rebuilt empty rows")
        });
        assert!(Arc::ptr_eq(&first, &rows));
    }
    let mut state = json!({"root":"project", "composition":"main", "revision":1, "frame":0});
    let rows = row_cache::get(&mut cache, &state, || vec![2, 1]);
    state["frame"] = json!(20.5);
    state["timeline_query"] = json!("title");
    let same = row_cache::get(&mut cache, &state, || panic!("UI-only change rebuilt rows"));
    assert!(Arc::ptr_eq(&rows, &same));
    assert_eq!(*same, [2, 1]);
}
