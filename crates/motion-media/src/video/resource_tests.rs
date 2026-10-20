use super::completion_tests::{prepared, task};
use super::*;
use crate::jobs::resource_tests::{accessible, PanickingSink};

fn check_resource(test: &str, mode: &'static str) {
    if !crate::registry_observation::tests::child(test) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("not-a-directory");
    fs::write(&path, b"foreign file").unwrap();
    let jobs = VideoJobs::new(directory.path().into()).unwrap();
    let task = task();
    let mut prepared = prepared(path.clone());
    prepared.options.at_frame = if mode == "commit" { 30 } else { 0 };
    {
        let mut state = task.lock().unwrap();
        state.status.state = "ready".into();
        state.status.phase = "awaiting_commit".into();
        state.prepared = Some(prepared);
    }
    jobs.tasks
        .lock()
        .unwrap()
        .insert("resource-video".into(), task.clone());
    let observed = task.clone();
    crate::jobs::completion_tests::check_blocked_teardown_cleanup(
        &path,
        move || {
            accessible(&observed, |task| {
                task.prepared.is_none() && (mode != "cancel" || task.cancel.load(Ordering::Acquire))
            })
        },
        move || match mode {
            "cancel" => {
                assert_eq!(jobs.cancel("resource-video").unwrap().state, "cancelled");
            }
            "commit" => {
                let mut engine =
                    motion_core::Engine::new(motion_model::Project::new(32, 32, 30, 30).unwrap())
                        .unwrap();
                let status = jobs.commit("resource-video", &mut engine).unwrap();
                assert_eq!(
                    status.error.as_deref(),
                    Some("video insertion outside composition")
                );
                assert_eq!(status.state, "failed");
                assert!(engine.project().layers.is_empty());
            }
            "panic" => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    jobs.commit("resource-video", &mut PanickingSink)
                }));
                assert_eq!(
                    crate::panic_reason(result.err().expect("commit provider must panic")),
                    "controlled commit provider panic"
                );
            }
            _ => unreachable!(),
        },
    );
    if mode == "panic" {
        assert!(
            task.is_poisoned(),
            "do not clear the original poisoned state"
        );
    }
}

#[test]
fn cancelled_resource_cleanup_releases_task_lock() {
    check_resource(
        "video::resource_tests::cancelled_resource_cleanup_releases_task_lock",
        "cancel",
    );
}

#[test]
fn failed_commit_resource_cleanup_releases_task_lock() {
    check_resource(
        "video::resource_tests::failed_commit_resource_cleanup_releases_task_lock",
        "commit",
    );
}

#[test]
fn panicking_commit_resource_cleanup_releases_task_lock() {
    check_resource(
        "video::resource_tests::panicking_commit_resource_cleanup_releases_task_lock",
        "panic",
    );
}

#[test]
fn poisoned_existing_task_rejects_reservation_without_another_panic() {
    if !crate::registry_observation::tests::child(
        "video::resource_tests::poisoned_existing_task_rejects_reservation_without_another_panic",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let jobs = VideoJobs::new(directory.path().into()).unwrap();
    let task = task();
    jobs.tasks
        .lock()
        .unwrap()
        .insert("poisoned".into(), task.clone());
    assert!(std::panic::catch_unwind(|| {
        let _state = task.lock().unwrap();
        panic!("controlled existing task poison");
    })
    .is_err());
    let error = jobs
        .reserve("next", "import_video")
        .err()
        .expect("poisoned task must reject reservation");
    assert_eq!(error.to_string(), "video task poisoned");
    assert_eq!(jobs.tasks.lock().unwrap().len(), 1);
    assert!(task.is_poisoned());
    assert_eq!(
        jobs.status("poisoned").err().map(|error| error.to_string()).as_deref(),
        Some("video task poisoned")
    );
    let healthy = VideoJobs::new(directory.path().into()).unwrap();
    let (_task, _worker) = healthy.reserve("healthy", "import_video").unwrap();
}
