//! Isolate allocation instrumentation from the application's allocator.
#[path = "../src/panels/effect_controls/definition.rs"]
mod definition;

use serde_json::{json, Value};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    sync::Arc,
};

thread_local! {
    static COUNT: Cell<Option<usize>> = const { Cell::new(None) };
}
struct CountingAllocator;
fn count() {
    let _ = COUNT.try_with(|counter| {
        if let Some(value) = counter.get() {
            counter.set(Some(value + 1));
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
        COUNT.with(|counter| counter.set(None));
    }
}
fn allocations(run: impl FnOnce()) -> usize {
    COUNT.with(|counter| {
        assert!(counter.get().is_none());
        counter.set(Some(0));
    });
    let scope = Scope;
    run();
    let total = COUNT.with(|counter| counter.get().unwrap());
    drop(scope);
    total
}

// Preserve the former production lookup and stable sort as an independent oracle.
fn legacy_find(catalogue: &Value, effect: &Value) -> Option<Value> {
    catalogue["packages"].as_array()?.iter().find(|p| {
        p["hash"] == effect["hash"]
            && p["manifest"]["id"] == effect["plugin"]
            && p["manifest"]["version"] == effect["version"]
    })?["manifest"]["effects"]
        .as_array()?
        .iter()
        .find(|d| d["id"] == effect["effect"])
        .cloned()
}
fn legacy_parameters(definition: &Value) -> Vec<&Value> {
    let mut params: Vec<_> = definition["params"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    params.sort_by_key(|param| param["kind"] == "curve");
    params
}
fn catalogue(params: Vec<Value>) -> Value {
    json!({"packages":[{"hash":"content-hash","manifest":{
        "id":"fixture.plugin","version":"1.0.0","effects":[
            {"id":"fixture.effect","params":params}
        ]
    }}]})
}
fn instance() -> Value {
    json!({"plugin":"fixture.plugin","version":"1.0.0",
        "hash":"content-hash","effect":"fixture.effect"})
}

#[test]
fn definition_lookup_preserves_exact_package_version_hash_and_first_match_rules() {
    let mut source = catalogue(vec![json!({"id":"value","kind":"float"})]);
    let selected = instance();
    for field in ["hash", "plugin", "version", "effect"] {
        for value in [Value::Null, json!("other"), json!(42), json!([])] {
            let mut request = selected.clone();
            request[field] = value;
            assert_eq!(
                definition::find(&source, &request).cloned(),
                legacy_find(&source, &request)
            );
            assert!(definition::find(&source, &request).is_none());
        }
    }
    let original = source["packages"][0].clone();
    let mut second = original.clone();
    second["manifest"]["effects"][0]["name"] = json!("must not replace the first");
    source["packages"].as_array_mut().unwrap().push(second);
    let borrowed = definition::find(&source, &selected).unwrap();
    assert_eq!(Some(borrowed.clone()), legacy_find(&source, &selected));
    assert!(std::ptr::eq(
        borrowed,
        &source["packages"][0]["manifest"]["effects"][0]
    ));
    source["packages"][0]["manifest"]["effects"] = json!([]);
    assert!(definition::find(&source, &selected).is_none());
    assert!(legacy_find(&source, &selected).is_none());
}

#[test]
fn borrowed_parameter_order_matches_the_original_stable_sort_for_mixed_kinds() {
    let kinds = [json!("curve"), json!("float"), Value::Null];
    for pattern in 0..3usize.pow(7) {
        let mut digits = pattern;
        let params: Vec<_> = (0..7)
            .map(|index| {
                let kind = kinds[digits % 3].clone();
                digits /= 3;
                json!({"id":index,"kind":kind})
            })
            .collect();
        let source = json!({"params":params});
        let original = legacy_parameters(&source);
        let borrowed: Vec<_> = definition::parameters(&source).collect();
        assert_eq!(borrowed, original, "pattern={pattern}");
        assert!(borrowed.iter().all(|parameter| source["params"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| std::ptr::eq(*parameter, value))));
    }
    for source in [
        Value::Null,
        json!({}),
        json!({"params":false}),
        json!({"params":[]}),
        json!({"params":[{},null,8,{"kind":true},{"kind":"curve"}]}),
    ] {
        assert_eq!(
            definition::parameters(&source).collect::<Vec<_>>(),
            legacy_parameters(&source)
        );
    }
}

#[test]
fn repeated_definition_and_parameter_visits_do_not_allocate_or_retain_old_generations() {
    for size in [0, 1, 200, 2000] {
        let source = Arc::new(catalogue((0..size).map(|i| {
            json!({"id":format!("parameter-{i}"),"kind":if i % 3 == 0 {"curve"} else {"float"},
                "name":"参数","default":[1,2,3,4]})
        }).collect()));
        let request = instance();
        let measured = allocations(|| {
            for _ in 0..120 {
                let generation = source.clone();
                let selected = definition::find(&generation, &request).unwrap();
                assert_eq!(
                    definition::parameters(selected)
                        .map(|p| black_box(p))
                        .count(),
                    size
                );
            }
        });
        let previous = allocations(|| {
            for _ in 0..120 {
                let selected = legacy_find(&source, &request).unwrap();
                assert_eq!(black_box(legacy_parameters(&selected)).len(), size);
            }
        });
        println!("params={size} visits=120 original_allocations={previous} borrowed_allocations={measured}");
        assert_eq!(measured, 0);
        assert!(
            previous > 0,
            "allocation counter must detect the original owned path"
        );
        assert_eq!(Arc::strong_count(&source), 1);
        let weak = Arc::downgrade(&source);
        drop(source);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn missing_or_replaced_catalogues_keep_original_miss_and_generation_semantics() {
    let request = instance();
    for source in [
        Value::Null,
        json!({}),
        json!({"packages":null}),
        json!({"packages":[]}),
        json!({"packages":[{},false]}),
        catalogue(vec![]),
    ] {
        assert_eq!(
            definition::find(&source, &request).cloned(),
            legacy_find(&source, &request)
        );
    }
    let mut current = Arc::new(catalogue(vec![json!({"id":"old"})]));
    let old_generation = current.clone();
    let old = definition::find(&old_generation, &request).unwrap();
    current = Arc::new(catalogue(vec![json!({"id":"new"})]));
    assert_eq!(old["params"][0]["id"], "old");
    assert_eq!(
        definition::find(&current, &request).unwrap()["params"][0]["id"],
        "new"
    );
    assert_eq!(old["params"][0]["id"], "old");
}
