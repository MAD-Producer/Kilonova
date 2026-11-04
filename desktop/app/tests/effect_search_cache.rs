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

use cache::{Fields, GroupedCache, GroupedMatches, Selection, Sort};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    key: String,
    name: String,
    category: String,
}

fn project_rows(catalogue: &serde_json::Value, zh: bool) -> Vec<(Row, String)> {
    (0..catalogue["count"].as_u64().unwrap_or(0))
        .map(|i| {
            let row = Row {
                key: format!("plugin/effect{i}"),
                name: if zh {
                    format!("效果{}", i % 7)
                } else {
                    format!("Effect{}", i % 7)
                },
                category: format!("Category{}", i % 3),
            };
            let terms = format!("{} {} {}", row.key, row.name, row.category).to_lowercase();
            (row, terms)
        })
        .collect()
}

fn fields() -> Fields<Row> {
    Fields {
        key: |row| &row.key,
        name: |row| &row.name,
        category: |row| &row.category,
    }
}

fn grouped_snapshot(view: GroupedMatches<'_, Row>) -> Vec<(String, Vec<Row>)> {
    view.groups()
        .map(|group| {
            assert_eq!(group.len().to_string(), group.count_text());
            (group.name().to_owned(), group.iter().cloned().collect())
        })
        .collect()
}

#[test]
fn unchanged_grouping_reuses_projection_rows_and_count_labels_without_allocating() {
    let favorites = BTreeSet::from(["plugin/effect1".into()]);
    let recent = vec!["plugin/effect2".into(), "plugin/effect1".into()];
    for size in [1, 200, 2000] {
        let catalogue = Arc::new(json!({"count": size}));
        let mut cache = GroupedCache::default();
        let selection = || Selection {
            tab: 0,
            favorites: &favorites,
            recent: &recent,
            sort: Sort::Name,
        };
        assert_eq!(
            cache
                .grouped(
                    &catalogue,
                    false,
                    "effect",
                    selection(),
                    || project_rows(&catalogue, false),
                    fields()
                )
                .shown,
            size
        );
        assert_eq!(cache.builds(), (1, 1));
        let allocations = count_allocations(|| {
            for _ in 0..120 {
                let view = cache.grouped(
                    &catalogue,
                    false,
                    "effect",
                    selection(),
                    || panic!("warm group rebuilt projection"),
                    fields(),
                );
                assert_eq!(view.total(), size);
                assert_eq!(view.all().count(), size);
                assert_eq!(view.shown, size);
                assert!(view.searching);
                assert_eq!(
                    view.groups()
                        .map(|group| {
                            black_box(group.name());
                            black_box(group.count_text());
                            assert_eq!(group.len(), group.iter().count());
                            group.iter().map(|row| black_box(row.key.len())).count()
                        })
                        .sum::<usize>(),
                    size
                );
            }
        });
        assert_eq!(cache.builds(), (1, 1));
        println!("{size} entries x 120 grouped cache hits: {allocations} allocations");
        assert_eq!(allocations, 0);
        let cold = count_allocations(|| {
            let mut cold = GroupedCache::default();
            black_box(
                cold.grouped(
                    &catalogue,
                    false,
                    "effect",
                    selection(),
                    || project_rows(&catalogue, false),
                    fields(),
                )
                .shown,
            );
        });
        assert!(cold > 0, "cold control must allocate");
    }
}

#[test]
fn grouped_cache_invalidates_each_selection_dimension_and_releases_replaced_snapshots() {
    let mut catalogue = Arc::new(json!({"count": 6}));
    let mut favorites = BTreeSet::new();
    let mut recent: Vec<String> = Vec::new();
    let mut cache = GroupedCache::default();
    for (query, tab, sort, add_favorite, recent_values, expected_builds, expected_shown) in [
        ("", 0, Sort::Name, false, vec![], (1, 1), 6),
        ("", 0, Sort::Name, false, vec![], (1, 1), 6),
        ("EFFECT1", 0, Sort::Name, false, vec![], (1, 2), 1),
        ("missing", 0, Sort::Name, false, vec![], (1, 3), 0),
        ("", 1, Sort::Name, false, vec![], (1, 4), 0),
        ("", 1, Sort::Name, true, vec![], (1, 5), 1),
        (
            "",
            2,
            Sort::Recent,
            false,
            vec!["plugin/effect2", "plugin/effect1"],
            (1, 6),
            2,
        ),
        (
            "",
            2,
            Sort::Recent,
            false,
            vec!["plugin/effect1", "plugin/effect2"],
            (1, 7),
            2,
        ),
        (
            "",
            2,
            Sort::FavoritesFirst,
            false,
            vec!["plugin/effect1", "plugin/effect2"],
            (1, 8),
            2,
        ),
    ] {
        if add_favorite {
            favorites.insert("plugin/effect1".into());
        }
        recent.clear();
        recent.extend(recent_values.into_iter().map(str::to_owned));
        let view = cache.grouped(
            &catalogue,
            false,
            query,
            Selection {
                tab,
                favorites: &favorites,
                recent: &recent,
                sort,
            },
            || project_rows(&catalogue, false),
            fields(),
        );
        assert_eq!(view.shown, expected_shown);
        assert_eq!(cache.builds(), expected_builds);
    }
    let selection = || Selection {
        tab: 0,
        favorites: &favorites,
        recent: &recent,
        sort: Sort::Name,
    };
    let view = cache.grouped(
        &catalogue,
        true,
        "效果",
        selection(),
        || project_rows(&catalogue, true),
        fields(),
    );
    assert_eq!(view.shown, 6);
    assert_eq!(cache.builds(), (2, 9));
    let old = Arc::downgrade(&catalogue);
    Arc::make_mut(&mut catalogue)["count"] = 0.into();
    assert!(old.upgrade().is_some());
    let view = cache.grouped(
        &catalogue,
        true,
        "效果",
        selection(),
        || project_rows(&catalogue, true),
        fields(),
    );
    assert_eq!(view.total(), 0);
    assert_eq!(view.shown, 0);
    assert_eq!(view.groups().count(), 0);
    assert!(old.upgrade().is_none());
    assert_eq!(cache.builds(), (3, 10));
    let replacement = Arc::new(json!({"count": 9}));
    assert_eq!(
        cache
            .grouped(
                &replacement,
                true,
                "效果",
                selection(),
                || project_rows(&replacement, true),
                fields()
            )
            .shown,
        9
    );
    assert_eq!(cache.builds(), (4, 11));
}

#[test]
fn cached_group_order_and_metadata_match_uncached_sorting_for_all_tabs_and_locales() {
    let catalogue = Arc::new(json!({"count": 35}));
    let favorites = BTreeSet::from([
        "plugin/effect1".into(),
        "plugin/effect7".into(),
        "plugin/effect22".into(),
    ]);
    let recent = vec![
        "plugin/effect22".into(),
        "plugin/effect1".into(),
        "plugin/effect7".into(),
    ];
    let mut cache = GroupedCache::default();
    for zh in [false, true] {
        for query in [
            "",
            "  EFFECT  ",
            "效果",
            "category1 plugin",
            "effect1",
            "absent",
        ] {
            for tab in 0..3 {
                for sort in [Sort::Name, Sort::Recent, Sort::FavoritesFirst] {
                    let mut expected: BTreeMap<String, Vec<Row>> = BTreeMap::new();
                    for (row, terms) in project_rows(&catalogue, zh) {
                        if query
                            .trim()
                            .to_lowercase()
                            .split_whitespace()
                            .all(|word| terms.contains(word))
                            && (tab != 1 || favorites.contains(&row.key))
                            && (tab != 2 || recent.contains(&row.key))
                        {
                            expected.entry(row.category.clone()).or_default().push(row);
                        }
                    }
                    for rows in expected.values_mut() {
                        match sort {
                            Sort::Name => {
                                rows.sort_by(|a, b| a.name.cmp(&b.name).then(a.key.cmp(&b.key)))
                            }
                            Sort::Recent => rows
                                .sort_by_key(|row| recent.iter().position(|key| key == &row.key)),
                            Sort::FavoritesFirst => rows.sort_by_key(|row| {
                                (!favorites.contains(&row.key), row.name.clone())
                            }),
                        }
                    }
                    let actual = grouped_snapshot(cache.grouped(
                        &catalogue,
                        zh,
                        query,
                        Selection {
                            tab,
                            favorites: &favorites,
                            recent: &recent,
                            sort,
                        },
                        || project_rows(&catalogue, zh),
                        fields(),
                    ));
                    assert_eq!(
                        actual,
                        expected.into_iter().collect::<Vec<_>>(),
                        "locale={zh} query={query:?} tab={tab}"
                    );
                }
            }
        }
    }
}
