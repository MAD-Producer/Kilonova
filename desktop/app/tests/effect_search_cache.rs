#[path = "../src/panels/effect_search/cache.rs"]
mod cache;

use cache::Cache;
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
fn unchanged_catalogue_locale_and_query_have_no_cache_hit_allocations() {
    assert!(
        count_allocations(|| {
            black_box("allocation canary".to_owned());
        }) > 0
    );
    let catalogue = Arc::new(json!({"packages":[]}));
    let mut cache = Cache::default();
    let warm = cache.matching(&catalogue, false, "effect", || {
        (0..200).map(|i| (i, format!("effect {i}"))).collect()
    });
    assert_eq!(warm.len(), 200);
    assert!(!warm.is_empty());
    let count = count_allocations(|| {
        for _ in 0..120 {
            let matches = cache.matching(&catalogue, false, black_box("effect"), || {
                panic!("warm cache rebuilt catalogue")
            });
            assert_eq!(matches.len(), 200);
            assert_eq!(black_box(matches.iter().copied().sum::<usize>()), 19900);
        }
    });
    println!("200 entries x 120 unchanged queries: {count} cache-hit allocations");
    assert_eq!(count, 0);
}

#[test]
fn query_changes_reuse_projection_and_catalogue_or_locale_changes_replace_it() {
    let mut catalogue = Arc::new(json!({"revision":1}));
    let mut cache = Cache::default();
    let selected = cache.matching(&catalogue, false, "  HELLO  ", || {
        vec![(1, "hello world".into()), (2, "bye".into())]
    });
    assert_eq!(selected.iter().copied().collect::<Vec<_>>(), [1]);
    for (query, expected) in [
        ("bye", vec![2]),
        ("hello world", vec![1]),
        ("missing", vec![]),
        ("", vec![1, 2]),
    ] {
        let selected = cache.matching(&Arc::clone(&catalogue), false, query, || {
            panic!("query rebuilt catalogue")
        });
        assert_eq!(selected.iter().copied().collect::<Vec<_>>(), expected);
    }
    let old = Arc::downgrade(&catalogue);
    // Cache retention makes safe make_mut publish a new immutable identity.
    Arc::make_mut(&mut catalogue)["revision"] = 2.into();
    assert!(old.upgrade().is_some());
    let selected = cache.matching(&catalogue, false, "", || vec![(3, "new".into())]);
    assert_eq!(selected.iter().copied().collect::<Vec<_>>(), [3]);
    assert!(
        old.upgrade().is_none(),
        "old snapshot was retained after replacement"
    );
    let selected = cache.matching(&catalogue, true, "", || vec![(4, "locale".into())]);
    assert_eq!(selected.iter().copied().collect::<Vec<_>>(), [4]);
    let selected = cache.matching(&catalogue, true, "nothing", || {
        panic!("unchanged locale rebuilt catalogue")
    });
    assert!(selected.is_empty());
}
