use super::*;

fn image() -> Arc<Pixels> {
    let mut rgba = Vec::with_capacity(64);
    rgba.extend([17, 29, 41, 255]);
    Arc::new(Pixels {
        width: 1,
        height: 1,
        rgba,
        cached: false,
    })
}

#[test]
fn actual_image_arcs_keep_spare_capacity_and_distinct_equal_allocations_without_retention() {
    let shared = image();
    let different = image();
    let capacity = shared.rgba.capacity() + different.rgba.capacity();
    let mut collector = PixelCacheCollector::default();
    assert_eq!(collector.image(&shared, 17), collector.image(&shared, 18));
    collector.image(&different, 17);
    let observed = collector.finish("pixel-test");
    assert_eq!(observed.known_payload_bytes, 8);
    assert_eq!(observed.known_capacity_bytes, capacity as u64);
    assert_eq!(observed.details["resourceCount"], 2);
    assert_eq!(
        observed.details["resources"][0]["references"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(Arc::strong_count(&shared), 1);
    assert_eq!(Arc::strong_count(&different), 1);
    assert_eq!(shared.rgba, [17, 29, 41, 255]);
    assert!(observed.details["unknownBytes"].is_null());
}

#[test]
fn allocation_and_reference_limits_preserve_aliases_mark_unknown_and_reset_per_query() {
    let images: Vec<_> = (0..MAX_ALLOCATIONS + 1).map(|_| image()).collect();
    let mut collector = PixelCacheCollector::default();
    for pixels in &images {
        collector.image(pixels, 17);
    }
    for source in 0..10 {
        collector.image(&images[0], source);
    }
    let observed = collector.finish("bounded-pixel-test");
    assert_eq!(observed.details["resourceCount"], MAX_ALLOCATIONS);
    assert_eq!(observed.details["unobservedReferences"], 1);
    assert_eq!(observed.details["allObservedAllocationsKnown"], false);
    assert_eq!(
        observed.details["resources"][0]["references"]
            .as_array()
            .unwrap()
            .len(),
        MAX_REFERENCES
    );
    assert_eq!(observed.details["resources"][0]["omittedReferences"], 3);
    assert_eq!(observed.known_payload_bytes, (MAX_ALLOCATIONS * 4) as u64);
    assert_eq!(
        observed.known_capacity_bytes,
        images[..MAX_ALLOCATIONS]
            .iter()
            .map(|p| p.rgba.capacity() as u64)
            .sum::<u64>()
    );
    assert!(observed.details["unknownBytes"].is_null());
    assert!(images.iter().all(|p| Arc::strong_count(p) == 1));
    let empty = PixelCacheCollector::default().finish("bounded-pixel-test");
    assert_eq!(empty.known_capacity_bytes, 0);
    assert_eq!(empty.details["unobservedReferences"], 0);
    assert_eq!(empty.details["allObservedAllocationsKnown"], true);
}
