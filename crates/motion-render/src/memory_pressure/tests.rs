use super::*;
use std::cell::Cell;

fn available(budget: u64, usage: u64) -> MemoryPressureReading {
    MemoryPressureReading::Available {
        source: MemoryPressureSource::DxgiLocalSegment,
        pools: vec![MemoryPressurePool {
            index: 0,
            budget_bytes: budget,
            usage_bytes: usage,
        }],
    }
}

#[test]
fn native_sampling_is_bounded_and_expiry_does_not_reuse_a_stale_success() {
    let now = Instant::now();
    let calls = Cell::new(0);
    let mut probe = MemoryPressureProbe::default();
    let first = probe.sample(now, "Dx12", || {
        calls.set(calls.get() + 1);
        available(100, 80)
    });
    assert_eq!(first.reading, available(100, 80));
    for millis in 1..1000 {
        let cached = probe.sample(now + Duration::from_millis(millis), "Dx12", || {
            panic!("native query must not run for cached callers");
        });
        assert_eq!(cached.reading, first.reading);
        assert_eq!(cached.age_ms, millis);
    }
    assert_eq!(calls.get(), 1);
    let failed = probe.sample(now + QUERY_INTERVAL, "Dx12", || {
        calls.set(calls.get() + 1);
        MemoryPressureReading::failed("injected original native failure")
    });
    assert_eq!(calls.get(), 2);
    let wire = serde_json::to_value(&failed).unwrap();
    assert_eq!(wire["reading"]["status"], "unavailable");
    assert_eq!(wire["reading"]["reason"], "native_query_failed");
    assert_eq!(wire["reading"]["cause"], "injected original native failure");
    assert!(wire["reading"].get("pools").is_none());
    assert_eq!(wire["isPhysicalCapacity"], false);
    assert_eq!(wire["isFramePeak"], false);
    assert_eq!(wire["minimumQueryIntervalMs"], 1000);
    assert_eq!(probe.consecutive_query_failures, 1);
    let recovered = probe.sample(now + QUERY_INTERVAL * 2, "Dx12", || available(200, 70));
    assert_eq!(recovered.reading, available(200, 70));
    assert_eq!(probe.consecutive_query_failures, 0);
}

#[test]
fn failure_counter_counts_queries_saturates_and_resets_at_normal_boundaries() {
    let now = Instant::now();
    let mut probe = MemoryPressureProbe::default();
    for i in 0..120 {
        probe.sample(now + QUERY_INTERVAL * i, "Dx12", || {
            MemoryPressureReading::failed("native failure")
        });
        assert_eq!(probe.consecutive_query_failures, (i + 1).min(60) as u8);
    }
    probe.sample(now + QUERY_INTERVAL * 120, "Vulkan", || {
        MemoryPressureReading::unavailable(MemoryPressureUnavailable::MissingBudgetExtension)
    });
    assert_eq!(probe.consecutive_query_failures, 0);
    probe.sample(now + QUERY_INTERVAL * 121, "Dx12", || {
        MemoryPressureReading::failed("new episode")
    });
    assert_eq!(probe.consecutive_query_failures, 1);
}

#[test]
fn unknown_budget_is_not_zero_and_native_causes_have_a_character_bound() {
    let unknown = serde_json::to_value(MemoryPressureReading::unavailable(
        MemoryPressureUnavailable::UnsupportedBackend,
    ))
    .unwrap();
    assert_eq!(unknown["status"], "unavailable");
    assert!(unknown.get("pools").is_none());
    let MemoryPressureReading::Unavailable {
        cause: Some(cause), ..
    } = MemoryPressureReading::failed("界".repeat(MAX_CAUSE_CHARS * 4))
    else {
        panic!("missing cause")
    };
    assert_eq!(cause.chars().count(), MAX_CAUSE_CHARS);
    // A genuine zero native budget, and usage above it, remain real observations.
    let zero = serde_json::to_value(available(0, 80)).unwrap();
    assert_eq!(zero["status"], "available");
    assert_eq!(zero["pools"][0]["budgetBytes"], 0);
    assert_eq!(zero["pools"][0]["usageBytes"], 80);
}

#[test]
fn sustained_query_failures_log_once_at_sixty_and_recovery_allows_a_new_episode() {
    const CHILD: &str = "MOTION_TEST_MEMORY_PRESSURE_LOG";
    const TEST: &str = "memory_pressure::tests::sustained_query_failures_log_once_at_sixty_and_recovery_allows_a_new_episode";
    let Ok(level) = std::env::var(CHILD) else {
        for level in ["info", "debug"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, level)
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains("memory pressure query backend=")
            );
        }
        return;
    };
    struct Capture(std::sync::Mutex<Vec<(log::Level, String)>>);
    impl log::Log for Capture {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.target().ends_with("memory_pressure")
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                self.0
                    .lock()
                    .unwrap()
                    .push((record.level(), record.args().to_string()));
            }
        }
        fn flush(&self) {}
    }
    let capture = Box::leak(Box::new(Capture(std::sync::Mutex::new(Vec::new()))));
    log::set_logger(capture).unwrap();
    log::set_max_level(if level == "info" {
        log::LevelFilter::Info
    } else {
        log::LevelFilter::Debug
    });
    let now = Instant::now();
    let mut probe = MemoryPressureProbe::default();
    for i in 0..59 {
        probe.sample(now + QUERY_INTERVAL * i, "Dx12", || {
            MemoryPressureReading::failed("original cause")
        });
    }
    assert_eq!(
        capture
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(l, _)| *l == log::Level::Error)
            .count(),
        0
    );
    for i in 59..120 {
        probe.sample(now + QUERY_INTERVAL * i, "Dx12", || {
            MemoryPressureReading::failed("original cause")
        });
    }
    assert_eq!(
        capture
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(l, _)| *l == log::Level::Error)
            .count(),
        1
    );
    probe.sample(now + QUERY_INTERVAL * 120, "Dx12", || available(100, 20));
    for i in 121..181 {
        probe.sample(now + QUERY_INTERVAL * i, "Dx12", || {
            MemoryPressureReading::failed("recovered then failed")
        });
    }
    let events = capture.0.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|(l, _)| *l == log::Level::Error)
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|(l, _)| *l == log::Level::Debug)
            .count(),
        if level == "info" { 0 } else { 2 }
    );
    for (_, message) in events.iter() {
        assert!(
            message.contains("backend=Dx12 phase=native elapsed_ms="),
            "{message}"
        );
        assert!(message.contains("cause="), "{message}");
    }
}

#[test]
fn host_scopes_bound_evidence_and_emit_after_business_unlock_even_on_unwind() {
    use std::sync::{Arc, Mutex, TryLockError};
    const CHILD: &str = "MOTION_TEST_MEMORY_PRESSURE_SCOPE";
    const TEST: &str = "memory_pressure::tests::host_scopes_bound_evidence_and_emit_after_business_unlock_even_on_unwind";
    let Ok(level) = std::env::var(CHILD) else {
        for level in ["info", "debug"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, level)
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains("memory pressure query backend=")
            );
        }
        return;
    };
    struct Capture {
        business: Arc<Mutex<()>>,
        events: Mutex<Vec<(log::Level, String)>>,
    }
    impl log::Log for Capture {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.target().ends_with("memory_pressure")
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                assert!(
                    !matches!(self.business.try_lock(), Err(TryLockError::WouldBlock)),
                    "logger ran while the business guard was still held"
                );
                self.events
                    .lock()
                    .unwrap()
                    .push((record.level(), record.args().to_string()));
            }
        }
        fn flush(&self) {}
    }
    let business = Arc::new(Mutex::new(()));
    let capture = Box::leak(Box::new(Capture {
        business: business.clone(),
        events: Mutex::new(Vec::new()),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(if level == "info" {
        log::LevelFilter::Info
    } else {
        log::LevelFilter::Debug
    });
    let now = Instant::now();
    {
        let _scope = MemoryPressureLogScope::enter(42);
        let _guard = business.lock().unwrap();
        let mut first = MemoryPressureProbe::default();
        for i in 0..60 {
            first.sample(now + QUERY_INTERVAL * i, "Dx12", || {
                MemoryPressureReading::failed("primary native cause")
            });
        }
        {
            let _nested = MemoryPressureLogScope::enter(43);
            let mut second = MemoryPressureProbe::default();
            for i in 0..60 {
                second.sample(now + QUERY_INTERVAL * i, "Dx12", || {
                    MemoryPressureReading::failed("later native cause")
                });
            }
        }
        assert!(capture.events.lock().unwrap().is_empty());
    }
    {
        let events = capture.events.lock().unwrap();
        assert_eq!(events.len(), if level == "info" { 1 } else { 2 });
        for (_, message) in events.iter() {
            assert!(message.contains("session=Some(42)"), "{message}");
            assert!(
                message.contains("omitted=2 cause=primary native cause"),
                "{message}"
            );
        }
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _scope = MemoryPressureLogScope::enter(84);
        let _guard = business.lock().unwrap();
        observation::Event::record("Dx12", "unwind native cause", 60, Duration::ZERO);
        panic!("controlled session unwind");
    }));
    assert!(result.is_err());
    let events = capture.events.lock().unwrap();
    assert_eq!(events.len(), if level == "info" { 2 } else { 3 });
    let last = &events.last().unwrap().1;
    assert!(
        last.contains("session=Some(84)") && last.contains("cause=unwind native cause"),
        "{last}"
    );
}
