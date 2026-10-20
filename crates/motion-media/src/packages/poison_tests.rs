use super::*;
use crate::poison_observation::{tests::*, RecoveryScope};
use std::time::Duration;

fn fixture() -> Arc<Mutex<Task>> {
    Arc::new(Mutex::new(Task {
        status: PackageTaskStatus {
            schema_version: 1,
            request_id: "poison-package".into(),
            operation: "export_project".into(),
            state: "running".into(),
            phase: "queued".into(),
            progress: 0.,
            bytes_processed: 0,
            total_bytes: 0,
            source_count: 0,
            frozen_revision: 0,
            path: None,
            error: None,
        },
        cancel: false,
        completed: Arc::new(Condvar::new()),
    }))
}

#[test]
fn package_admission_spawn_failure_and_drop_emit_after_outer_registry_unlocks() {
    let Some(mode) = case("packages::poison_tests::package_admission_spawn_failure_and_drop_emit_after_outer_registry_unlocks",
        &["active", "spawn", "spawn_long", "drop"]) else { return; };
    let jobs = Box::leak(Box::new(PackageJobs::default()));
    let task = fixture();
    jobs.tasks
        .lock()
        .unwrap()
        .insert("poison-package".into(), task.clone());
    let target = task.clone();
    // Keep the registry available to the logger even in the Drop case, which
    // moves the map to an owned session before destruction.
    let registry = &jobs.tasks;
    let capture = Capture::install(move |_| {
        unlocked(registry);
        unlocked(&target);
    });
    poison(&task);
    match mode.as_str() {
        "active" => {
            let root = tempfile::tempdir().unwrap();
            let project = motion_model::Project::new(32, 32, 30, 30).unwrap();
            let snapshot =
                motion_core::storage::PackageSnapshot::new(root.path(), &project).unwrap();
            assert_eq!(
                jobs.start_export("new", snapshot, root.path().join("test.msproj"), 0)
                    .unwrap_err(),
                "at most one package task may run per session"
            );
            capture.first("package.start.active_task", "poison-package");
            assert_eq!(recovered(&task).status.state, "running");
        }
        "spawn" | "spawn_long" => {
            let original = if mode == "spawn_long" {
                "故".repeat(4096)
            } else {
                "controlled thread spawn failure".into()
            };
            {
                let mut scope = RecoveryScope::new();
                let _registry = jobs.tasks.lock().unwrap();
                let reason = mark_spawn_failed(
                    &mut scope,
                    &task,
                    "poison-package",
                    crate::task_logging::TaskLog::new("poison-package", "export_project"),
                    std::io::Error::other(original.clone()),
                );
                assert_eq!(reason, original);
                assert!(capture.matching("media").is_empty());
            }
            capture.first("package.start.spawn_failure_task", "poison-package");
            let state = recovered(&task);
            assert_eq!(state.status.state, "failed");
            assert_eq!(state.status.error.as_deref(), Some(original.as_str()));
            let errors = capture.matching("media error request=poison-package");
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].0, log::Level::Error);
            assert!(errors[0].1.contains("phase=spawn"));
            assert!(errors[0]
                .1
                .contains(&original.chars().take(2048).collect::<String>()));
            assert!(!errors[0].1.contains(&"故".repeat(2049)));
        }
        "drop" => {
            let map = std::mem::take(&mut *jobs.tasks.lock().unwrap());
            let owned = PackageJobs {
                tasks: Mutex::new(map),
            };
            poison(&owned.tasks);
            drop(owned);
            capture.first("package.drop.registry", "session");
            capture.first("package.drop.task", "poison-package");
            let state = recovered(&task);
            assert!(state.cancel);
            assert_eq!(state.status.phase, "cancelling");
        }
        _ => unreachable!(),
    }
    assert!(task.is_poisoned());
    if mode != "drop" {
        assert_eq!(
            jobs.status("poison-package").unwrap_err(),
            "package task poisoned"
        );
    }
}

#[test]
fn package_worker_progress_and_result_recoveries_preserve_success_and_cancel() {
    let Some(mode) = case("packages::poison_tests::package_worker_progress_and_result_recoveries_preserve_success_and_cancel",
        &["success", "cancel", "failure"]) else { return; };
    let root = tempfile::tempdir().unwrap();
    let project = motion_model::Project::new(32, 32, 30, 30).unwrap();
    let snapshot = motion_core::storage::PackageSnapshot::new(root.path(), &project).unwrap();
    let jobs = PackageJobs::default();
    let (gate, entered, release) = Gate::new();
    let observed: Arc<Mutex<Option<Arc<Mutex<Task>>>>> = Arc::new(Mutex::new(None));
    let target = observed.clone();
    let capture = Capture::install(move |text| {
        if text.starts_with("media start request=poison-package ") {
            gate.block();
        }
        if text.contains("media poison recovery") || text.starts_with("media error") {
            if let Some(task) = target.lock().unwrap().clone() {
                unlocked(&task);
            }
        }
    });
    let output = root.path().join("out/test.msproj");
    if mode == "failure" {
        std::fs::write(root.path().join("out"), b"block directory").unwrap();
    }
    jobs.start_export("poison-package", snapshot, output.clone(), 0)
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let task = jobs.task("poison-package").unwrap();
    *observed.lock().unwrap() = Some(task.clone());
    if mode == "cancel" {
        jobs.cancel("poison-package").unwrap();
    }
    poison(&task);
    drop(release);
    capture.first("package.worker.result_task", "poison-package");
    if mode != "failure" {
        capture.first("package.worker.progress_task", "poison-package");
    }
    let state = recovered(&task);
    assert_eq!(
        state.status.state,
        match mode.as_str() {
            "success" => "succeeded",
            "cancel" => "cancelled",
            _ => "failed",
        }
    );
    assert_eq!(output.exists(), mode == "success");
    assert_eq!(
        capture.matching("media error request=poison-package").len(),
        usize::from(mode == "failure")
    );
    if mode == "failure" {
        let reason = state.status.error.as_ref().unwrap();
        assert!(capture.matching("media error")[0].1.contains(reason));
    }
    drop(state);
    assert_eq!(
        jobs.status("poison-package").unwrap_err(),
        "package task poisoned"
    );
}
