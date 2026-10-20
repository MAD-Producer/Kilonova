use super::*;

fn renderer() -> Renderer {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.image_cache_enabled = true;
    renderer
}

fn upload(renderer: &mut Renderer, id: u64) {
    renderer.upload_image(id, 2, 2, &[255; 16]).unwrap();
    // Scene preparation records access after the public upload completes.
    renderer.touch_image(id);
}

fn measured(renderer: &Renderer) -> u64 {
    renderer.images.values().map(|image| image.bytes).sum()
}

#[test]
fn image_trim_preserves_lru_ties_active_and_replacement_protection() {
    let mut renderer = renderer();
    for id in 1..=4 {
        upload(&mut renderer, id);
    }
    renderer.image_active.insert(1);
    renderer
        .image_access
        .extend([(1, 0), (2, 1), (3, 7), (4, 7)]);
    renderer.effect_gpu.builder.source_texture_budget = 52;
    renderer.trim_image_cache_except(0, Some(TextureKey::Static(2)), 0);
    assert_eq!(
        renderer.image_dimensions(3),
        None,
        "oldest eligible tie must use the lower ID"
    );
    for id in [1, 2, 4] {
        assert_eq!(renderer.image_dimensions(id), Some((2, 2)));
    }
    assert_eq!(renderer.texture_bytes, 52);
    assert_eq!(renderer.texture_bytes, measured(&renderer));
    // The replacement exclusion applies only to that call, not a later trim.
    renderer.effect_gpu.builder.source_texture_budget = 36;
    renderer.trim_image_cache(0);
    assert_eq!(renderer.image_dimensions(2), None);
    assert_eq!(renderer.image_dimensions(1), Some((2, 2)));
    assert_eq!(renderer.image_dimensions(4), Some((2, 2)));
    assert_eq!(renderer.texture_bytes, 36);
    assert_eq!(renderer.texture_bytes, measured(&renderer));
}

#[test]
fn repeated_pressure_reuses_one_eviction_allocation_and_healthy_trims_leave_it_idle() {
    let mut renderer = renderer();
    renderer.image_active.insert(1);
    let mut retained = None;
    for _ in 0..64 {
        renderer.effect_gpu.builder.source_texture_budget = TEXTURE_BUDGET;
        for id in 1..=8 {
            upload(&mut renderer, id);
        }
        renderer.effect_gpu.builder.source_texture_budget = 116;
        renderer.trim_image_cache_except(0, Some(TextureKey::Static(2)), 0);
        assert_eq!(renderer.image_dimensions(3), None);
        for id in [1, 2, 4, 5, 6, 7, 8] {
            assert_eq!(renderer.image_dimensions(id), Some((2, 2)));
        }
        assert_eq!(renderer.texture_bytes, 116);
        assert_eq!(renderer.texture_bytes, measured(&renderer));
        assert!(renderer.image_trim_candidates.is_empty());
        let capacity = renderer.image_trim_candidates.capacity();
        assert!(
            capacity >= 6,
            "eviction storage was discarded instead of reused"
        );
        assert!(capacity <= MAX_IMAGE_TRIM_CANDIDATES);
        let allocation = (renderer.image_trim_candidates.as_ptr(), capacity);
        if let Some(previous) = retained {
            assert_eq!(
                allocation, previous,
                "same-size pressure allocated another backing store"
            );
        }
        retained = Some(allocation);
        let images = renderer.images.len();
        renderer.trim_image_cache(0);
        assert_eq!(renderer.images.len(), images);
        assert_eq!(renderer.texture_bytes, 116);
        assert_eq!(
            (
                renderer.image_trim_candidates.as_ptr(),
                renderer.image_trim_candidates.capacity()
            ),
            allocation
        );
    }
    renderer.reclaim_preview_working_set().unwrap();
    assert_eq!(renderer.image_dimensions(1), Some((2, 2)));
    assert_eq!(renderer.texture_bytes, 20);
    assert_eq!(renderer.image_trim_candidates.capacity(), 0);
}

#[test]
fn oversized_eviction_storage_is_not_retained_and_clear_releases_small_storage() {
    let mut renderer = renderer();
    for id in 1..=MAX_IMAGE_TRIM_CANDIDATES as u64 + 1 {
        upload(&mut renderer, id);
    }
    renderer.image_cache_enabled = false;
    renderer.trim_image_cache(0);
    assert_eq!(renderer.image_count(), 1);
    assert_eq!(renderer.texture_bytes, 4);
    assert_eq!(
        renderer.image_trim_candidates.capacity(),
        0,
        "large scenes must not retain unbounded CPU scratch"
    );
    renderer.image_cache_enabled = true;
    for id in 1..=8 {
        upload(&mut renderer, id);
    }
    renderer.image_cache_enabled = false;
    renderer.trim_image_cache(0);
    assert!(renderer.image_trim_candidates.capacity() >= 8);
    renderer.clear_assets();
    assert_eq!(renderer.image_count(), 1);
    assert_eq!(renderer.texture_bytes, 4);
    assert_eq!(renderer.texture_bytes, measured(&renderer));
    assert_eq!(renderer.image_trim_candidates.capacity(), 0);
}
