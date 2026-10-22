use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Condvar,
};
use std::time::Duration;

fn wait_finished(workers: &Workers) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if workers
            .handles
            .lock()
            .unwrap()
            .iter()
            .all(|worker| worker.handle.is_finished())
        {
            return;
        }
        assert!(Instant::now() < deadline, "worker did not finish");
        std::thread::yield_now();
    }
}

#[test]
fn completed_workers_are_reaped_without_exhausting_the_handle_budget() {
    let workers = Workers::default();
    let completed = Arc::new(AtomicUsize::new(0));
    for index in 0..MAX_RETAINED_WORKERS * 2 + 1 {
        workers.reap().unwrap();
        let count = completed.clone();
        workers
            .spawn(
                "media-reap-test",
                TaskLog::new(&index.to_string(), "reap_test"),
                move || {
                    count.fetch_add(1, Ordering::AcqRel);
                },
            )
            .unwrap();
        wait_finished(&workers);
    }
    workers.reap().unwrap();
    assert!(workers.handles.lock().unwrap().is_empty());
    assert_eq!(
        completed.load(Ordering::Acquire),
        MAX_RETAINED_WORKERS * 2 + 1
    );
}

#[test]
fn retained_native_handles_are_bounded_and_rejection_does_not_run_work() {
    let mut workers = Workers::default();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    struct Release(Arc<(Mutex<bool>, Condvar)>);
    impl Drop for Release {
        fn drop(&mut self) {
            *self.0 .0.lock().unwrap() = true;
            self.0 .1.notify_all();
        }
    }
    // Release blocked native threads before Workers::Drop on any assertion.
    let _release_on_unwind = Release(gate.clone());
    let completed = Arc::new(AtomicUsize::new(0));
    for index in 0..MAX_RETAINED_WORKERS {
        let gate = gate.clone();
        let count = completed.clone();
        workers
            .spawn(
                "media-budget-test",
                TaskLog::new(&index.to_string(), "budget_test"),
                move || {
                    let (lock, wake) = &*gate;
                    let mut released = lock.lock().unwrap();
                    while !*released {
                        released = wake
                            .wait_timeout(released, Duration::from_secs(5))
                            .unwrap()
                            .0;
                    }
                    count.fetch_add(1, Ordering::AcqRel);
                },
            )
            .unwrap();
    }
    let rejected = Arc::new(AtomicUsize::new(0));
    let attempted = rejected.clone();
    let error = workers
        .spawn(
            "media-rejected-test",
            TaskLog::new("overflow", "budget_test"),
            move || {
                attempted.fetch_add(1, Ordering::AcqRel);
            },
        )
        .unwrap_err();
    let (lock, wake) = &*gate;
    *lock.lock().unwrap() = true;
    wake.notify_all();
    workers.shutdown();
    assert_eq!(error.to_string(), "media worker handle budget exhausted");
    assert_eq!(completed.load(Ordering::Acquire), MAX_RETAINED_WORKERS);
    assert_eq!(rejected.load(Ordering::Acquire), 0);
    assert!(workers.handles.get_mut().unwrap().is_empty());
}

#[test]
fn reap_preserves_uncaught_worker_panic_and_then_allows_healthy_work() {
    let workers = Workers::default();
    workers
        .spawn(
            "media-uncaught-test",
            TaskLog::new("uncaught", "panic_test"),
            || {
                panic!("original panic outside media handler");
            },
        )
        .unwrap();
    wait_finished(&workers);
    let error = workers.reap().unwrap_err();
    assert!(matches!(
        error,
        crate::Error::Panic {
            operation: "media worker join",
            ..
        }
    ));
    assert!(error
        .to_string()
        .contains("original panic outside media handler"));
    assert!(workers.handles.lock().unwrap().is_empty());
    workers
        .spawn(
            "media-after-panic",
            TaskLog::new("healthy", "panic_test"),
            || {},
        )
        .unwrap();
    wait_finished(&workers);
    workers.reap().unwrap();
}

#[test]
fn poisoned_worker_registry_rejects_spawn_and_shutdown_still_releases_handles() {
    let mut workers = Workers::default();
    workers
        .spawn(
            "media-poison-test",
            TaskLog::new("before-poison", "poison_test"),
            || {},
        )
        .unwrap();
    wait_finished(&workers);
    std::thread::scope(|scope| {
        assert!(scope
            .spawn(|| {
                let _guard = workers.handles.lock().unwrap();
                panic!("controlled worker handle poison");
            })
            .join()
            .is_err());
    });
    let error = workers
        .spawn(
            "media-poison-rejected",
            TaskLog::new("after-poison", "poison_test"),
            || {
                panic!("poisoned spawn must not execute");
            },
        )
        .unwrap_err();
    assert_eq!(error.to_string(), "media worker registry poisoned");
    assert_eq!(
        workers.reap().unwrap_err().to_string(),
        "media worker registry poisoned"
    );
    workers.shutdown();
    match workers.handles.get_mut() {
        Ok(_) => panic!("shutdown must not clear the original poison"),
        Err(error) => assert!(error.into_inner().is_empty()),
    }
}

#[test]
fn shutdown_is_idempotent_rejects_new_work_and_logs_poison_once_per_instance() {
    const TEST: &str =
        "workers::tests::shutdown_is_idempotent_rejects_new_work_and_logs_poison_once_per_instance";
    const CHILD: &str = "MOTION_WORKER_SHUTDOWN_POISON_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    struct Capture(AtomicUsize);
    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            if record
                .args()
                .to_string()
                .contains("media worker shutdown operation=join phase=registry")
            {
                assert_eq!(record.level(), log::Level::Error);
                self.0.fetch_add(1, Ordering::AcqRel);
            }
        }
        fn flush(&self) {}
    }
    let capture = Box::leak(Box::new(Capture(AtomicUsize::new(0))));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let mut workers = Workers::default();
    std::thread::scope(|scope| {
        assert!(scope
            .spawn(|| {
                let _guard = workers.handles.lock().unwrap();
                panic!("controlled idempotent shutdown poison");
            })
            .join()
            .is_err());
    });
    workers.shutdown();
    workers.shutdown();
    let executed = Arc::new(AtomicUsize::new(0));
    let attempted = executed.clone();
    let error = workers
        .spawn(
            "media-after-shutdown",
            TaskLog::new("closed", "shutdown_test"),
            move || {
                attempted.fetch_add(1, Ordering::AcqRel);
            },
        )
        .unwrap_err();
    assert_eq!(error.to_string(), "media workers already shut down");
    assert_eq!(
        workers.reap().unwrap_err().to_string(),
        "media workers already shut down"
    );
    drop(workers);
    assert_eq!(capture.0.load(Ordering::Acquire), 1);
    assert_eq!(executed.load(Ordering::Acquire), 0);
}
