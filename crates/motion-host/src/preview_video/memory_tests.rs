use super::*;

#[test]
fn proxy_snapshot_counts_owned_capacity_and_never_retains_weak_source_pixels() {
    let mut pixels = Vec::with_capacity(48);
    pixels.extend([1, 2, 3, 255]);
    let capacity = pixels.capacity() as u64;
    let mut cache = PreviewVideoCache::default();
    cache.entries.insert(
        9,
        Entry {
            frame: Weak::new(),
            source: 17,
            edge: 1,
            proxy: Proxy::Rgba {
                width: 1,
                height: 1,
                pixels,
            },
        },
    );
    let first = serde_json::to_value(cache.memory_ledger()).unwrap();
    assert_eq!(first["knownPayloadBytes"], 4);
    assert_eq!(first["knownCapacityBytes"], capacity);
    assert_eq!(first["resources"][0]["object"], 9);
    assert_eq!(first["sourceReferences"], "weak_non_owning");
    assert!(cache.entries[&9].frame.upgrade().is_none());
    let y = vec![128; 4];
    let uv = vec![100, 160];
    cache.entries.insert(
        3,
        Entry {
            frame: Weak::new(),
            source: 18,
            edge: 2,
            proxy: Proxy::Yuv(Yuv420Frame {
                width: 2,
                height: 2,
                rotation: 0,
                standard: 1,
                range: 2,
                phase: [0, 0],
                chroma_layout: motion_render::ChromaLayout::Uv,
                y,
                uv,
            }),
        },
    );
    let observed = serde_json::to_value(cache.memory_ledger()).unwrap();
    assert_eq!(observed["knownPayloadBytes"], 10);
    assert_eq!(observed["knownCapacityBytes"], capacity + 6);
    assert_eq!(observed["resources"][0]["object"], 3);
    cache.clear();
    assert_eq!(cache.memory_ledger().known_capacity_bytes, 0);
}
