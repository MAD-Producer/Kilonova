use motion_ui::text::TextAtlas;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};

struct Counter;
thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
fn record() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
struct Scope;
impl Drop for Scope {
    fn drop(&mut self) {
        ALLOCATIONS.with(|c| c.set(None));
    }
}
fn allocations(run: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|c| {
        assert!(c.get().is_none());
        c.set(Some(0));
    });
    let scope = Scope;
    run();
    let count = ALLOCATIONS.with(|c| c.get().unwrap());
    drop(scope);
    count
}

#[test]
fn cached_layouts_for_one_or_two_hundred_controls_allocate_nothing() {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    for label in [
        "Position",
        "cafe\u{301} / 中文",
        "ABC سلام 123",
        "A🇯🇵👩\u{200d}💻",
    ] {
        let initial = atlas.layout_line(label).unwrap();
        assert!(initial.advance > 0.);
        for count in [1, 200] {
            let allocated = allocations(|| {
                for _ in 0..count {
                    let actual = atlas.layout_line(label).unwrap();
                    assert!(Arc::ptr_eq(&initial, &actual));
                    std::hint::black_box(actual);
                }
            });
            assert_eq!(
                allocated, 0,
                "retained single-line layout must not allocate per control"
            );
        }
    }
}

#[test]
fn cached_fitted_layouts_keep_complete_graphemes_without_per_control_allocations() {
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    for label in [
        "Position / 中文 long label",
        "cafe\u{301} long label",
        "ABC سلام 123",
        "A🇯🇵👩\u{200d}💻 long label",
    ] {
        for width in [2., 6., 100.] {
            let initial = atlas.fit_line(label, width).unwrap();
            assert!(initial.layout.advance <= width);
            for count in [1, 200] {
                assert_eq!(
                    allocations(|| {
                        for _ in 0..count {
                            let actual = atlas.fit_line(label, width).unwrap();
                            assert!(Arc::ptr_eq(&initial.layout, &actual.layout));
                            std::hint::black_box(actual);
                        }
                    }),
                    0,
                    "fitted layout cache hits may not allocate per control"
                );
            }
        }
    }
}

#[test]
fn distinct_label_churn_keeps_retained_layouts_immutable_and_reports_allocations() {
    // Exceed the bounded caches with real, different labels. Repeating one
    // cached label 200 times does not exercise the layout miss path.
    let labels: Vec<_> = (0..200)
        .map(|n| format!("Property {n:03} cafe\u{301} / 位置 / سلام long label"))
        .collect();
    for chinese in [false, true] {
        let mut atlas = TextAtlas::from_system_language(12., chinese).unwrap();
        for width in [7., 100.] {
            let retained = atlas.fit_line(&labels[0], width).unwrap();
            let snapshot = retained
                .layout
                .glyphs
                .iter()
                .map(|g| (g.id, g.face, g.cluster, g.position, g.advance))
                .collect::<Vec<_>>();
            for count in [32, 200] {
                for _ in 0..2 {
                    for label in &labels[..count] {
                        std::hint::black_box(atlas.fit_line(label, width).unwrap());
                    }
                }
                let allocated = allocations(|| {
                    for label in &labels[..count] {
                        std::hint::black_box(atlas.fit_line(label, width).unwrap());
                    }
                });
                println!("distinct_labels={count} chinese={chinese} width_em={width} allocations={allocated}");
            }
            assert_eq!(
                retained
                    .layout
                    .glyphs
                    .iter()
                    .map(|g| (g.id, g.face, g.cluster, g.position, g.advance))
                    .collect::<Vec<_>>(),
                snapshot,
                "eviction and scratch reuse must not overwrite an outstanding layout"
            );
            let rebuilt = atlas.fit_line(&labels[0], width).unwrap();
            assert_eq!(rebuilt.prefix_end, retained.prefix_end);
            assert_eq!(rebuilt.ellipsis, retained.ellipsis);
            assert_eq!(rebuilt.layout.advance, retained.layout.advance);
            assert_eq!(
                rebuilt.layout.grapheme_boundaries,
                retained.layout.grapheme_boundaries
            );
            assert_eq!(
                rebuilt
                    .layout
                    .glyphs
                    .iter()
                    .map(|g| (g.id, g.face, g.cluster, g.position, g.advance))
                    .collect::<Vec<_>>(),
                snapshot
            );
        }
    }
}

#[test]
fn retained_shaping_plans_reduce_allocations_against_fresh_engine_plans() {
    use harfrust::{Buffer, ClusterLevel, Direction, Font, Script, ShapeOptions, ShaperFont};
    use motion_ui::text::shaping::RunShaper;
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    let id = database
        .query(&fontdb::Query {
            families: &[fontdb::Family::SansSerif],
            ..Default::default()
        })
        .unwrap();
    let (bytes, index): (Arc<[u8]>, _) = database
        .with_face_data(id, |bytes, index| (Arc::from(bytes), index))
        .unwrap();
    let source: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(bytes.clone());
    let font = Font::new(source, index).unwrap();
    let mut shaper = RunShaper::new(bytes, index).unwrap();
    let mut buffer = Buffer::new();
    let labels: Vec<_> = (0..200)
        .map(|n| format!("Property {n:03} office AV"))
        .collect();
    let mut retained = || {
        for text in &labels {
            std::hint::black_box(
                shaper
                    .shape_script(text, Direction::LeftToRight, Some(Script::LATIN))
                    .unwrap(),
            );
        }
    };
    let mut fresh = || {
        for text in &labels {
            buffer.clear();
            buffer.push_str(text);
            buffer.set_cluster_level(ClusterLevel::MonotoneGraphemes);
            buffer.set_direction(Direction::LeftToRight);
            buffer.set_script(Some(Script::LATIN));
            buffer.guess_segment_properties();
            harfrust::shape(
                &ShaperFont::new(&font),
                &mut buffer,
                ShapeOptions::default(),
            )
            .unwrap();
            std::hint::black_box(buffer.glyph_infos());
        }
    };
    retained();
    fresh();
    let retained_count = allocations(retained);
    let fresh_count = allocations(fresh);
    println!("shaping_runs=200 retained_plan_allocations={retained_count} fresh_plan_allocations={fresh_count}");
    assert!(
        retained_count < fresh_count,
        "reusing plans must reduce actual allocations on different runs"
    );
}
