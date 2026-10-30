use super::*;
use crate::renderer::composition_frames::tests::{context, key, read};
use motion_effects::image_contract::ChannelPrecision;

fn image(precision: ChannelPrecision) -> crate::CapturedImage {
    let pixel: Vec<u8> = match precision {
        ChannelPrecision::Unorm8 => vec![230, 17, 91, 128],
        ChannelPrecision::Unorm16 => [60000u16, 12, 8192, 32768]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect(),
        ChannelPrecision::Float16 => [0x4100u16, 0xb800, 0x3000, 0x3800]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect(),
        ChannelPrecision::Float32 => [2.5f32, -0.5, 0.125, 0.5]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect(),
    };
    crate::CapturedImage {
        contract: native_contract(precision),
        storage: crate::image_formats::color_storage(precision),
        width: 4,
        height: 4,
        pixels: pixel.repeat(16),
    }
}

fn renderer(precision: ChannelPrecision) -> Renderer {
    pollster::block_on(Renderer::headless_with_working_precision(
        precision,
        crate::AllocationBudget::default(),
    ))
    .unwrap()
}
fn wait(renderer: &Renderer) {
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}
fn frame(image: &crate::CapturedImage, number: f64) -> CompositionFrameKey {
    CompositionFrameKey::new(
        context(),
        number,
        image.width,
        image.height,
        crate::image_formats::native_format(image.storage),
    )
    .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn protected_native_admission_refuses_atomically_and_evicts_only_unprotected_pixels() {
    let renderer = renderer(ChannelPrecision::Unorm8);
    let image = image(ChannelPrecision::Unorm8);
    let mut cache = CompositionFrameCache::new(context(), 128, 2).unwrap();
    for number in [0., 1.] {
        let target = cache
            .prepare_native(&renderer, frame(&image, number))
            .unwrap()
            .unwrap();
        assert!(cache.install_native(&renderer, target, &image).unwrap());
    }
    wait(&renderer);
    let before = cache.info();
    let bytes = renderer.allocation_budget.info().unwrap().admitted_bytes;
    assert!(cache
        .prepare_native_preserving(
            &renderer,
            frame(&image, 2.),
            &[frame(&image, 0.), frame(&image, 1.)]
        )
        .unwrap()
        .is_none());
    assert_eq!(cache.info().evicted, before.evicted);
    assert_eq!(cache.info().resident_frames, 2);
    assert_eq!(cache.info().pending_bytes, 0);
    assert_eq!(cache.info().hits, before.hits);
    assert_eq!(cache.info().misses, before.misses);
    assert_eq!(
        renderer.allocation_budget.info().unwrap().admitted_bytes,
        bytes
    );
    let kept = cache.get(&renderer, frame(&image, 1.)).unwrap().unwrap();
    let target = cache
        .prepare_native_preserving(&renderer, frame(&image, 2.), &[frame(&image, 1.)])
        .unwrap()
        .unwrap();
    assert_eq!(cache.info().evicted, before.evicted + 1);
    assert!(cache.install_native(&renderer, target, &image).unwrap());
    assert!(cache.get(&renderer, frame(&image, 0.)).unwrap().is_none());
    let again = cache.get(&renderer, frame(&image, 1.)).unwrap().unwrap();
    assert!(Arc::ptr_eq(&kept, &again));
    assert_eq!(
        renderer
            .read_native_render_target(again.source())
            .unwrap()
            .pixels,
        image.pixels
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_destination_is_really_admitted_before_dispatch_and_shares_cache_limits() {
    let renderer = renderer(ChannelPrecision::Unorm8);
    wait(&renderer);
    let baseline = renderer.allocation_budget.info().unwrap().admitted_bytes;
    let image = image(ChannelPrecision::Unorm8);
    let mut cache = CompositionFrameCache::new(context(), 128, 2).unwrap();
    let candidate = cache
        .prepare_native(&renderer, frame(&image, 0.))
        .unwrap()
        .unwrap();
    assert_eq!(candidate.key(), frame(&image, 0.));
    assert_eq!(candidate.texture_bytes(), 64);
    assert_eq!(
        renderer.allocation_budget.info().unwrap().admitted_bytes,
        baseline + 64
    );
    assert_eq!(cache.info().pending_frames, 1);
    assert_eq!(cache.info().pending_bytes, 64);
    assert_eq!(cache.info().resident_frames, 0);
    assert!(cache
        .prepare_native(&renderer, frame(&image, 1.))
        .unwrap()
        .is_none());
    // Existing GPU-copy writes must account for the admitted pending slot too.
    let source = renderer.render_target(4, 4).unwrap();
    assert!(cache.store(&renderer, key(&source, 1.), &source).unwrap());
    assert!(cache.store(&renderer, key(&source, 2.), &source).unwrap());
    assert_eq!(cache.info().resident_frames, 1);
    assert_eq!(
        cache.info().resident_bytes + cache.info().pending_bytes,
        128
    );
    assert!(cache.install_native(&renderer, candidate, &image).unwrap());
    assert_eq!(cache.info().pending_frames, 0);
    assert_eq!(cache.info().resident_frames, 2);
    assert_eq!(cache.info().resident_bytes, 128);
    let stored = cache.get(&renderer, frame(&image, 0.)).unwrap().unwrap();
    assert_eq!(read(&renderer, stored.source()), image.pixels);
    cache.clear();
    drop(stored);
    drop(source);
    wait(&renderer);
    assert_eq!(
        renderer.allocation_budget.info().unwrap().admitted_bytes,
        baseline
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalid_native_completions_preserve_valid_pixels_and_expired_reservations_recover() {
    let renderer = renderer(ChannelPrecision::Unorm8);
    let image = image(ChannelPrecision::Unorm8);
    let mut cache = CompositionFrameCache::new(context(), 128, 2).unwrap();
    let first = frame(&image, 0.);
    let candidate = cache.prepare_native(&renderer, first).unwrap().unwrap();
    assert!(cache.install_native(&renderer, candidate, &image).unwrap());
    for defect in 0..5 {
        let candidate = cache
            .prepare_native(&renderer, frame(&image, 1.))
            .unwrap()
            .unwrap();
        let mut bad = self::image(ChannelPrecision::Unorm8);
        match defect {
            0 => {
                bad.pixels.pop();
            }
            1 => bad.width = 2,
            2 => bad.storage = motion_effects::image_contract::ImageStorage::Rgba8Unorm,
            3 => {
                bad.contract = ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Straight,
                    },
                    ChannelPrecision::Unorm8,
                )
            }
            _ => bad.contract.precision = ChannelPrecision::Float16,
        }
        assert!(cache.install_native(&renderer, candidate, &bad).is_err());
        assert_eq!(cache.info().pending_frames, 0);
        let valid = cache.get(&renderer, first).unwrap().unwrap();
        assert_eq!(read(&renderer, valid.source()), image.pixels);
    }
    let abandoned = cache
        .prepare_native(&renderer, frame(&image, 1.))
        .unwrap()
        .unwrap();
    drop(abandoned);
    assert_eq!(cache.info().pending_bytes, 0);
    let recovered = cache
        .prepare_native(&renderer, frame(&image, 1.))
        .unwrap()
        .unwrap();
    assert!(cache.install_native(&renderer, recovered, &image).unwrap());
    assert_eq!(cache.info().installed, 2);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_candidate_cache_identity_device_context_and_policy_cancel_stale_installation() {
    let renderer = renderer(ChannelPrecision::Unorm8);
    let foreign = self::renderer(ChannelPrecision::Unorm8);
    let image = image(ChannelPrecision::Unorm8);
    let mut a = CompositionFrameCache::new(context(), 128, 2).unwrap();
    let mut b = CompositionFrameCache::new(context(), 128, 2).unwrap();
    let first = frame(&image, 0.);
    let candidate = a.prepare_native(&renderer, first).unwrap().unwrap();
    assert!(!b.install_native(&foreign, candidate, &image).unwrap());
    let candidate = a.prepare_native(&renderer, first).unwrap().unwrap();
    assert!(a.install_native(&foreign, candidate, &image).is_err());
    let candidate = a.prepare_native(&renderer, first).unwrap().unwrap();
    let mut changed = context();
    changed.device_generation += 1;
    a.invalidate(changed);
    // Stale acknowledgments are discarded before accessing the old GPU owner.
    assert!(!a.install_native(&foreign, candidate, &image).unwrap());
    assert!(a.prepare_native(&renderer, first).unwrap().is_none());
    a.invalidate(context());
    let candidate = a.prepare_native(&renderer, first).unwrap().unwrap();
    a.set_limits(0, 0).unwrap();
    assert!(!a.install_native(&foreign, candidate, &image).unwrap());
    assert_eq!(a.info().resident_frames, 0);
    assert_eq!(a.info().pending_frames, 0);
    a.set_limits(128, 2).unwrap();
    let candidate = a.prepare_native(&renderer, first).unwrap().unwrap();
    assert!(a.install_native(&renderer, candidate, &image).unwrap());
    assert_eq!(
        read(
            &renderer,
            a.get(&renderer, first).unwrap().unwrap().source()
        ),
        image.pixels
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_candidate_shared_budget_refusal_has_no_fake_reservation_and_recovers() {
    let renderer = renderer(ChannelPrecision::Float16);
    wait(&renderer);
    let image = image(ChannelPrecision::Float16);
    let key = frame(&image, 0.25);
    let mut cache = CompositionFrameCache::new(context(), 128, 1).unwrap();
    let budget = renderer.allocation_budget.info().unwrap();
    renderer
        .allocation_budget
        .set_limit(budget.admitted_bytes)
        .unwrap();
    assert!(cache.prepare_native(&renderer, key).is_err());
    assert_eq!(cache.info().pending_frames, 0);
    assert_eq!(cache.info().pending_bytes, 0);
    assert_eq!(
        renderer.allocation_budget.info().unwrap().admitted_bytes,
        budget.admitted_bytes
    );
    renderer
        .allocation_budget
        .set_limit(budget.budget_bytes)
        .unwrap();
    let candidate = cache.prepare_native(&renderer, key).unwrap().unwrap();
    assert!(cache.install_native(&renderer, candidate, &image).unwrap());
    let retained = cache.get(&renderer, key).unwrap().unwrap();
    assert_eq!(read(&renderer, retained.source()), image.pixels);
    cache.clear();
    assert_eq!(cache.info().resident_bytes, 0);
    assert_eq!(
        renderer.allocation_budget.info().unwrap().admitted_bytes,
        budget.admitted_bytes + 128
    );
    drop(retained);
    wait(&renderer);
    assert_eq!(
        renderer.allocation_budget.info().unwrap().admitted_bytes,
        budget.admitted_bytes
    );
}

#[test]
#[ignore = "explicit native upload checkpoint for all four optional working precisions"]
fn all_four_native_completion_uploads_preserve_encoded_storage_hdr_negative_and_alpha() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let renderer = renderer(precision);
        let image = image(precision);
        let key = frame(&image, 0.125);
        let mut cache = CompositionFrameCache::new(context(), 4096, 2).unwrap();
        let candidate = cache.prepare_native(&renderer, key).unwrap().unwrap();
        assert_eq!(candidate.texture_bytes(), image.pixels.len() as u64);
        assert!(cache.install_native(&renderer, candidate, &image).unwrap());
        let stored = cache.get(&renderer, key).unwrap().unwrap();
        assert_eq!(
            renderer
                .read_native_render_target(stored.source())
                .unwrap()
                .pixels,
            image.pixels
        );
        assert_eq!(stored.key(), key);
        assert_eq!(
            read(&renderer, stored.source()),
            image.pixels,
            "{precision:?}"
        );
        let mut preview = crate::SdrPreviewFrameTargets::new(&renderer, 4, 4).unwrap();
        preview.prepare(&renderer, 4, 4).unwrap();
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        stored
            .copy_to(&renderer, preview.prepared().unwrap(), &mut encoder)
            .unwrap();
        preview.encode_output(&renderer, &mut encoder).unwrap();
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        assert_eq!(
            read(&renderer, preview.prepared().unwrap()),
            image.pixels,
            "native preview copy {precision:?}"
        );
        preview.commit_submitted().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cached_preview_copy_checks_owners_dimensions_format_and_usage_before_encoding() {
    let renderer = renderer(ChannelPrecision::Unorm8);
    let foreign = self::renderer(ChannelPrecision::Unorm8);
    let image = image(ChannelPrecision::Unorm8);
    let key = frame(&image, 1.);
    assert_eq!(key.context(), context());
    let mut cache = CompositionFrameCache::new(context(), 4096, 2).unwrap();
    let candidate = cache.prepare_native(&renderer, key).unwrap().unwrap();
    assert!(cache.install_native(&renderer, candidate, &image).unwrap());
    let stored = cache.get(&renderer, key).unwrap().unwrap();
    let mut preview = crate::PreviewFrameTargets::new(&renderer, 4, 4).unwrap();
    preview.prepare(&renderer, 4, 4).unwrap();
    let destination = preview.prepared().unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(stored.copy_to(&foreign, destination, &mut encoder).is_err());
    let mut foreign_encoder = foreign
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(stored
        .copy_to(&renderer, destination, &mut foreign_encoder)
        .is_err());
    let foreign_target = foreign.render_target(4, 4).unwrap();
    assert!(stored
        .copy_to(&renderer, &foreign_target, &mut encoder)
        .is_err());
    let no_copy_usage = renderer.render_target(4, 4).unwrap();
    assert!(stored
        .copy_to(&renderer, &no_copy_usage, &mut encoder)
        .is_err());
    for (size, format) in [
        (2, renderer.target_format),
        (4, wgpu::TextureFormat::Rgba8Unorm),
    ] {
        let bad = renderer
            .render_target_with_format(size, 4, format, crate::TextureCategory::PreviewTargets)
            .unwrap();
        assert!(stored.copy_to(&renderer, &bad, &mut encoder).is_err());
    }
    assert!(!preview.valid());
    stored
        .copy_to(&renderer, destination, &mut encoder)
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    assert_eq!(read(&renderer, destination), image.pixels);
    preview.commit_submitted();
    assert!(preview.valid());
    assert_eq!(read(&renderer, &preview), image.pixels);
    renderer.check_health().unwrap();
}
