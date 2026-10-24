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
fn native_trace_preserves_original_words_after_recent_eviction_and_stops_without_waiting() {
    let renderer = renderer(AllocationBudget::default());
    let Some(profiler) = GpuCommandProfiler::new(&renderer, 4).unwrap() else {
        eprintln!("GPU_COMMAND_TRACE available=false; no native full-trace acceptance");
        return;
    };
    let observer = profiler.observer();
    assert!(observer.trace_snapshot().is_none());
    let target = renderer.capture_target(8, 8).unwrap();
    let mut before_window = Vec::new();
    for request in 900..902 {
        let mut commands = encoder(&profiler, &renderer, request);
        clear(&mut commands, &target.view);
        before_window.push(commands.finish());
    }
    let window = observer.start_trace().unwrap();
    assert_eq!(
        observer.trace_snapshot().unwrap().excluded_active_at_start,
        2
    );
    for commands in before_window {
        renderer.submit_allocated(commands, |_| {}).unwrap();
    }
    poll(&renderer);
    assert!(observer.trace_snapshot().unwrap().records.is_empty());
    for batch in 0..65 {
        let mut batch_commands = Vec::new();
        for slot in 0..4 {
            let mut commands = encoder(&profiler, &renderer, 1000 + batch * 4 + slot);
            clear(&mut commands, &target.view);
            batch_commands.push(commands.finish());
        }
        if batch == 64 {
            assert_eq!(observer.stop_trace(), Some(window));
            let pending = observer.trace_snapshot().unwrap();
            assert_eq!(pending.started_commands, 260);
            assert_eq!(pending.records.len() + pending.active.len(), 260);
            assert_eq!(pending.active.len(), 4);
            assert_eq!(pending.stopped_at_command, Some(262));
        }
        for commands in batch_commands {
            renderer.submit_allocated(commands, |_| {}).unwrap();
        }
        poll(&renderer);
    }
    for request in 2000..2004 {
        let mut commands = encoder(&profiler, &renderer, request);
        clear(&mut commands, &target.view);
        renderer
            .submit_allocated(commands.finish(), |_| {})
            .unwrap();
    }
    poll(&renderer);
    assert_eq!(observer.stop_trace(), Some(window));
    let trace = observer.trace_snapshot().unwrap();
    let snapshot = observer.snapshot();
    assert_eq!(trace.records.len(), 260);
    assert!(trace.active.is_empty());
    assert!(!trace.truncated);
    assert_eq!(trace.omitted_records, 0);
    assert_eq!(trace.started_commands, 260);
    assert_eq!(trace.stopped_at_command, Some(262));
    assert_eq!(snapshot.commands.counts.completed, 266);
    assert_eq!(snapshot.readback.valid_samples, 266);
    assert_eq!(snapshot.commands.recent.len(), MAX_RECENT_GPU_COMMANDS);
    assert!(snapshot.commands.counts.evicted_recent >= 10);
    let requests: std::collections::BTreeSet<_> = trace
        .records
        .iter()
        .map(|record| {
            assert_eq!(record.epoch, trace.epoch);
            assert_eq!(record.phase, CommandPhase::Complete);
            assert!(record
                .timestamp_words
                .is_some_and(|words| words[1] > words[0]));
            assert!(record
                .gpu_elapsed_us
                .is_some_and(|elapsed| elapsed.is_finite() && elapsed > 0.0));
            assert_eq!(record.contexts.len(), 1);
            assert_eq!(record.contexts[0].source_frame, Some(1.25));
            assert!(record.id > 2 && record.id <= 262);
            record.contexts[0].request.unwrap()
        })
        .collect();
    assert_eq!(requests, (1000..1260).collect());
    assert!(!snapshot.commands.recent.iter().any(|record| record.id == 3));
    assert!(trace.records.iter().any(|record| record.id == 3));
    assert!(renderer
        .read_target(&target)
        .unwrap()
        .chunks_exact(4)
        .all(|pixel| pixel == [255, 0, 0, 255]));
    eprintln!(
        "GPU_COMMAND_TRACE {}",
        serde_json::json!({
            "available":true,"actualNativeSamples":266,"retainedTraceSamples":260,
            "excludedPreWindowSamples":2,"excludedPostWindowSamples":4,"actualPixelsChecked":64,
            "originalWordsRetained":true,"exactContextRequests":260,"sourceFramesIdentical":true,
            "scope":"actual per-encoder clear/resolve/map across ring eviction and nonblocking trace closure; not a whole-scene performance baseline"
        })
    );
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
    profiler.pool.slots[0]
        .cancelled
        .store(true, Ordering::Release);
    profiler.pool.slots[0].mapped.unmap(); // Actual owner cancellation before polling.
    poll(&renderer);
    let cancelled = observer.snapshot();
    assert_eq!(cancelled.readback.map_failures, 1);
    assert_eq!(cancelled.commands.counts.map_failed, 1);
    assert_eq!(cancelled.commands.counts.completed, 0);
    assert!(cancelled.commands.recent[0].cause.is_some());
    assert!(cancelled.commands.recent[0].gpu_elapsed_us.is_none());
    assert_eq!(cancelled.commands.recent[0].phase, CommandPhase::Cancelled);
    assert_eq!(cancelled.commands.counts.cancelled, 1);
    assert_eq!(cancelled.commands.consecutive_readback_failures, 0);
    assert!(profiler.pool.records.take_logs().events.is_empty());
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

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn automatic_attachment_joins_only_cpu_active_windows_and_preserves_late_callback_epochs() {
    fn assert_send<T: Send>() {}
    assert_send::<Renderer>(); // Session's existing registry/thread contract.
    let mut renderer = renderer(AllocationBudget::default());
    assert!(renderer.command_observer().is_none());
    let budget = renderer.allocation_budget().clone();
    let target = renderer.capture_target(8, 8).unwrap();
    let Some(observer) = renderer.start_command_profiling(2).unwrap() else {
        eprintln!("GPU_COMMAND_ATTACHED available=false; no native attachment acceptance");
        return;
    };
    let first = budget
        .begin_frame_at(401, FrameOperation::Present, 1.25)
        .unwrap()
        .unwrap();
    let mut commands = renderer
        .allocation_command_encoder_for(
            CommandStage::Composition,
            &wgpu::CommandEncoderDescriptor::default(),
        )
        .unwrap();
    let active = observer.snapshot();
    assert_eq!(active.commands.active[0].phase, CommandPhase::Encoding);
    assert_eq!(active.commands.active[0].stage, CommandStage::Composition);
    assert_eq!(active.commands.active[0].contexts[0].request, Some(401));
    assert_eq!(
        active.commands.active[0].contexts[0].source_frame,
        Some(1.25)
    );
    clear(&mut commands, &target.view);
    renderer
        .submit_allocated(commands.finish(), |_| {})
        .unwrap();
    first.finish(crate::AllocationFrameOutcome::Rendered);
    assert!(budget
        .frame_peaks()
        .unwrap()
        .active
        .iter()
        .any(|f| f.request == 401 && f.outcome.is_some()));

    let second = budget
        .begin_frame_at(402, FrameOperation::CaptureUi, 2.5)
        .unwrap()
        .unwrap();
    let mut commands = renderer
        .allocation_command_encoder_for(
            CommandStage::ReadbackCopy,
            &wgpu::CommandEncoderDescriptor::default(),
        )
        .unwrap();
    let active = observer.snapshot();
    let current = active
        .commands
        .active
        .iter()
        .find(|entry| entry.phase == CommandPhase::Encoding)
        .unwrap();
    assert_eq!(current.contexts.len(), 1);
    assert_eq!(current.contexts[0].request, Some(402));
    assert_eq!(current.contexts[0].source_frame, Some(2.5));
    clear(&mut commands, &target.view);
    renderer
        .submit_allocated(commands.finish(), |_| {})
        .unwrap();
    second.finish(crate::AllocationFrameOutcome::Rendered);

    let current_epoch = observer.snapshot().commands.epoch;
    let current_budget = budget.info().unwrap().admitted_bytes;
    budget.set_limit(current_budget).unwrap();
    assert!(renderer
        .start_command_profiling(1)
        .err()
        .unwrap()
        .resource_rejection()
        .is_some());
    assert_eq!(
        renderer
            .command_observer()
            .unwrap()
            .snapshot()
            .commands
            .epoch,
        current_epoch
    );
    budget
        .set_limit(crate::DEFAULT_RENDER_ALLOCATION_BUDGET)
        .unwrap();
    let retired = renderer.stop_command_profiling().unwrap();
    assert_eq!(retired.snapshot().commands.epoch, current_epoch);
    assert!(renderer.command_observer().is_none());
    let restarted = renderer.start_command_profiling(1).unwrap().unwrap();
    assert_ne!(restarted.snapshot().commands.epoch, current_epoch);
    poll(&renderer);
    let completed = observer.snapshot();
    assert!(completed.commands.active.is_empty());
    assert_eq!(completed.commands.counts.completed, 2);
    assert_eq!(restarted.snapshot().commands.counts.completed, 0);
    assert!(completed.commands.recent.iter().all(|entry| {
        entry.contexts.len() == 1 && entry.timestamp_words.is_some_and(|w| w[1] > w[0])
    }));
    assert!(renderer
        .read_target(&target)
        .unwrap()
        .chunks_exact(4)
        .all(|p| p == [255, 0, 0, 255]));
    eprintln!(
        "GPU_COMMAND_ATTACHED {}",
        serde_json::json!({
            "available":true,"actualCompletions":completed.commands.counts.completed,
            "requests":completed.commands.recent.iter().map(|entry|entry.contexts[0].request).collect::<Vec<_>>(),
            "actualPixelsChecked":64,"oldEpoch":current_epoch,"newEpoch":restarted.snapshot().commands.epoch,
            "scope":"actual automatic encoder attachment; CPU-ended frame exclusion and late callback identity"
        })
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_invalid_words_defer_one_escalation_at_sixty_then_reset_after_real_gpu_success() {
    let renderer = renderer(AllocationBudget::default());
    let Some(profiler) = GpuCommandProfiler::new(&renderer, 1).unwrap() else {
        eprintln!("GPU_COMMAND_INVALID available=false; no native invalid sample acceptance");
        return;
    };
    let observer = profiler.observer();
    let target = renderer.capture_target(8, 8).unwrap();
    let zero = renderer
        .allocation_context()
        .buffer(&wgpu::BufferDescriptor {
            label: Some("native invalid-word injection"),
            size: WORD_BYTES,
            usage: wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
        .unwrap();
    for request in 501..571 {
        let mut commands = ObservedEncoder::new(
            renderer
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default()),
        );
        commands.observe(
            profiler
                .pool
                .begin(CommandStage::ExposureMedia, contexts(request)),
        );
        clear(&mut commands, &target.view);
        let commands = commands.finish();
        // Inject invalid diagnostic words with an actual second GPU command,
        // after the real timestamps resolve. No mocked map result/ready flag.
        let mut overwrite = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        overwrite.copy_buffer_to_buffer(&zero, 0, &profiler.pool.slots[0].mapped, 0, WORD_BYTES);
        commands.submit(|commands| renderer.queue.submit([commands, overwrite.finish()]));
        poll(&renderer);
    }
    let invalid = observer.snapshot();
    assert_eq!(invalid.commands.counts.invalid, 70);
    assert_eq!(invalid.commands.counts.completed, 0);
    assert_eq!(invalid.readback.mapped_buffer_bytes, 70 * WORD_BYTES);
    assert!(invalid
        .commands
        .recent
        .iter()
        .all(|entry| entry.timestamp_words == Some([0, 0]) && entry.gpu_elapsed_us.is_none()));
    let logs = profiler.pool.records.take_logs();
    assert_eq!(logs.events.len(), 2);
    assert_eq!(
        (logs.events[0].level, logs.events[0].occurrence),
        (log::Level::Debug, 1)
    );
    assert_eq!(
        (logs.events[1].level, logs.events[1].occurrence),
        (log::Level::Error, 60)
    );
    assert_eq!(logs.events[1].record.contexts[0].request, Some(560));
    assert!(profiler.pool.records.take_logs().events.is_empty());
    let mut healthy = encoder(&profiler, &renderer, 571);
    clear(&mut healthy, &target.view);
    renderer.submit_allocated(healthy.finish(), |_| {}).unwrap();
    poll(&renderer);
    let recovered = observer.snapshot();
    assert_eq!(recovered.commands.consecutive_readback_failures, 0);
    assert_eq!(recovered.readback.valid_samples, 1);
    assert_eq!(recovered.commands.counts.completed, 1);
    assert!(renderer
        .read_target(&target)
        .unwrap()
        .chunks_exact(4)
        .all(|p| p == [255, 0, 0, 255]));
    eprintln!(
        "GPU_COMMAND_INVALID {}",
        serde_json::json!({
            "available":true,"actualInvalidMappedSamples":invalid.commands.counts.invalid,
            "actualMappedBytes":invalid.readback.mapped_buffer_bytes,"errorEscalations":1,
            "actualValidSamplesAfterRecovery":recovered.readback.valid_samples,
            "scope":"native diagnostic-buffer corruption injection; not a hardware driver failure"
        })
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn observed_raw_submission_keeps_original_scope_and_frame_completion_contract() {
    let budget = AllocationBudget::default();
    let mut renderer = renderer(budget.clone());
    let target = renderer.capture_target(8, 8).unwrap();
    let Some(observer) = renderer.start_command_profiling(1).unwrap() else {
        eprintln!("GPU_COMMAND_RAW available=false; no native raw-submit acceptance");
        return;
    };
    let foreign = self::renderer(budget.clone());
    let scopes = budget.info().unwrap().encoder_scopes;
    let mut wrong = renderer.tracked_command_encoder_for(
        CommandStage::ExposureMedia,
        &wgpu::CommandEncoderDescriptor::default(),
    );
    clear(&mut wrong, &target.view);
    assert_eq!(budget.info().unwrap().encoder_scopes, scopes);
    let primary = foreign.submit_observed(wrong.finish(), |_| {}).unwrap_err();
    assert!(primary.to_string().contains("another render queue"));
    assert_eq!(observer.snapshot().commands.counts.discarded, 1);
    assert_eq!(observer.snapshot().readback.map_requests, 0);

    let frame = budget
        .begin_frame_at(601, FrameOperation::Present, 7.5)
        .unwrap()
        .unwrap();
    let mut commands = renderer.tracked_command_encoder_for(
        CommandStage::ExposureMedia,
        &wgpu::CommandEncoderDescriptor::default(),
    );
    clear(&mut commands, &target.view);
    assert_eq!(budget.info().unwrap().encoder_scopes, scopes);
    renderer.submit_observed(commands.finish(), |_| {}).unwrap();
    let current = budget.frame_peaks().unwrap();
    assert_eq!(
        current
            .active
            .iter()
            .find(|frame| frame.request == 601)
            .unwrap()
            .submissions,
        1
    );
    frame.finish(crate::AllocationFrameOutcome::Rendered);
    poll(&renderer);
    let completed = observer.snapshot();
    assert_eq!(completed.commands.counts.completed, 1);
    let sample = completed.commands.recent.last().unwrap();
    assert_eq!(sample.stage, CommandStage::ExposureMedia);
    assert_eq!(sample.contexts[0].request, Some(601));
    assert!(sample.timestamp_words.is_some_and(|w| w[1] > w[0]));
    assert!(renderer
        .read_target(&target)
        .unwrap()
        .chunks_exact(4)
        .all(|p| p == [255, 0, 0, 255]));
    eprintln!(
        "GPU_COMMAND_RAW {}",
        serde_json::json!({
            "available":true,"actualRawCompletions":completed.commands.counts.completed,
            "addedAllocationEncoderScopes":0,"originalFrameSubmissionCount":1,"actualPixelsChecked":64,
            "scope":"actual raw wrapper/native clear; this does not claim YUV decoder path coverage"
        })
    );
}
