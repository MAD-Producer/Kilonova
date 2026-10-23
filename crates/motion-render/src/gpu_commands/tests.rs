use super::*;
use crate::{AllocationBudget, FrameOperation};

fn renderer(budget: AllocationBudget) -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    pollster::block_on(Renderer::new_profiled_with_allocation_budget(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8Unorm,
        true,
        budget,
    ))
    .unwrap()
}
fn contexts(request: u64) -> Vec<CommandContext> {
    vec![CommandContext {
        allocation_frame: request,
        request: Some(request),
        operation: FrameOperation::Present,
        source_frame: Some(1.25),
        exposure_request: Some(73),
        exposure_poll: Some(request),
    }]
}
fn encoder(profiler: &GpuCommandProfiler, renderer: &Renderer, request: u64) -> AllocationEncoder {
    profiler
        .encoder(
            renderer,
            &wgpu::CommandEncoderDescriptor::default(),
            CommandStage::CompositionAndPresentation,
            contexts(request),
        )
        .unwrap()
}
fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("actual command profiler clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }));
}
fn poll(renderer: &Renderer) {
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_commands_publish_before_finish_and_preserve_discard_owner_and_epoch_boundaries() {
    let budget = AllocationBudget::default();
    let renderer = renderer(budget.clone());
    let before = budget.info().unwrap().admitted_bytes;
    let Some(profiler) = GpuCommandProfiler::new(&renderer, 1).unwrap() else {
        eprintln!("GPU_COMMAND_NATIVE available=false; no native timing acceptance");
        return;
    };
    assert!(!renderer
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS));
    let observer = profiler.observer();
    assert_eq!(
        budget.info().unwrap().admitted_bytes - before,
        wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT + WORD_BYTES
    );

    let first = encoder(&profiler, &renderer, 31);
    let active = observer.snapshot();
    assert_eq!(active.commands.active.len(), 1);
    assert_eq!(active.commands.active[0].phase, CommandPhase::Encoding);
    assert_eq!(active.commands.active[0].contexts[0].request, Some(31));
    assert!(active.commands.active[0].gpu_elapsed_us.is_none());
    // A full observation pool must leave the actual command usable.
    let unobserved = encoder(&profiler, &renderer, 32);
    renderer
        .submit_allocated(unobserved.finish(), |_| {})
        .unwrap();
    assert_eq!(observer.snapshot().commands.counts.omitted_slots, 1);
    let finished = first.finish();
    assert_eq!(
        observer.snapshot().commands.active[0].phase,
        CommandPhase::Encoded
    );
    drop(finished);
    let discarded = observer.snapshot();
    assert!(discarded.commands.active.is_empty());
    assert_eq!(discarded.commands.counts.discarded, 1);
    assert_eq!(discarded.readback.map_requests, 0);
    assert_eq!(discarded.readback.encoded_resolve_bytes, WORD_BYTES);

    // Both renderers share admission but retain distinct native queue owners.
    let foreign = self::renderer(budget.clone());
    assert!(profiler
        .encoder(
            &foreign,
            &wgpu::CommandEncoderDescriptor::default(),
            CommandStage::Unclassified,
            contexts(33)
        )
        .err()
        .unwrap()
        .to_string()
        .contains("another renderer"));
    let wrong_owner = encoder(&profiler, &renderer, 34).finish();
    let error = foreign.submit_allocated(wrong_owner, |_| {}).unwrap_err();
    assert!(error.to_string().contains("another render queue"));
    assert_eq!(observer.snapshot().commands.counts.discarded, 2);
    assert_eq!(observer.snapshot().commands.counts.unconfirmed, 0);

    let current = budget.info().unwrap().admitted_bytes;
    budget.set_limit(current).unwrap();
    assert!(GpuCommandProfiler::new(&renderer, 1)
        .err()
        .unwrap()
        .resource_rejection()
        .is_some());
    assert_eq!(budget.info().unwrap().admitted_bytes, current);
    budget
        .set_limit(crate::DEFAULT_RENDER_ALLOCATION_BUDGET)
        .unwrap();

    let target = renderer.capture_target(8, 8).unwrap();
    let mut actual = encoder(&profiler, &renderer, 35);
    clear(&mut actual, &target.view);
    renderer.submit_allocated(actual.finish(), |_| {}).unwrap();
    // Drop the profiling owner while its exact native mapping is still pending.
    drop(profiler);
    let restarted = GpuCommandProfiler::new(&renderer, 1).unwrap().unwrap();
    assert_ne!(
        observer.snapshot().commands.epoch,
        restarted.observer().snapshot().commands.epoch
    );
    poll(&renderer);
    let completed = observer.snapshot();
    assert_eq!(completed.commands.counts.completed, 1);
    assert_eq!(completed.commands.counts.map_failed, 0);
    assert!(!completed.commands.observation_fault);
    assert!(completed.commands.active.is_empty());
    let sample = completed.commands.recent.last().unwrap();
    assert_eq!(sample.contexts[0].request, Some(35));
    assert_eq!(sample.phase, CommandPhase::Complete);
    let words = sample.timestamp_words.unwrap();
    assert!(words[1] > words[0]);
    assert!(sample.gpu_elapsed_us.unwrap() > 0.0);
    assert_eq!(completed.readback.map_requests, 1);
    assert_eq!(completed.readback.mapped_buffer_bytes, WORD_BYTES);
    assert_eq!(completed.readback.valid_samples, 1);
    assert_eq!(restarted.observer().snapshot().commands.counts.completed, 0);
    assert!(renderer
        .read_target(&target)
        .unwrap()
        .chunks_exact(4)
        .all(|p| p == [255, 0, 0, 255]));
    eprintln!(
        "GPU_COMMAND_NATIVE {}",
        serde_json::json!({
            "available":true,"originalWords":words,"elapsedUs":sample.gpu_elapsed_us,
            "actualPixelsChecked":64,"completed":completed.commands.counts.completed,
            "discarded":completed.commands.counts.discarded,"omittedSlots":completed.commands.counts.omitted_slots,
            "mappedBytes":completed.readback.mapped_buffer_bytes,"adapter":renderer.adapter_info.name,
            "backend":format!("{:?}",renderer.adapter_info.backend),"scope":"explicit encoder primitive only; host coverage pending"
        })
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_mapping_cancellation_is_counted_once_and_the_slot_can_measure_later_real_work() {
    let renderer = renderer(AllocationBudget::default());
    let Some(profiler) = GpuCommandProfiler::new(&renderer, 1).unwrap() else {
        eprintln!("GPU_COMMAND_CANCEL available=false; no native cancellation acceptance");
        return;
    };
    let observer = profiler.observer();
    renderer
        .submit_allocated(encoder(&profiler, &renderer, 41).finish(), |_| {})
        .unwrap();
    profiler.pool.slots[0].mapped.unmap(); // Actual wgpu cancellation before polling.
    poll(&renderer);
    let cancelled = observer.snapshot();
    assert_eq!(cancelled.readback.map_failures, 1);
    assert_eq!(cancelled.commands.counts.map_failed, 1);
    assert_eq!(cancelled.commands.counts.completed, 0);
    assert!(cancelled.commands.recent[0].cause.is_some());
    assert!(cancelled.commands.recent[0].gpu_elapsed_us.is_none());
    assert_eq!(cancelled.quarantined_slots, 0);
    poll(&renderer);
    assert_eq!(observer.snapshot().readback, cancelled.readback);
    let target = renderer.capture_target(8, 8).unwrap();
    let mut actual = encoder(&profiler, &renderer, 42);
    clear(&mut actual, &target.view);
    renderer.submit_allocated(actual.finish(), |_| {}).unwrap();
    poll(&renderer);
    let recovered = observer.snapshot();
    assert_eq!(recovered.readback.map_requests, 2);
    assert_eq!(recovered.readback.map_failures, 1);
    assert_eq!(recovered.readback.valid_samples, 1);
    assert_eq!(recovered.commands.counts.completed, 1);
    assert_eq!(recovered.commands.counts.map_failed, 1);
    assert!(recovered.commands.active.is_empty());
    eprintln!(
        "GPU_COMMAND_CANCEL {}",
        serde_json::json!({
            "available":true,"actualMapFailures":recovered.readback.map_failures,
            "actualValidSamplesAfterRecovery":recovered.readback.valid_samples,
            "scope":"actual unmap callback and later native reuse; not a renderer fault"
        })
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_submit_unwind_quarantines_the_slot_and_retires_leases_only_after_queue_completion() {
    let renderer = renderer(AllocationBudget::default());
    let target = renderer.capture_target(8, 8).unwrap();
    let budget = renderer.allocation_budget();
    let before = budget.info().unwrap().admitted_bytes;
    let Some(profiler) = GpuCommandProfiler::new(&renderer, 1).unwrap() else {
        eprintln!("GPU_COMMAND_UNWIND available=false; no native unwind acceptance");
        return;
    };
    let observer = profiler.observer();
    let mut commands = ObservedEncoder::new(
        renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default()),
    );
    commands.observe(
        profiler
            .pool
            .begin(CommandStage::Unclassified, contexts(51)),
    );
    clear(&mut commands, &target.view);
    let finished = commands.finish();
    let primary = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        finished.submit(|buffer| {
            renderer.queue.submit([buffer]);
            panic!("controlled panic after actual GPU submit");
        });
    }))
    .unwrap_err();
    assert_eq!(
        primary.downcast_ref::<&str>().copied(),
        Some("controlled panic after actual GPU submit")
    );
    let snapshot = observer.snapshot();
    assert_eq!(snapshot.commands.counts.unconfirmed, 1);
    assert_eq!(snapshot.quarantined_slots, 1);
    assert_eq!(snapshot.readback.map_requests, 0);
    assert!(snapshot.commands.recent[0].gpu_elapsed_us.is_none());
    assert!(snapshot.commands.active.is_empty());
    // A quarantined slot cannot be reassigned, even after a later queue poll.
    drop(encoder(&profiler, &renderer, 52));
    assert_eq!(observer.snapshot().commands.counts.omitted_slots, 1);
    drop(profiler);
    // The read-only observer retains metadata, not native buffers or the pool.
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        before + wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT + WORD_BYTES
    );
    poll(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, before);
    assert_eq!(observer.snapshot().commands.counts.unconfirmed, 1);
    assert!(renderer
        .read_target(&target)
        .unwrap()
        .chunks_exact(4)
        .all(|p| p == [255, 0, 0, 255]));
    eprintln!(
        "GPU_COMMAND_UNWIND {}",
        serde_json::json!({
            "available":true,"actualQueueSubmitted":true,"primaryPanicPreserved":true,
            "quarantinedSlots":snapshot.quarantined_slots,"admissionRestoredOnlyAfterActualPoll":true,
            "actualPixelsChecked":64
        })
    );
}
