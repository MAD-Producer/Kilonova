//! Count allocations on this test thread, independently of timing or GPU speed.
use motion_render::effect_plan::{
    EffectFramePlan, EffectPass, EffectUniform, LayerComposite, PlannedDraw, image_input_order,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

struct ObservedAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

fn observe() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}

// SAFETY: each method delegates the unchanged pointer, layout and size to System.
// Observation uses only a const-initialized thread-local Cell, without allocating.
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        observe();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        observe();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        observe();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

fn allocations(body: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|count| count.set(None));
        }
    }
    let reset = Reset;
    ALLOCATIONS.with(|count| assert_eq!(count.replace(Some(0)), None));
    body();
    let count = ALLOCATIONS.with(|count| count.get().unwrap());
    drop(reset);
    count
}

fn plan(inputs: &[(i32, bool)]) -> EffectFramePlan {
    EffectFramePlan {
        draws: inputs
            .iter()
            .enumerate()
            .map(|(i, _)| PlannedDraw {
                layer: i as u64 + 100,
                words: [0.; 32],
                pass_start: i,
                pass_end: i + 1,
            })
            .collect(),
        passes: inputs
            .iter()
            .map(|&(resource_input, effected)| {
                let mut uniform = EffectUniform::zeroed();
                uniform.params[30][1] = f32::from(u8::from(effected));
                EffectPass {
                    instance_data: 0,
                    resource_input,
                    program: 0,
                    input: 0,
                    source: 0,
                    output: 0,
                    width: 1,
                    height: 1,
                    lut: -1,
                    uniform,
                    sprite: false,
                    sprite_start: 0,
                    sprite_count: 0,
                }
            })
            .collect(),
        ..Default::default()
    }
}

use bytemuck::Zeroable;

fn matte(index: i32) -> LayerComposite {
    LayerComposite {
        blend: 0,
        space: 0,
        matte: index,
        mode: 0,
        visible: true,
    }
}

#[test]
fn allocation_observer_detects_real_allocations_and_stops_after_the_scope() {
    assert!(
        allocations(|| {
            std::hint::black_box(Vec::<u8>::with_capacity(128));
        }) > 0
    );
    assert_eq!(allocations(|| {}), 0);
    assert_eq!(ALLOCATIONS.with(Cell::get), None);
}

#[test]
fn plans_without_snapshot_dependencies_do_not_allocate_traversal_storage() {
    for input in [(0, true), (-1, true), (1, false)] {
        let mut frame = plan(&vec![input; motion_model::MAX_LAYERS]);
        frame.compositing = vec![matte(-1); frame.draws.len()];
        assert_eq!(
            allocations(|| {
                for _ in 0..120 {
                    assert!(
                        image_input_order(std::hint::black_box(&frame))
                            .unwrap()
                            .is_empty()
                    );
                }
            }),
            0,
            "no snapshot traversal is required for input={input:?}"
        );
    }
    assert_eq!(
        allocations(|| assert!(
            image_input_order(&EffectFramePlan::default())
                .unwrap()
                .is_empty()
        )),
        0
    );
}

#[test]
fn shared_dependencies_and_mattes_remain_in_postorder_once_each() {
    let reverse = plan(&[(2, true), (3, true), (0, false)]);
    assert_eq!(image_input_order(&reverse).unwrap(), [2, 1]);
    let mut frame = plan(&[(0, false), (1, true), (1, true), (2, true)]);
    frame.compositing = vec![matte(3), matte(2), matte(3)];
    assert_eq!(image_input_order(&frame).unwrap(), [0, 1, 3, 2]);
    frame.compositing.clear();
    assert_eq!(image_input_order(&frame).unwrap(), [0, 1]);
}

#[test]
fn dependency_errors_do_not_poison_the_next_traversal() {
    for frame in [plan(&[(2, true), (1, true)]), plan(&[(1, true)])] {
        assert_eq!(
            image_input_order(&frame).unwrap_err().to_string(),
            "GPU validation: effect image input cycle"
        );
    }
    let mut invalid = plan(&[(2, true)]);
    assert_eq!(
        image_input_order(&invalid).unwrap_err().to_string(),
        "GPU validation: effect image input index is invalid"
    );
    invalid.passes[0].resource_input = 0;
    invalid.compositing.push(matte(1));
    assert_eq!(
        image_input_order(&invalid).unwrap_err().to_string(),
        "GPU validation: effect image input index is invalid"
    );
    invalid.compositing.clear();
    assert!(image_input_order(&invalid).unwrap().is_empty());
}
