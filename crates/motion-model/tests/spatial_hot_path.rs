use motion_model::{Ease, SpatialAnimation, SpatialTangents, Track};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static MEASURE:Cell<bool>=const { Cell::new(false) };
    static ALLOCATIONS:Cell<usize>=const { Cell::new(0) };
}
fn allocation() {
    MEASURE.with(|enabled| {
        if enabled.get() {
            ALLOCATIONS.with(|count| count.set(count.get() + 1));
        }
    });
}
struct Counter;
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
struct Measurement;
impl Measurement {
    fn start() -> Self {
        ALLOCATIONS.with(|count| count.set(0));
        MEASURE.with(|enabled| enabled.set(true));
        Self
    }
}
impl Drop for Measurement {
    fn drop(&mut self) {
        MEASURE.with(|enabled| enabled.set(false));
    }
}

#[test]
fn prepared_and_reopened_spatial_queries_allocate_nothing_and_counter_detects_real_allocations() {
    let mut track = Track::constant([0f32; 3]);
    for (frame, value) in [(0, [0.; 3]), (4, [0.; 3]), (10, [10., 0., 0.])] {
        track.upsert(frame, value, Ease::Linear).unwrap();
    }
    track
        .set_spatial(
            0,
            Some(SpatialTangents {
                incoming: None,
                outgoing: Some([20., 30., 5.]),
            }),
        )
        .unwrap();
    track
        .set_spatial(
            4,
            Some(SpatialTangents {
                incoming: Some([-15., 25., -5.]),
                outgoing: None,
            }),
        )
        .unwrap();
    let mut animation = SpatialAnimation::from_track(&track).unwrap();
    animation.set_roving(&[2], true).unwrap();
    let reopened: SpatialAnimation<[f32; 3]> =
        serde_json::from_slice(&serde_json::to_vec(&animation).unwrap()).unwrap();
    for source in [&animation, &reopened] {
        let measurement = Measurement::start();
        for tick in 0..10_000 {
            let sample = source.sample(f64::from(tick) / 999.9).unwrap();
            std::hint::black_box(sample);
        }
        drop(measurement);
        assert_eq!(ALLOCATIONS.with(Cell::get), 0, "prepared query allocated");
    }
    let measurement = Measurement::start();
    std::hint::black_box(vec![0u8; 4096]);
    drop(measurement);
    assert!(
        ALLOCATIONS.with(Cell::get) > 0,
        "counter must detect an actual allocation"
    );
}

#[test]
fn authoritative_and_reopened_track_sampling_does_not_allocate_on_the_present_path() {
    let mut track = Track::constant([0.; 3]);
    for (frame, x) in [(0, 0.), (4, 1.), (10, 7.), (20, 20.)] {
        track.upsert(frame, [x, 0., 0.], Ease::Linear).unwrap();
    }
    track.enable_roving(&[4]).unwrap();
    let reopened: Track<[f32; 3]> =
        serde_json::from_slice(&serde_json::to_vec(&track).unwrap()).unwrap();
    for source in [&track, &reopened] {
        let measurement = Measurement::start();
        for tick in 0..10_000 {
            std::hint::black_box(source.sample(f64::from(tick) / 499.95).unwrap());
        }
        drop(measurement);
        assert_eq!(
            ALLOCATIONS.with(Cell::get),
            0,
            "authoritative Track sampling allocated"
        );
    }
}
