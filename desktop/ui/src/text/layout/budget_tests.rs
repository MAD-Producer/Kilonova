use super::*;

fn empty() -> Arc<LineLayout> {
    Arc::new(LineLayout {
        glyphs: Vec::new(),
        advance: 0.,
        grapheme_boundaries: Vec::new(),
        grapheme_spans: Vec::new(),
        missing_glyphs: 0,
    })
}

fn assert_accounted(cache: &LayoutCache) {
    assert_eq!(
        cache.bytes,
        cache.entries.values().map(|e| e.bytes).sum::<usize>()
    );
    for (text, entry) in &cache.entries {
        assert_eq!(
            entry.bytes,
            ENTRY_BYTES + text.len() + entry.value.retained_bytes()
        );
    }
    assert!(cache.bytes <= MAX_CACHE_BYTES);
    assert!(cache.entries.len() <= MAX_CACHE_ENTRIES);
}

#[test]
fn empty_values_charge_keys_headers_and_metadata_and_remain_bounded() {
    let mut cache = LayoutCache {
        entries: HashMap::with_capacity(MAX_CACHE_ENTRIES),
        ..Default::default()
    };
    // HashMap::capacity excludes deletion tombstones. Capture the original
    // empty table, so clear is compared with the same tombstone-free state.
    let capacity = cache.entries.capacity();
    let value = empty();
    assert_eq!(value.retained_bytes(), LineLayout::FIXED_RETAINED_BYTES);
    for i in 0..MAX_CACHE_ENTRIES + 20 {
        cache.insert(&format!("key-{i}"), Arc::clone(&value));
        assert_accounted(&cache);
    }
    assert!(cache.entries.len() > 128);
    assert!(cache.get("key-0").is_none());
    assert!(cache.entries.capacity() <= capacity);
    cache.clear();
    assert_eq!((cache.bytes, cache.clock, cache.entries.len()), (0, 0, 0));
    assert_eq!(cache.entries.capacity(), capacity);
}

#[test]
fn replacing_the_same_key_does_not_accumulate_phantom_bytes() {
    let mut cache = LayoutCache::default();
    let value = empty();
    cache.insert("Position", Arc::clone(&value));
    let expected = cache.bytes;
    for _ in 0..100 {
        cache.insert("Position", Arc::clone(&value));
        assert_eq!(cache.bytes, expected);
        assert_eq!(cache.entries.len(), 1);
        assert!(Arc::ptr_eq(&cache.get("Position").unwrap(), &value));
        assert_accounted(&cache);
    }
}

#[test]
fn data_budget_evicts_the_least_recent_value_and_preserves_outstanding_owners() {
    let mut cache = LayoutCache::default();
    let value = Arc::new(LineLayout {
        glyphs: Vec::with_capacity(MAX_CACHE_BYTES / (3 * size_of::<PositionedGlyph>())),
        ..Arc::try_unwrap(empty()).unwrap()
    });
    let retained_bytes = value.retained_bytes();
    for key in ["cold", "hot"] {
        cache.insert(key, Arc::clone(&value));
    }
    assert_eq!(cache.entries.len(), 2);
    assert!(cache.get("cold").is_some());
    cache.insert("next", Arc::clone(&value));
    assert!(cache.get("hot").is_none());
    assert!(cache.get("cold").is_some());
    assert!(cache.get("next").is_some());
    assert_eq!(value.retained_bytes(), retained_bytes);
    assert_accounted(&cache);
}

#[test]
fn oversized_data_or_keys_are_normal_misses_and_leave_the_working_set_intact() {
    let mut cache = LayoutCache::default();
    let healthy = empty();
    cache.insert("Healthy", Arc::clone(&healthy));
    let before = cache.bytes;
    let oversized = Arc::new(LineLayout {
        glyphs: Vec::with_capacity(MAX_CACHE_BYTES / size_of::<PositionedGlyph>() + 1),
        ..Arc::try_unwrap(empty()).unwrap()
    });
    cache.insert("large", oversized);
    cache.insert(&"x".repeat(MAX_CACHE_BYTES), empty());
    assert_eq!(cache.bytes, before);
    assert_eq!(cache.entries.len(), 1);
    assert!(Arc::ptr_eq(&cache.get("Healthy").unwrap(), &healthy));
    cache.clock = u64::MAX;
    assert!(cache.get("Healthy").is_some());
    assert_eq!(cache.clock, 1);
    assert_accounted(&cache);
}
