use super::*;
use std::sync::TryLockError;

struct Capture {
    failure: Arc<Mutex<Option<String>>>,
    events: Mutex<Vec<(log::Level, String)>>,
}
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if text.starts_with("GPU failure latch poison recovery") {
            assert!(
                !matches!(self.failure.try_lock(), Err(TryLockError::WouldBlock)),
                "latch still locked during logging"
            );
            self.events.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}
fn poison(failure: &Mutex<Option<String>>) {
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = failure.lock().unwrap();
        panic!("controlled GPU latch poison");
    }))
    .is_err());
}
fn value(failure: &Mutex<Option<String>>) -> Option<String> {
    failure.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn poisoned_gpu_latch_recovery_is_visible_bounded_and_keeps_health_semantics() {
    const TEST: &str = "renderer::gpu_failure::recovery_tests::poisoned_gpu_latch_recovery_is_visible_bounded_and_keeps_health_semantics";
    const CHILD: &str = "MOTION_GPU_LATCH_RECOVERY_TEST";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["threshold", "unwind", "none"] {
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
                .contains("GPU failure latch poison recovery"));
        }
        return;
    };
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let failure = renderer.gpu_failure.clone();
    let observer = Observation::new(failure.clone(), renderer.adapter.get_info());
    let capture = Box::leak(Box::new(Capture {
        failure: failure.clone(),
        events: Mutex::new(Vec::new()),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    for _ in 0..120 {
        assert!(renderer.gpu_error().is_none());
        renderer.check_health().unwrap();
    }
    assert!(capture.events.lock().unwrap().is_empty());
    if mode == "none" {
        poison(&failure);
        for _ in 0..120 {
            assert!(renderer.gpu_error().is_none());
        }
        // Observing poison does not invent a GPU failure or change the public result.
        renderer.check_health().unwrap();
        let events = capture.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, log::Level::Warn);
        assert_eq!(events[1].0, log::Level::Error);
        assert!(events[0].1.contains("lifetime_count=1 "));
        assert!(events[1].1.contains("lifetime_count=60 "));
        assert!(events
            .iter()
            .all(|(_, s)| s.contains("cause_present_before=false cause_bytes_before=0")));
        assert!(failure.is_poisoned());
        return;
    }
    let original = "confidential-original-敏".repeat(4096);
    *failure.lock().unwrap() = Some(original);
    poison(&failure);
    if mode == "unwind" {
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut recovery = Recovery::new(Site::Read, None);
            let _guard = recovery.lock(failure.lock());
            panic!("controlled recovery unwind");
        }))
        .is_err());
        let events = capture.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, log::Level::Warn);
        assert!(events[0]
            .1
            .contains("site=gpu_failure.read lifetime_count=1 renderer=None"));
        assert!(!events[0].1.contains("confidential-original"));
        assert!(failure.is_poisoned());
        return;
    }
    let cycle = || {
        let before = value(&failure);
        observer.uncaptured(wgpu::Error::Validation {
            source: Box::new(std::io::Error::other("controlled callback source")),
            description: "controlled callback validation".into(),
        });
        assert_eq!(value(&failure), before);
        assert_eq!(renderer.gpu_error(), before);
        observer.device_lost(
            wgpu::DeviceLostReason::Destroyed,
            "controlled latch replacement".into(),
        );
        assert_eq!(
            value(&failure).as_deref(),
            Some("GPU device lost: Destroyed: controlled latch replacement")
        );
    };
    let healthy = Mutex::new(None);
    for _ in 0..120 {
        cycle();
        assert!(read(&healthy).is_none());
    }
    failure.clear_poison();
    cycle();
    poison(&failure);
    cycle();
    assert!(failure.is_poisoned());
    let events = capture.events.lock().unwrap();
    assert_eq!(events.len(), 6);
    for site in [
        "gpu_failure.uncaptured",
        "gpu_failure.device_lost",
        "gpu_failure.read",
    ] {
        let records: Vec<_> = events
            .iter()
            .filter(|(_, s)| s.contains(&format!("site={site} ")))
            .collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, log::Level::Warn);
        assert_eq!(records[1].0, log::Level::Error);
        assert!(records[0].1.contains("lifetime_count=1 "));
        assert!(records[1].1.contains("lifetime_count=60 "));
        for (_, text) in records {
            assert!(text.len() < 400);
            assert!(
                text.contains("recovery_elapsed_ms=")
                    && text.contains("cause_present_before=true cause_bytes_before=")
            );
            assert!(text.contains("reason=poisoned_mutex recovery=into_inner"));
            assert!(
                !text.contains("confidential-original")
                    && !text.contains("controlled latch replacement")
            );
            assert!(text.contains(if site == "gpu_failure.read" {
                "renderer=None"
            } else {
                "renderer=Some("
            }));
        }
    }
    drop(events);
    assert!(renderer
        .check_health()
        .unwrap_err()
        .to_string()
        .contains("controlled latch replacement"));
}
