#[path = "../src/panels/timeline/scroll_signature.rs"]
mod scroll_signature;

use motion_core::{Axis, ExpressionTarget, Property};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hash::Hasher;
use std::hint::black_box;

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

fn signature(targets: &[ExpressionTarget]) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    scroll_signature::expression_rows(targets, &mut hash);
    hash.finish()
}

#[test]
fn expression_scroll_keys_have_no_format_allocations_for_empty_property_or_effect_rows() {
    assert!(
        count_allocations(|| {
            black_box("allocation canary".to_owned());
        }) > 0
    );
    let fixtures = [
        vec![],
        vec![ExpressionTarget::Property {
            object: 7,
            property: Property::Position,
            axis: Some(Axis::Y),
        }],
        vec![ExpressionTarget::Effect {
            object: 7,
            effect: 3,
            param: "amount\0中文".into(),
        }],
    ];
    let counts: Vec<_> = fixtures
        .iter()
        .map(|targets| {
            let expected = signature(targets);
            count_allocations(|| {
                for _ in 0..120 {
                    assert_eq!(black_box(signature(black_box(targets))), expected);
                }
            })
        })
        .collect();
    println!("120 signatures [empty, property, effect]: {counts:?} allocations");
    assert_eq!(counts, [0, 0, 0]);
}

#[test]
fn every_expression_target_component_and_row_order_affect_identity() {
    let fixtures = vec![
        vec![],
        vec![ExpressionTarget::Property {
            object: 7,
            property: Property::Position,
            axis: None,
        }],
        vec![ExpressionTarget::Property {
            object: 8,
            property: Property::Position,
            axis: None,
        }],
        vec![ExpressionTarget::Property {
            object: 7,
            property: Property::Scale,
            axis: None,
        }],
        vec![ExpressionTarget::Property {
            object: 7,
            property: Property::Position,
            axis: Some(Axis::X),
        }],
        vec![ExpressionTarget::Property {
            object: 7,
            property: Property::Position,
            axis: Some(Axis::Y),
        }],
        vec![ExpressionTarget::Property {
            object: 7,
            property: Property::Position,
            axis: Some(Axis::Z),
        }],
        vec![ExpressionTarget::Effect {
            object: 7,
            effect: 3,
            param: "amount".into(),
        }],
        vec![ExpressionTarget::Effect {
            object: 8,
            effect: 3,
            param: "amount".into(),
        }],
        vec![ExpressionTarget::Effect {
            object: 7,
            effect: 4,
            param: "amount".into(),
        }],
        vec![ExpressionTarget::Effect {
            object: 7,
            effect: 3,
            param: "amount\0中文".into(),
        }],
        vec![ExpressionTarget::Effect {
            object: 7,
            effect: 3,
            param: "".into(),
        }],
    ];
    let mut hashes: std::collections::HashSet<_> = fixtures
        .iter()
        .map(|targets| {
            let hash = signature(targets);
            assert_eq!(hash, signature(&targets.clone()));
            hash
        })
        .collect();
    assert_eq!(hashes.len(), fixtures.len());
    let a = vec![fixtures[1][0].clone(), fixtures[7][0].clone()];
    let b = vec![a[1].clone(), a[0].clone()];
    assert!(hashes.insert(signature(&a)));
    assert!(hashes.insert(signature(&b)));
}
