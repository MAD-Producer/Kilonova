use super::*;
use std::sync::atomic::{AtomicI64, Ordering};

fn high() -> motion_render::MemoryPressureSnapshot {
    let mut sample = motion_render::MemoryPressureSnapshot::without_renderer();
    sample.reading = motion_render::MemoryPressureReading::Available {
        source: motion_render::MemoryPressureSource::VulkanDeviceLocalHeaps,
        pools: vec![motion_render::MemoryPressurePool {
            index: 3,
            budget_bytes: 100,
            usage_bytes: 95,
        }],
    };
    sample
}

static SESSION_ID: AtomicI64 = AtomicI64::new(0);
struct Capture(Mutex<Vec<(log::Level, String)>>);
static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if !text.contains("site=host.preview_pressure") {
            return;
        }
        let slot = {
            let registry = sessions()
                .try_lock()
                .expect("registry must be unlocked at the pressure sink");
            registry
                .get(&SESSION_ID.load(Ordering::Relaxed))
                .unwrap()
                .clone()
        };
        assert!(
            !matches!(
                slot.session.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ),
            "Session must be unlocked, including unwind, before pressure logging"
        );
        PENDING.with(|pending| {
            assert!(
                pending.try_borrow_mut().is_ok(),
                "observation borrow held during logging"
            )
        });
        self.0.lock().unwrap().push((record.level(), text));
    }
    fn flush(&self) {}
}

#[test]
fn default_pressure_events_are_bounded_reset_and_leave_session_locks_before_logging() {
    const NAME: &str = "session::preview_pressure::tests::default_pressure_events_are_bounded_reset_and_leave_session_locks_before_logging";
    const CHILD: &str = "MOTION_PREVIEW_PRESSURE_SCOPE_CHILD";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["return", "panic"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
                .env(CHILD, mode)
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    };
    log::set_logger(&CAPTURE).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let root = tempfile::tempdir().unwrap();
    let id = open(
        root.path().to_owned(),
        Project::new(32, 24, 30, 6).unwrap(),
        Arc::new(crate::session::last_error_observation_tests::Stub),
    )
    .unwrap();
    SESSION_ID.store(id, Ordering::Relaxed);
    for episode in 0..2 {
        for index in 0..120 {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let result: Result<()> = with_session(id, |session| {
                    let now = Duration::from_secs(1000 + episode * 200 + index);
                    let update = session.preview_memory.observe_pressure(now, high);
                    let info = session.preview_memory.pressure_info(now);
                    record(session.frame, info, update, Instant::now());
                    if episode == 0 && index == 59 {
                        if mode == "panic" {
                            panic!("controlled pressure caller panic");
                        }
                        return Err("primary preview operation failure".into());
                    }
                    Ok(())
                });
                result
            }));
            if episode == 0 && index == 59 {
                if mode == "panic" {
                    assert!(result.is_err());
                } else {
                    assert_eq!(
                        result.unwrap().unwrap_err().to_string(),
                        "primary preview operation failure"
                    );
                }
            } else {
                result.unwrap().unwrap();
            }
        }
        with_session(id, |session| {
            let now = Duration::from_secs(1199 + episode * 200);
            let update = session
                .preview_memory
                .observe_pressure(now, motion_render::MemoryPressureSnapshot::without_renderer);
            record(
                session.frame,
                session.preview_memory.pressure_info(now),
                update,
                Instant::now(),
            );
            Ok(())
        })
        .unwrap();
    }
    SESSION.with(|session| assert_eq!(session.get(), None));
    let events = CAPTURE.0.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|(level, _)| *level == log::Level::Warn)
            .count(),
        3
    );
    let errors: Vec<_> = events
        .iter()
        .filter(|(level, _)| *level == log::Level::Error)
        .collect();
    assert_eq!(errors.len(), 2);
    assert!(!events.iter().any(|(level, _)| *level == log::Level::Debug));
    for (_, text) in errors {
        for context in [
            "operation=preview frame=0 phase=sustained",
            "elapsed_ms=",
            "count=60",
            "pool=Some",
            "requested_floor=3",
            "deferred_omitted=0",
        ] {
            assert!(text.contains(context), "missing {context}: {text}");
        }
    }
    drop(events);
    close(id);
}

#[cfg(windows)]
mod native;
