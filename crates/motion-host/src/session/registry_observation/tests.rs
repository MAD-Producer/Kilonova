use super::*;
use crate::session::{close, open, sessions, with_session};
use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
};

struct Capture(Mutex<Vec<String>>);
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if !text.contains("site=host.registry.load") {
            return;
        }
        assert_eq!(record.level(), log::Level::Error);
        assert!(!matches!(
            sessions().try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        PENDING.with(|pending| assert!(pending.try_borrow_mut().is_ok()));
        self.0.lock().unwrap().push(text);
    }
    fn flush(&self) {}
}

#[test]
fn actual_recovery_is_quiet_when_healthy_and_emits_bounded_evidence_after_unlock() {
    const TEST: &str = "session::registry_observation::tests::actual_recovery_is_quiet_when_healthy_and_emits_bounded_evidence_after_unlock";
    const CHILD: &str = "MOTION_REGISTRY_RECOVERY_CHILD";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["return", "panic", "bounded", "long"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
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
            assert!(!String::from_utf8_lossy(&output.stdout).contains("site=host.registry.load"));
        }
        return;
    };
    let capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("plugins");
    let project_root = directory.path().join("project");
    std::fs::create_dir(&project_root).unwrap();
    let id = open(
        project_root,
        motion_core::Project::new(64, 64, 30, 120).unwrap(),
        Arc::new(crate::session::last_error_observation_tests::Stub),
    )
    .unwrap();
    assert!(capture.0.lock().unwrap().is_empty());
    let damaged = if mode == "long" {
        serde_json::to_vec(&"x".repeat(6000)).unwrap()
    } else {
        b"[[\"broken plugin settings".to_vec()
    };
    std::fs::write(path.join("disabled.json"), &damaged).unwrap();
    let operation = || {
        with_session(id, |_| {
            for _ in 0..if mode == "bounded" { 6 } else { 1 } {
                let recovered = load(&path).unwrap();
                assert_eq!(recovered.diagnostics.len(), 1);
                if mode == "long" {
                    assert!(recovered.diagnostics[0].len() > 6000);
                }
                assert!(!recovered.packages.is_empty());
                assert!(
                    capture.0.lock().unwrap().is_empty(),
                    "logged under session lock"
                );
            }
            if mode == "panic" {
                panic!("primary panic preserved");
            }
            Err::<(), _>("primary failure preserved".into())
        })
    };
    if mode == "panic" {
        let error = std::panic::catch_unwind(AssertUnwindSafe(operation)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<&str>(),
            Some(&"primary panic preserved")
        );
    } else {
        assert_eq!(operation().unwrap_err().to_string(), "primary failure preserved");
    }
    let records = capture.0.lock().unwrap().clone();
    assert_eq!(records.len(), if mode == "bounded" { 4 } else { 1 });
    for record in &records {
        assert!(record.contains(&format!("session=Some({id})")));
        assert!(record.contains("disabled.json"));
        if mode == "long" {
            assert!(record.contains("invalid type") && !record.contains(&"x".repeat(6000)));
            assert!(record.len() < 4096);
        } else {
            assert!(record.contains("file was preserved"));
        }
        assert!(record.contains("operation=registry_load") && record.contains("elapsed_ms="));
    }
    assert!(records.last().unwrap().contains(if mode == "bounded" {
        "omitted_loads=2"
    } else {
        "omitted_loads=0"
    }));
    assert_eq!(std::fs::read(path.join("disabled.json")).unwrap(), damaged);
    capture.0.lock().unwrap().clear();
    std::fs::write(path.join("disabled.json"), b"[]").unwrap();
    assert!(load(&path).unwrap().diagnostics.is_empty());
    assert!(capture.0.lock().unwrap().is_empty());
    close(id);
}
