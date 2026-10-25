use super::*;
use std::sync::{Mutex, TryLockError};

struct Capture {
    business: Mutex<()>,
    records: Mutex<Vec<(log::Level, String)>>,
}
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if record.target() != "motion_render::pointwise_compile" {
            return;
        }
        assert!(!matches!(
            self.business.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        PENDING.with(|p| assert!(p.try_borrow_mut().is_ok()));
        assert!(SESSION.get().is_none());
        self.records
            .lock()
            .unwrap()
            .push((record.level(), record.args().to_string()));
    }
    fn flush(&self) {}
}

#[test]
fn compiler_observations_leave_business_guards_bound_bursts_and_preserve_unwind() {
    const CHILD: &str = "MOTION_POINTWISE_SCOPE_CHILD";
    const TEST: &str = "pointwise_observation::tests::compiler_observations_leave_business_guards_bound_bursts_and_preserve_unwind";
    let Some(mode) = std::env::var_os(CHILD) else {
        for mode in ["info", "debug", "panic", "overflow"] {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, mode)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{mode}: {}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        return;
    };
    let mode = mode.to_string_lossy();
    let capture = Box::leak(Box::new(Capture {
        business: Mutex::new(()),
        records: Mutex::new(vec![]),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(if mode == "debug" {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    });
    // A direct caller gets the signal before entering its potentially blocking
    // compiler. A scoped caller gets an explicitly deferred phase record.
    let direct = CompilePhase::start(
        "pointwise_compile",
        "direct",
        Some(47),
        [11, 12],
        None,
        None,
    );
    assert_eq!(capture.records.lock().unwrap().len(), 1);
    assert!(capture.records.lock().unwrap()[0].1.contains("phase=start"));
    direct.done(1234);
    assert_eq!(
        capture.records.lock().unwrap().len(),
        if mode == "debug" { 2 } else { 1 }
    );
    capture.records.lock().unwrap().clear();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(), &'static str> {
            let _outer = PointwiseLogScope::enter(41);
            let _business = capture.business.lock().unwrap();
            {
                let _nested = PointwiseLogScope::enter(42);
                if mode == "overflow" {
                    for _ in 0..MAX_EVENTS + 9 {
                        CompilePhase::start(
                            "pointwise_compile",
                            "bounded",
                            Some(47),
                            0..40,
                            None,
                            None,
                        )
                        .done(9);
                    }
                }
                let phase = CompilePhase::start(
                    "pointwise_compile",
                    &"k".repeat(300),
                    Some(47),
                    0..40,
                    None,
                    None,
                );
                phase.failed(
                    &("original compiler cause ".to_owned() + &"界".repeat(MAX_CAUSE_CHARS + 10)),
                );
                assert!(capture.records.lock().unwrap().is_empty());
                PENDING.with(|p| {
                    let p = p.borrow();
                    assert!(p.events.len() <= MAX_EVENTS);
                    for event in p.events.iter().chain(p.first_overflow_error.iter()) {
                        assert!(event.context.effects.len() <= 16);
                        assert!(event.context.structure.chars().count() <= 128);
                        assert!(event.cause.chars().count() <= MAX_CAUSE_CHARS);
                    }
                });
            }
            assert!(capture.records.lock().unwrap().is_empty());
            if mode == "panic" {
                panic!("original caller panic");
            }
            Err("original caller failure")
        },
    ));
    if mode == "panic" {
        assert_eq!(
            outcome.unwrap_err().downcast_ref::<&str>(),
            Some(&"original caller panic")
        );
    } else {
        assert_eq!(outcome.unwrap().unwrap_err(), "original caller failure");
    }
    let records = capture.records.lock().unwrap();
    let errors = records
        .iter()
        .filter(|(level, _)| *level == log::Level::Error)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].1.contains("cause=original compiler cause"));
    assert!(errors[0].1.contains("session=Some(42)"));
    assert!(errors[0].1.contains("deferred=true"));
    assert_eq!(
        records.iter().any(|(level, _)| *level == log::Level::Warn),
        mode == "overflow"
    );
    if mode != "overflow" {
        assert_eq!(
            records
                .iter()
                .filter(|(l, _)| *l == log::Level::Info)
                .count(),
            1
        );
    }
}
