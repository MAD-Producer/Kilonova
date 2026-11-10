use super::*;
use crate::color_preparation::{self as prep, Observation};
#[path = "../../../motion-core/tests/support/color_project.rs"]
mod fixture;

struct Capture(Mutex<Vec<(log::Level, String)>>);
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if !text.starts_with("color preparation ") {
            return;
        }
        let slots: Vec<_> = sessions()
            .try_lock()
            .expect("registry unlocked")
            .values()
            .cloned()
            .collect();
        for slot in slots {
            assert!(
                !matches!(
                    slot.session.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ),
                "Session held during color logging"
            );
        }
        // A logger callback can re-enter the observation boundary: no RefCell
        // borrow is held across the logging sink either.
        drop(prep::observation::Scope::enter(-1));
        self.0.lock().unwrap().push((record.level(), text));
    }
    fn flush(&self) {
        let slots: Vec<_> = sessions()
            .try_lock()
            .expect("registry unlocked during flush")
            .values()
            .cloned()
            .collect();
        for slot in slots {
            assert!(
                !matches!(slot.session.try_lock(), Err(std::sync::TryLockError::WouldBlock)),
                "Session held during color log flush"
            );
        }
        drop(prep::observation::Scope::enter(-1));
    }
}

#[test]
fn preparation_records_release_owner_and_observation_locks_on_error_and_unwind() {
    const TEST: &str = "session::color_observation_tests::preparation_records_release_owner_and_observation_locks_on_error_and_unwind";
    const CHILD: &str = "MOTION_COLOR_LOCK_LOG_CHILD";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["error", "panic", "bounded", "healthy_capacity"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
                .env(CHILD, mode)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    };
    let capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let root = tempfile::tempdir().unwrap();
    let project = fixture::project();
    fixture::write_resources(root.path(), &project);
    let before = serde_json::to_value(&project).unwrap();
    let id = open(
        root.path().to_owned(),
        Project::new(64, 64, 30, 120).unwrap(),
        Arc::new(last_error_observation_tests::Stub),
    )
    .unwrap();
    let observation = Observation::new(9031);
    if mode == "healthy_capacity" {
        let unmanaged = Project::new(64, 64, 30, 120).unwrap();
        with_session(id, |_| {
            for _ in 0..80 {
                assert!(prep::prepare_project_colors(
                    &unmanaged,
                    root.path(),
                    |_| panic!("normal unmanaged request must not call provider"),
                    &observation
                )
                .unwrap()
                .is_none());
            }
            assert!(capture.0.lock().unwrap().is_empty());
            Ok(())
        })
        .unwrap();
        let records = capture.0.lock().unwrap();
        assert_eq!(records.len(), 64);
        assert!(records.iter().all(|(level, _)| *level == log::Level::Info));
        assert!(!observation.running());
        drop(records);
        close(id);
        return;
    }
    let operation = || {
        with_session(id, |_| {
            for _ in 0..if mode == "bounded" { 80 } else { 1 } {
                let error = prep::prepare_project_colors(
                    &project,
                    root.path(),
                    |_| {
                        assert!(observation.running(), "published before provider");
                        assert!(
                            capture.0.lock().unwrap().is_empty(),
                            "no logger under owner lock"
                        );
                        if mode == "panic" {
                            panic!("original provider unwind");
                        }
                        Err(format!("original provider cause {}", "界".repeat(3000)).into())
                    },
                    &observation,
                )
                .err()
                .unwrap();
                assert!(matches!(error, prep::Error::Provider { .. }));
                assert!(!observation.running());
            }
            Err::<(), _>("original caller error".into())
        })
    };
    if mode == "panic" {
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).unwrap_err();
        assert_eq!(
            panic.downcast_ref::<&str>(),
            Some(&"original provider unwind")
        );
    } else {
        assert_eq!(
            operation().unwrap_err().to_string(),
            "original caller error"
        );
    }
    assert!(!observation.running());
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    let records = capture.0.lock().unwrap().clone();
    assert!(records.iter().all(|(_, text)| text.len() < 2300));
    assert!(records
        .iter()
        .any(|(level, text)| *level == log::Level::Info
            && text.contains("request=9031")
            && text.contains("phase=start")));
    let errors: Vec<_> = records
        .iter()
        .filter(|(level, _)| *level == log::Level::Error)
        .collect();
    assert_eq!(errors.len(), if mode == "bounded" { 5 } else { 1 });
    if mode == "panic" {
        assert!(errors[0].1.contains("phase=abandoned"));
    } else {
        assert!(errors[0].1.contains("original provider cause"));
    }
    if mode == "bounded" {
        assert_eq!(records.len(), 69);
        assert!(errors
            .last()
            .unwrap()
            .1
            .contains("phase=observation_capacity"));
        assert!(errors.last().unwrap().1.contains("omitted_records=252"));
    }
    capture.0.lock().unwrap().clear();
    with_session(id, |_| Ok(())).unwrap();
    assert!(
        capture.0.lock().unwrap().is_empty(),
        "healthy boundary is quiet"
    );
    close(id);
}
