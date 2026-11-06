use motion_render::{
    AllocationBudget, AllocationCategory, ChromaLayout, Renderer, ResourceBudgetScope,
    ResourceRejection, TextureCategory, Yuv420Frame,
};

const SYSTEM: u64 = motion_render::FIXED_RENDERER_ALLOCATION_BYTES;
const RGBA: u64 = 2 * 2 * 4;
const PLANES: u64 = 2 * 2 + 2;
const UNIFORM: u64 = 48;
const CONVERSION: u64 = RGBA + PLANES + UNIFORM;

fn frame(luma: u8) -> Yuv420Frame {
    Yuv420Frame {
        width: 2,
        height: 2,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: ChromaLayout::Uv,
        y: vec![luma; 4],
        uv: vec![128; 2],
    }
}

fn flush(renderer: &Renderer) {
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.submit_tracked(encoder.finish(), |_| {
        panic!("optional submission diagnostics must remain disabled")
    });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
}

fn category_bytes(budget: &AllocationBudget, category: AllocationCategory) -> u64 {
    budget
        .info()
        .unwrap()
        .categories
        .into_iter()
        .find(|entry| entry.category == category)
        .unwrap()
        .admitted_bytes
}

fn video_textures(renderer: &Renderer) -> (usize, u64, u64) {
    let ledger = renderer.texture_resource_ledger();
    let mut count = 0;
    let mut rgba = 0;
    let mut planes = 0;
    for resource in ledger.resources() {
        let is_rgba = resource
            .references
            .iter()
            .any(|reference| reference.category == TextureCategory::VideoRgba);
        let is_plane = resource
            .references
            .iter()
            .any(|reference| reference.category == TextureCategory::VideoPlanes);
        assert!(!(is_rgba && is_plane));
        if is_rgba || is_plane {
            count += 1;
            let bytes = resource
                .logical_bytes
                .expect("video texture capacity is known");
            if is_rgba {
                rgba += bytes;
            } else {
                planes += bytes;
            }
        }
    }
    (count, rgba, planes)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn sixty_retired_clips_keep_unsubmitted_conversions_admitted_until_cancel_or_completion() {
    let budget = AllocationBudget::new(SYSTEM + 3 * CONVERSION).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    flush(&renderer);
    let baseline = budget.info().unwrap();
    assert_eq!(baseline.admitted_bytes, SYSTEM);
    let baseline_uniforms = category_bytes(&budget, AllocationCategory::UniformBuffers);

    // Each batch owns three independent encoders. Their source images are
    // removed before submission, just as when replacing a project or input.
    // No timing assumption about GPU execution is needed: these encoders have
    // not been submitted when the pressure and cancellation checks run.
    for batch in 0..20u64 {
        let mut pending = Vec::new();
        for slot in 0..3u64 {
            let object = batch * 3 + slot + 1;
            let mut encoder = renderer.device.create_command_encoder(&Default::default());
            let token = renderer
                .encode_video_yuv(
                    object,
                    10,
                    object * 33_333,
                    &frame(32 + object as u8),
                    &mut encoder,
                )
                .unwrap()
                .expect("new clip requires conversion");
            pending.push(Some((encoder, token)));
            assert_eq!(
                video_textures(&renderer),
                (
                    (slot as usize + 1) * 3,
                    (slot + 1) * RGBA,
                    (slot + 1) * PLANES
                )
            );
        }
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            SYSTEM + 3 * CONVERSION
        );
        assert_eq!(
            budget.info().unwrap().live_allocations,
            baseline.live_allocations + 12
        );
        assert_eq!(
            category_bytes(&budget, AllocationCategory::Video),
            3 * (RGBA + PLANES)
        );
        assert_eq!(
            category_bytes(&budget, AllocationCategory::UniformBuffers),
            baseline_uniforms + 3 * UNIFORM
        );

        renderer.clear_assets();
        assert_eq!(video_textures(&renderer), (0, 0, 0));
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            SYSTEM + 3 * CONVERSION,
            "cache removal must not return capacity owned by unsubmitted tokens"
        );

        let mut probe = renderer.device.create_command_encoder(&Default::default());
        let error = renderer
            .encode_video_yuv(10_000, 10, batch, &frame(128), &mut probe)
            .err()
            .expect("a fourth output must be refused at the actual peak");
        assert_eq!(
            error.resource_rejection(),
            Some(&ResourceRejection::Budget {
                scope: ResourceBudgetScope::RenderAllocations,
                required_bytes: SYSTEM + 3 * CONVERSION + RGBA,
                budget_bytes: SYSTEM + 3 * CONVERSION,
            })
        );
        assert_eq!(video_textures(&renderer), (0, 0, 0));
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            SYSTEM + 3 * CONVERSION
        );

        // Vary the cancelled slot so every encoder position is exercised.
        let cancelled = batch as usize % pending.len();
        let (encoder, token) = pending[cancelled].take().unwrap();
        drop(encoder);
        drop(token);
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            SYSTEM + 3 * CONVERSION - RGBA
        );
        assert_eq!(
            category_bytes(&budget, AllocationCategory::Video),
            3 * (RGBA + PLANES) - RGBA
        );

        // An empty real submission drains queued plane/uniform writes. It
        // must release the cancelled clip, but not the other two tokens.
        flush(&renderer);
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            SYSTEM + 2 * CONVERSION
        );
        assert_eq!(
            budget.info().unwrap().live_allocations,
            baseline.live_allocations + 8
        );
        assert_eq!(
            category_bytes(&budget, AllocationCategory::Video),
            2 * (RGBA + PLANES)
        );

        for (encoder, token) in pending.into_iter().flatten() {
            renderer.queue.submit(Some(encoder.finish()));
            token.mark_submitted();
        }
        flush(&renderer);
        let after = budget.info().unwrap();
        assert_eq!(
            after.admitted_bytes, baseline.admitted_bytes,
            "batch={batch}"
        );
        assert_eq!(
            after.live_allocations, baseline.live_allocations,
            "batch={batch}"
        );
        assert_eq!(category_bytes(&budget, AllocationCategory::Video), 0);
        assert_eq!(
            category_bytes(&budget, AllocationCategory::UniformBuffers),
            baseline_uniforms
        );
        assert_eq!(video_textures(&renderer), (0, 0, 0));
    }
    let final_info = budget.info().unwrap();
    assert_eq!(final_info.peak_admitted_bytes, SYSTEM + 3 * CONVERSION);
    eprintln!(
        "video retirement: 60 clips, 20 cancellations, baseline={}, peak={}, final={}",
        baseline.admitted_bytes, final_info.peak_admitted_bytes, final_info.admitted_bytes
    );
}
