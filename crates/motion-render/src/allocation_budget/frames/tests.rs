use super::*;

fn begin(budget: &AllocationBudget, request: u64) -> AllocationFrame {
    budget
        .begin_frame(request, FrameOperation::Render)
        .unwrap()
        .unwrap()
}

#[test]
fn authored_frame_context_is_finite_and_optional_and_cancellation_is_distinct() {
    let budget = AllocationBudget::new(64).unwrap();
    for frame in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(budget
            .begin_frame_at(19, FrameOperation::Export, frame)
            .is_err());
    }
    let job = budget
        .begin_frame(19, FrameOperation::ExportJob)
        .unwrap()
        .unwrap();
    let frame = budget
        .begin_frame_at(19, FrameOperation::Export, 1.25)
        .unwrap()
        .unwrap();
    let active = budget.frame_peaks().unwrap();
    assert_eq!(active.version, 2);
    assert_eq!(active.active.len(), 2);
    assert_eq!(active.active[0].source_frame, None);
    assert_eq!(active.active[1].source_frame, Some(1.25));
    assert_eq!(active.omitted_frames, 0);
    frame.finish(AllocationFrameOutcome::Cancelled);
    job.finish(AllocationFrameOutcome::Failed);
    let wire = serde_json::to_value(budget.frame_peaks().unwrap()).unwrap();
    assert_eq!(wire["recent"][0]["sourceFrame"], 1.25);
    assert_eq!(wire["recent"][0]["outcome"], "cancelled");
    assert_eq!(wire["recent"][1]["operation"], "export_job");
    assert_eq!(wire["recent"][1]["sourceFrame"], serde_json::Value::Null);
}

#[test]
fn windows_capture_transient_shared_capacity_without_inheriting_lifetime_peak() {
    let budget = AllocationBudget::new(64).unwrap();
    let original = budget.reserve(64).unwrap();
    drop(original);
    let front = budget.reserve(8).unwrap();
    let first = begin(&budget, 1);
    let second = begin(&budget.clone(), 2);
    let candidate = budget.reserve(16).unwrap();
    let alias = candidate.clone();
    assert!(budget.reserve(64).is_err());
    drop((candidate, alias));
    first.finish(AllocationFrameOutcome::Rendered);
    second.finish(AllocationFrameOutcome::Skipped);
    drop(front);
    let third = begin(&budget, 3);
    let tiny = budget.reserve(4).unwrap();
    drop(tiny);
    third.finish(AllocationFrameOutcome::Rendered);
    let snapshot = budget.frame_peaks().unwrap();
    assert_eq!(budget.info().unwrap().peak_admitted_bytes, 64);
    assert_eq!(
        snapshot
            .recent
            .iter()
            .map(|frame| frame.peak_admitted_bytes)
            .collect::<Vec<_>>(),
        [24, 24, 4]
    );
    assert_eq!(snapshot.recent[0].start_admitted_bytes, 8);
    assert_eq!(snapshot.recent[0].end_admitted_bytes, Some(8));
    assert_eq!(snapshot.recent[2].end_admitted_bytes, Some(0));
    assert!(!snapshot.complete_coverage && !snapshot.is_physical_vram && !snapshot.tracking_fault);
}

#[test]
fn cpu_and_all_its_queue_completions_join_without_renewal_by_later_submissions() {
    let budget = AllocationBudget::new(64).unwrap();
    let frame = begin(&budget, 7);
    let first = budget.frame_submission();
    let second = budget.frame_submission();
    first.complete();
    frame.finish(AllocationFrameOutcome::Rendered);
    let snapshot = budget.frame_peaks().unwrap();
    assert!(snapshot.recent.is_empty());
    assert_eq!(snapshot.active[0].pending_submissions, 1);
    assert_eq!(snapshot.active[0].submissions, 2);
    assert_eq!(
        snapshot.active[0].outcome,
        Some(AllocationFrameOutcome::Rendered)
    );
    let unrelated = budget.frame_submission();
    assert!(!unrelated.is_tracked());
    let transient = budget.reserve(32).unwrap();
    drop(transient);
    second.complete();
    let snapshot = budget.frame_peaks().unwrap();
    assert!(snapshot.active.is_empty());
    assert_eq!(snapshot.recent[0].peak_admitted_bytes, 32);
    assert_eq!(snapshot.recent[0].end_admitted_bytes, Some(0));
    assert_eq!(snapshot.recent[0].unconfirmed_submissions, 0);
    assert!(snapshot.recent[0].elapsed_us >= snapshot.recent[0].cpu_elapsed_us.unwrap());
    drop(unrelated);
    assert_eq!(budget.frame_peaks().unwrap().unconfirmed_submissions, 0);
}

#[test]
fn active_and_recent_metadata_are_bounded_and_saturation_recovers() {
    let budget = AllocationBudget::new(64).unwrap();
    let frames: Vec<_> = (0..MAX_ACTIVE_ALLOCATION_FRAMES)
        .map(|id| begin(&budget, id as u64))
        .collect();
    for _ in 0..120 {
        assert!(budget
            .begin_frame(999, FrameOperation::Export)
            .unwrap()
            .is_none());
    }
    assert_eq!(budget.frame_peaks().unwrap().omitted_frames, 120);
    // Saturated observation has no effect on native admission.
    let lease = budget.reserve(64).unwrap();
    drop(lease);
    drop(frames);
    for id in 0..(MAX_RECENT_ALLOCATION_FRAMES + 10) {
        begin(&budget, id as u64).finish(AllocationFrameOutcome::Rendered);
    }
    let snapshot = budget.frame_peaks().unwrap();
    assert!(snapshot.active.is_empty());
    assert_eq!(snapshot.recent.len(), MAX_RECENT_ALLOCATION_FRAMES);
    assert_eq!(snapshot.recent[0].request, 10);
    assert_eq!(snapshot.omitted_frames, 120);
}

#[test]
fn unwind_and_dropped_callbacks_remain_distinct_from_confirmed_success() {
    let budget = AllocationBudget::new(64).unwrap();
    assert!(std::panic::catch_unwind(|| {
        let _frame = begin(&budget, 1);
        let _ticket = budget.frame_submission();
        panic!("controlled frame panic");
    })
    .is_err());
    let abandoned = begin(&budget, 2);
    drop(abandoned);
    let snapshot = budget.frame_peaks().unwrap();
    assert!(snapshot.active.is_empty());
    assert_eq!(
        snapshot.recent[0].outcome,
        Some(AllocationFrameOutcome::Panicked)
    );
    assert_eq!(snapshot.recent[0].unconfirmed_submissions, 1);
    assert_eq!(
        snapshot.recent[1].outcome,
        Some(AllocationFrameOutcome::Abandoned)
    );
    assert_eq!(snapshot.unconfirmed_submissions, 1);
}

#[test]
fn blocked_work_is_published_and_readable_without_entering_the_handler() {
    let budget = AllocationBudget::new(64).unwrap();
    let (entered, waiting) = std::sync::mpsc::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let worker = {
        let budget = budget.clone();
        std::thread::spawn(move || {
            let frame = begin(&budget, 15);
            let _live = budget.reserve(16).unwrap();
            entered.send(()).unwrap();
            blocked
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            frame.finish(AllocationFrameOutcome::Rendered);
        })
    };
    waiting
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let snapshot = budget.frame_peaks().unwrap();
    release.send(()).unwrap();
    worker.join().unwrap();
    assert_eq!(snapshot.active.len(), 1);
    assert_eq!(snapshot.active[0].request, 15);
    assert_eq!(snapshot.active[0].peak_admitted_bytes, 16);
    assert!(snapshot.active[0].outcome.is_none());
    assert!(budget.frame_peaks().unwrap().active.is_empty());
}

#[test]
fn poisoned_observation_never_returns_a_fabricated_healthy_zero() {
    let budget = AllocationBudget::new(64).unwrap();
    let frame = begin(&budget, 1);
    let ticket = budget.frame_submission();
    assert!(std::panic::catch_unwind(|| {
        let _held = budget.0.lock().unwrap();
        panic!("controlled observation poison");
    })
    .is_err());
    // Cleanup is non-panicking and performs no logging under a business lock.
    let business = Mutex::new(());
    let _held = business.lock().unwrap();
    frame.finish(AllocationFrameOutcome::Failed);
    ticket.complete();
    assert!(budget.frame_peaks().is_err());
    assert!(budget.begin_frame(2, FrameOperation::Render).is_err());
    let state = match budget.0.lock() {
        Err(error) => error.into_inner(),
        Ok(_) => panic!("expected poisoned observation state"),
    };
    assert!(state.frames.fault);
    assert!(state.frames.active.is_empty());
}
