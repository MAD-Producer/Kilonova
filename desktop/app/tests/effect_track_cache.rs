//! Allocation probe uses the production cache, isolated from the desktop test
//! executable so its allocator never changes the application's allocator.
#[path = "../src/panels/effect_controls/track_cache.rs"]
mod track_cache;
use motion_core::{EffectParam, Track};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use track_cache::TrackCache;

struct CountingAllocator;
thread_local! { static COUNT: Cell<Option<usize>> = const { Cell::new(None) }; }
fn count() {
    let _ = COUNT.try_with(|c| {
        if let Some(n) = c.get() {
            c.set(Some(n + 1));
        }
    });
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
struct Scope;
impl Drop for Scope {
    fn drop(&mut self) {
        COUNT.with(|c| c.set(None));
    }
}
fn allocations(run: impl FnOnce()) -> usize {
    COUNT.with(|c| {
        assert!(c.get().is_none());
        c.set(Some(0));
    });
    let scope = Scope;
    run();
    let count = COUNT.with(|c| c.get().unwrap());
    drop(scope);
    count
}
fn parameter(value: f32) -> EffectParam {
    EffectParam {
        kind: motion_effects::ParamKind::Float,
        animatable: true,
        min: -1000.,
        max: 1000.,
        implemented: true,
        default: [0.; 4],
        track: Track::constant([value, 0., 0., 0.]),
        curve: None,
    }
}

#[test]
fn warm_cache_borrows_parameter_names_without_per_frame_allocations() {
    let mut cache = TrackCache::default();
    let keys: Vec<_> = (0..200)
        .map(|i| (7 + i / 50, 3 + i % 5, format!("parameter-{i:04}")))
        .collect();
    for (i, (object, effect, name)) in keys.iter().enumerate() {
        cache
            .get_or_try_insert(*object, *effect, name, || Ok::<_, ()>(parameter(i as f32)))
            .unwrap();
    }
    let measured = allocations(|| {
        for _ in 0..120 {
            for (object, effect, name) in &keys {
                let track = cache
                    .get_or_try_insert(*object, *effect, name, || -> Result<EffectParam, ()> {
                        panic!("warm cache called its loader")
                    })
                    .unwrap();
                std::hint::black_box(&track.track.value);
            }
        }
    });
    println!("200 parameters x 120 frames: cache-hit allocations={measured}");
    assert_eq!(measured, 0);
}

#[test]
fn instance_identity_clear_and_failed_loads_preserve_original_values_and_causes() {
    let mut cache = TrackCache::default();
    for (object, effect, value) in [(7, 3, 10.), (7, 4, 20.), (8, 3, 30.)] {
        assert_eq!(
            cache
                .get_or_try_insert(object, effect, "gain", || Ok::<_, ()>(parameter(value)))
                .unwrap()
                .track
                .value[0],
            value
        );
    }
    for (object, effect, value) in [(7, 3, 10.), (7, 4, 20.), (8, 3, 30.)] {
        assert_eq!(
            cache
                .get_or_try_insert(object, effect, "gain", || Ok::<_, ()>(parameter(999.)))
                .unwrap()
                .track
                .value[0],
            value
        );
    }
    let cause = "effect parameter decode: missing track";
    assert_eq!(
        cache
            .get_or_try_insert(7, 3, "bad", || Err(cause))
            .unwrap_err(),
        cause
    );
    assert_eq!(
        cache
            .get_or_try_insert(7, 3, "bad", || Ok::<_, ()>(parameter(40.)))
            .unwrap()
            .track
            .value[0],
        40.
    );
    cache.clear();
    assert_eq!(
        cache
            .get_or_try_insert(7, 3, "gain", || Ok::<_, ()>(parameter(50.)))
            .unwrap()
            .track
            .value[0],
        50.
    );
}
