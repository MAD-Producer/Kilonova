use super::*;

fn value() -> FittedLine {
    FittedLine {
        layout: Arc::new(LineLayout {
            glyphs: Vec::new(),
            advance: 0.,
            grapheme_boundaries: Vec::new(),
            grapheme_spans: Vec::new(),
            missing_glyphs: 0,
        }),
        prefix_end: 0,
        ellipsis: false,
    }
}

fn assert_accounted(cache: &FitCache) {
    assert_eq!(
        cache.bytes,
        cache.entries.values().map(|e| e.bytes).sum::<usize>()
    );
    for entry in cache.entries.values() {
        assert_eq!(
            entry.bytes,
            ENTRY_BYTES + entry.text.len() + entry.value.layout.retained_bytes()
        );
    }
    assert!(cache.bytes <= MAX_BYTES);
    assert!(cache.entries.len() <= MAX_ENTRIES);
}

#[test]
fn fitted_metadata_is_charged_and_replacements_and_clear_keep_exact_totals() {
    let mut cache = FitCache {
        entries: HashMap::with_capacity(MAX_ENTRIES),
        ..Default::default()
    };
    // Compare the same empty-table state across clear; tombstones can reduce
    // the publicly reported capacity without changing the allocation.
    let capacity = cache.entries.capacity();
    let fit = value();
    cache.insert("Position", 2., fit.clone());
    let bytes = cache.bytes;
    for _ in 0..100 {
        cache.insert("Position", 2., fit.clone());
        assert_eq!(cache.bytes, bytes);
        assert_accounted(&cache);
    }
    for i in 0..MAX_ENTRIES + 20 {
        cache.insert(&format!("key-{i}"), 2., fit.clone());
        assert_accounted(&cache);
    }
    assert!(cache.entries.len() > 128);
    assert!(cache.get("Position", 2.).is_none());
    assert!(cache.entries.capacity() <= capacity);
    cache.clear();
    assert_eq!((cache.bytes, cache.clock, cache.entries.len()), (0, 0, 0));
    assert_eq!(cache.entries.capacity(), capacity);
}

#[test]
fn oversized_fitted_values_do_not_evict_healthy_values_or_reset_their_identity() {
    let mut cache = FitCache::default();
    let fit = value();
    cache.insert("Healthy", 2., fit.clone());
    let before = cache.bytes;
    let mut oversized = value();
    Arc::get_mut(&mut oversized.layout).unwrap().grapheme_spans =
        Vec::with_capacity(MAX_BYTES / size_of::<[f32; 2]>() + 1);
    cache.insert("large", 2., oversized);
    cache.insert(&"x".repeat(MAX_BYTES), 2., value());
    assert_eq!(cache.bytes, before);
    assert_eq!(cache.entries.len(), 1);
    assert!(Arc::ptr_eq(
        &cache.get("Healthy", 2.).unwrap().layout,
        &fit.layout
    ));
    assert_accounted(&cache);
}

#[test]
fn collision_checks_and_clock_rollover_preserve_the_fitting_contract() {
    let mut cache = FitCache::default();
    cache.insert("Position", 2., value());
    let key = cache.key("Position", 2.);
    // Model a collision without relying on discovering a RandomState collision.
    // get must still verify both original text and width after the key matches.
    let entry = cache.entries.get_mut(&key).unwrap();
    entry.text = Arc::from("Rotation");
    assert!(cache.get("Position", 2.).is_none());
    cache.entries.get_mut(&key).unwrap().text = Arc::from("Position");
    cache.entries.get_mut(&key).unwrap().width = 3f32.to_bits();
    assert!(cache.get("Position", 2.).is_none());
    cache.entries.get_mut(&key).unwrap().width = 2f32.to_bits();
    cache.clock = u64::MAX;
    assert!(cache.get("Position", 2.).is_some());
    assert_eq!(cache.clock, 1);
    assert_accounted(&cache);
}
