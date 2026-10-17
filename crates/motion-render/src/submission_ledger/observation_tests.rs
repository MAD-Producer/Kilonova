use super::*;
use std::sync::TryLockError;

struct Capture {
    state: Arc<Mutex<State>>,
    events: Mutex<Vec<(log::Level, String)>>,
}
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if text.starts_with("submission ledger poison recovery") {
            assert!(
                !matches!(self.state.try_lock(), Err(TryLockError::WouldBlock)),
                "logger called with submission ledger still locked"
            );
            self.events.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}
fn poison(state: &Mutex<State>) {
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _state = state.lock().unwrap();
        panic!("controlled submission ledger poison");
    }))
    .is_err());
}

#[test]
fn poisoned_submission_sites_are_bounded_visible_and_emit_after_unlock() {
    const TEST: &str = "submission_ledger::observation_tests::poisoned_submission_sites_are_bounded_visible_and_emit_after_unlock";
    const CHILD: &str = "MOTION_TEST_SUBMISSION_RECOVERY";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["threshold", "unwind", "gpu"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, mode)
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!String::from_utf8_lossy(&output.stdout)
                .contains("submission ledger poison recovery"));
        }
        return;
    };
    let tracker = SubmissionLedger::default();
    tracker.set_enabled(true);
    let capture = Box::leak(Box::new(Capture {
        state: tracker.state.clone(),
        events: Mutex::new(Vec::new()),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let cycle = || {
        let ticket = tracker.begin(Default::default());
        let pending = tracker.info(&Default::default());
        assert_eq!(pending.pending_submissions, 1);
        SubmissionLedger::complete(&tracker.state, ticket);
    };
    for _ in 0..120 {
        cycle();
    }
    assert!(capture.events.lock().unwrap().is_empty());
    poison(&tracker.state);
    if mode == "unwind" {
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut recovery = Recovery::new(Site::Begin, Some(121));
            let _state = recovery.lock(tracker.state.lock());
            panic!("controlled unwind while publishing ledger state");
        }))
        .is_err());
        let events = capture.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, log::Level::Warn);
        assert!(events[0]
            .1
            .contains("site=submission.begin lifetime_count=1 submission=Some(121)"));
        assert!(tracker.state.is_poisoned());
        return;
    }
    if mode == "gpu" {
        let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
        let target = renderer.render_target(16, 16).unwrap();
        let mut owned = TextureResourceLedger::default();
        target.collect_texture_resources(&mut owned, "test/target", crate::TextureState::Retained);
        let encoder = renderer.device.create_command_encoder(&Default::default());
        tracker.submit(&renderer.queue, encoder.finish(), owned.clone());
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
        let info = tracker.info(&owned);
        assert_eq!(info.submitted, 121);
        assert_eq!(info.completed, 121);
        assert_eq!(info.pending_submissions, 0);
        assert_eq!(info.known_pending_bytes, 0);
        assert_eq!(info.peak_known_pending_bytes, owned.total_known_bytes());
        renderer.check_health().unwrap();
        let events = capture.events.lock().unwrap();
        for site in ["submission.begin", "submission.complete", "submission.info"] {
            assert_eq!(
                events
                    .iter()
                    .filter(|(level, text)| *level == log::Level::Warn
                        && text.contains(&format!("site={site} lifetime_count=1 ")))
                    .count(),
                1
            );
        }
        assert_eq!(events.len(), 3);
        return;
    }
    let healthy = SubmissionLedger::default();
    for _ in 0..120 {
        cycle();
        assert_eq!(healthy.info(&Default::default()).pending_submissions, 0);
        tracker.set_enabled(false);
        tracker.set_enabled(true);
    }
    tracker.state.clear_poison();
    cycle();
    poison(&tracker.state);
    cycle();
    assert!(tracker.state.is_poisoned());
    let events = capture.events.lock().unwrap();
    assert_eq!(events.len(), 6);
    for site in ["submission.begin", "submission.complete", "submission.info"] {
        let records = events
            .iter()
            .filter(|(_, text)| text.contains(&format!("site={site} ")))
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, log::Level::Warn);
        assert_eq!(records[1].0, log::Level::Error);
        assert!(records[0].1.contains("lifetime_count=1 "));
        assert!(records[1].1.contains("lifetime_count=60 "));
        for (_, text) in records {
            for field in [
                "recovery_elapsed_ms=",
                "submitted_before=",
                "completed_before=",
                "pending_tracked_before=",
                "pending_untracked_before=",
                "known_bytes_before=",
                "skipped_before=",
                "reason=poisoned_mutex recovery=into_inner",
            ] {
                assert!(text.contains(field), "{text}");
            }
        }
    }
    drop(events);
    let info = tracker.info(&Default::default());
    assert_eq!(info.submitted, 242);
    assert_eq!(info.completed, 242);
    assert_eq!(info.pending_submissions, 0);
}
