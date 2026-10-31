use super::*;

fn flush(renderer: &Renderer) {
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    wait(renderer);
}

#[test]
#[ignore = "explicit queued native upload checkpoint for all four optional working precisions"]
fn queued_native_upload_and_foreground_copy_share_one_submission_in_all_four_precisions() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let renderer = renderer(precision);
        flush(&renderer);
        renderer.set_submission_tracking(true);
        let submitted = || {
            renderer
                .submission_memory_info(&Default::default())
                .submitted
        };
        let before = submitted();
        let image = image(precision);
        let key = frame(&image, 0.25);
        let mut cache = CompositionFrameCache::new(context(), 4096, 2).unwrap();
        let target = cache.prepare_native(&renderer, key).unwrap().unwrap();
        assert!(
            cache
                .install_native_for_next_submission(&renderer, target, &image)
                .unwrap()
        );
        assert_eq!(
            submitted(),
            before,
            "installation must not submit a separate empty command"
        );
        let retained = cache.get(&renderer, key).unwrap().unwrap();
        let mut preview = crate::SdrPreviewFrameTargets::new(&renderer, 4, 4).unwrap();
        preview.prepare(&renderer, 4, 4).unwrap();
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        retained
            .copy_to(&renderer, preview.prepared().unwrap(), &mut encoder)
            .unwrap();
        // Only the encoder and queued upload retain these native allocations.
        cache.clear();
        drop(retained);
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        assert_eq!(
            submitted(),
            before + 1,
            "upload and copy must use one real submission"
        );
        assert_eq!(
            read(&renderer, preview.prepared().unwrap()),
            image.pixels,
            "{precision:?}"
        );
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unsubmitted_native_upload_retains_budget_after_eviction_until_a_real_flush() {
    let renderer = renderer(ChannelPrecision::Unorm8);
    flush(&renderer);
    let baseline = renderer.allocation_budget.info().unwrap().admitted_bytes;
    let image = image(ChannelPrecision::Unorm8);
    let key = frame(&image, 1.);
    let mut cache = CompositionFrameCache::new(context(), 64, 1).unwrap();
    let target = cache.prepare_native(&renderer, key).unwrap().unwrap();
    assert!(
        cache
            .install_native_for_next_submission(&renderer, target, &image)
            .unwrap()
    );
    cache.clear();
    assert_eq!(cache.info().resident_bytes, 0);
    wait(&renderer);
    assert_eq!(
        renderer.allocation_budget.info().unwrap().admitted_bytes,
        baseline + 64,
        "unsubmitted native destination must stay admitted after eviction"
    );
    flush(&renderer);
    assert_eq!(
        renderer.allocation_budget.info().unwrap().admitted_bytes,
        baseline
    );

    // Shutdown also flushes abandoned queued writes, even when callers retain
    // raw device/queue clones beyond the renderer's lifetime.
    let target = cache.prepare_native(&renderer, key).unwrap().unwrap();
    assert!(
        cache
            .install_native_for_next_submission(&renderer, target, &image)
            .unwrap()
    );
    cache.clear();
    let budget = renderer.allocation_budget.clone();
    let device = renderer.device.clone();
    let queue = renderer.queue.clone();
    drop(cache);
    drop(renderer);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        0,
        "renderer shutdown must retire the abandoned queued upload"
    );
    drop(queue);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn queued_native_upload_rejects_bad_contract_and_foreign_owner_before_queue_io() {
    let renderer = renderer(ChannelPrecision::Unorm8);
    let foreign = self::renderer(ChannelPrecision::Unorm8);
    flush(&renderer);
    renderer.set_submission_tracking(true);
    let mut cache = CompositionFrameCache::new(context(), 128, 2).unwrap();
    for defect in 0..5 {
        let mut image = image(ChannelPrecision::Unorm8);
        let key = frame(&image, 1.);
        let target = cache.prepare_native(&renderer, key).unwrap().unwrap();
        let owner = match defect {
            0 => &foreign,
            1 => {
                image.pixels.pop();
                &renderer
            }
            2 => {
                image.width = 2;
                &renderer
            }
            3 => {
                image.contract.precision = ChannelPrecision::Float16;
                &renderer
            }
            _ => {
                image.storage = motion_effects::image_contract::ImageStorage::Rgba8Unorm;
                &renderer
            }
        };
        assert!(
            cache
                .install_native_for_next_submission(owner, target, &image)
                .is_err()
        );
        assert_eq!(cache.info().resident_frames, 0);
        assert_eq!(cache.info().pending_frames, 0);
        assert_eq!(
            renderer
                .submission_memory_info(&Default::default())
                .submitted,
            0
        );
    }
    let image = image(ChannelPrecision::Unorm8);
    let key = frame(&image, 2.);
    let target = cache.prepare_native(&renderer, key).unwrap().unwrap();
    cache.invalidate(CompositionFrameContext {
        document_generation: 99,
        ..context()
    });
    assert!(
        !cache
            .install_native_for_next_submission(&foreign, target, &image)
            .unwrap(),
        "stale completion is discarded before the foreign owner is accessed"
    );
}
