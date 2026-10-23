use super::*;

#[test]
fn actual_font_cache_reports_coverage_without_io_or_lru_changes_and_keeps_parser_unknown() {
    let root = tempfile::tempdir().unwrap();
    let mut store = FontStore::new(root.path()).unwrap();
    let id = FontStore::builtin_id();
    let glyph = store.glyph(&id, 'A', 32.).unwrap();
    let again = store.glyph(&id, 'A', 32.).unwrap();
    assert!(Arc::ptr_eq(&glyph, &again));
    let hits = store.cache_hits;
    let strong = Arc::strong_count(&glyph);
    // An explicit read must still work after the disk font has disappeared.
    fs::remove_file(store.path(&id, "ttf").unwrap()).unwrap();
    let info = store.memory_info();
    assert_eq!(info.known_payload_bytes, glyph.coverage.len() as u64);
    assert_eq!(info.known_capacity_bytes, glyph.coverage.capacity() as u64);
    assert_eq!(info.resources.len(), 1);
    assert_eq!(info.resources[0].references[0]["codePoint"], 'A' as u32);
    assert!(info.details["faceCapacityBytes"].is_null());
    assert_eq!(info.details["cachedGlyphReferences"], 1);
    assert_eq!(store.cache_hits, hits);
    assert_eq!(Arc::strong_count(&glyph), strong);
    assert_eq!(
        serde_json::to_value(info).unwrap(),
        serde_json::to_value(store.memory_info()).unwrap()
    );
}

#[test]
fn glyph_aliases_count_once_but_equal_independent_coverage_counts_separately() {
    let root = tempfile::tempdir().unwrap();
    let mut store = FontStore::new(root.path()).unwrap();
    let id = FontStore::builtin_id();
    let rendered = store.glyph(&id, 'A', 32.).unwrap();
    let mut coverage = Vec::with_capacity(rendered.coverage.len() + 64);
    coverage.extend_from_slice(&rendered.coverage);
    let glyph = Arc::new(Glyph {
        metrics: rendered.metrics.clone(),
        coverage,
    });
    store.glyphs.clear();
    store
        .glyphs
        .push_back(((id.clone(), 'A', 32f32.to_bits()), glyph.clone()));
    store
        .glyphs
        .push_back(((id.clone(), 'B', 32f32.to_bits()), glyph.clone()));
    store.glyph_bytes = 2 * glyph.coverage.len();
    let info = store.memory_info();
    assert_eq!(info.known_payload_bytes, glyph.coverage.len() as u64);
    assert_eq!(info.known_capacity_bytes, glyph.coverage.capacity() as u64);
    assert!(info.known_capacity_bytes > info.known_payload_bytes);
    assert_eq!(info.resources[0].references.len(), 2);
    assert_eq!(
        store.cache_bytes(),
        2 * glyph.coverage.len(),
        "booked cache semantics must not change"
    );
    store.glyphs.push_back((
        (id, 'C', 32f32.to_bits()),
        Arc::new(Glyph {
            metrics: glyph.metrics.clone(),
            coverage: glyph.coverage.clone(),
        }),
    ));
    let distinct = store.memory_info();
    assert_eq!(distinct.resources.len(), 2);
    assert_eq!(
        distinct.known_payload_bytes,
        2 * glyph.coverage.len() as u64
    );
}

#[test]
fn actual_glyph_eviction_excludes_escaped_arc_and_preserves_remaining_coverage() {
    let root = tempfile::tempdir().unwrap();
    let mut store = FontStore::new(root.path()).unwrap();
    let id = FontStore::builtin_id();
    let escaped = store.glyph(&id, 'A', 32.).unwrap();
    for index in 0..2049 {
        store.glyph(&id, 'A', 4. + index as f32 / 4096.).unwrap();
    }
    assert_eq!(store.glyphs.len(), 2048);
    assert!(store.glyphs.iter().all(|(_, g)| !Arc::ptr_eq(g, &escaped)));
    assert!(
        !escaped.coverage.is_empty(),
        "external ownership remains outside this cache observation"
    );
    let observed = store.memory_info();
    assert_eq!(observed.resources.len(), 2048);
    assert_eq!(observed.known_payload_bytes, store.cache_bytes() as u64);
    assert!(!observed.complete_coverage);
    assert!(observed.details["unknownBytes"].is_null());
}
