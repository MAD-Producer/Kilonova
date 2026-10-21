use super::*;

struct Capture {
    uploads: &'static AllocationUploads,
    events: Mutex<Vec<(log::Level, String)>>,
}
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if text.starts_with("render upload reservations poisoned ") {
            assert!(!matches!(
                self.uploads.pending.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ));
            self.events.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}

#[test]
fn poisoned_upload_retirement_preserves_known_leases_and_logs_once_after_unlock() {
    const CHILD: &str = "MOTION_TEST_UPLOAD_RESERVATION_POISON";
    const TEST: &str = "allocation_uploads::tests::poisoned_upload_retirement_preserves_known_leases_and_logs_once_after_unlock";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, "1")
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout)
            .contains("render upload reservations poisoned "));
        return;
    }
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let budget = crate::AllocationBudget::new(8).unwrap();
    let lease = budget.reserve(4).unwrap();
    let uploads = Box::leak(Box::new(AllocationUploads::new(renderer.queue.clone())));
    for _ in 0..120 {
        uploads.track(Some(lease.clone())).unwrap();
    }
    assert_eq!(uploads.pending.lock().unwrap().len(), 1);
    drop(lease);
    let capture = Box::leak(Box::new(Capture {
        uploads,
        events: Mutex::new(Vec::new()),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _state = uploads.pending.lock().unwrap();
        panic!("controlled pending upload state poison");
    }))
    .is_err());
    // The primary fault refuses future writes; cleanup can still retire the
    // known destination on the actual queue instead of silently erasing it.
    let error = uploads.track(Some(budget.reserve(4).unwrap())).unwrap_err();
    assert!(error
        .to_string()
        .contains("render upload reservations are poisoned"));
    assert_eq!(budget.info().unwrap().admitted_bytes, 4);
    renderer.queue.submit([]);
    uploads.submitted();
    for _ in 0..120 {
        uploads.submitted();
    }
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
    let events = capture.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, log::Level::Error);
    assert!(events[0].1.contains("phase=retire retained_destinations=1"));
}
