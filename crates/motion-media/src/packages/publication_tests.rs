use super::*;
use motion_core::storage::{PackageSnapshot, PreparedPackage};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, PartialEq)]
enum Outcome {
    Success,
    RenameFailure,
    PublicationPanic,
    PublicationAndCleanupPanic,
    RenameAndCleanupFailure,
    SuccessCleanupPanic,
    CancelCleanupPanic,
}

struct Source {
    snapshot: PackageSnapshot,
    prepared: Sender<()>,
    proceed: Receiver<()>,
    publication: Publication,
}

struct Publication {
    archive: Option<PreparedPackage>,
    task: Arc<Mutex<Option<Arc<Mutex<Task>>>>>,
    entered: Sender<Option<(String, String, bool)>>,
    release: Receiver<()>,
    outcome: Outcome,
    publish_locked: Arc<Mutex<Option<bool>>>,
    primary: Arc<Mutex<Option<String>>>,
}

impl PackageExportSource for Source {
    type Prepared = Publication;

    fn total_bytes(&self) -> u64 {
        self.snapshot.total_bytes()
    }

    fn source_count(&self) -> usize {
        self.snapshot.source_count()
    }

    fn prepare(
        self,
        output: &Path,
        progress: impl FnMut(u64, u64) -> motion_model::Result<()>,
    ) -> motion_model::Result<Publication> {
        let mut publication = self.publication;
        publication.archive = Some(self.snapshot.prepare(output, progress)?);
        self.prepared.send(()).unwrap();
        self.proceed.recv_timeout(WAIT).unwrap();
        Ok(publication)
    }
}

impl PackagePublication for Publication {
    fn publish(&mut self) -> motion_model::Result<PathBuf> {
        let task = self.task.lock().unwrap().as_ref().unwrap().clone();
        *self.publish_locked.lock().unwrap() = Some(task.try_lock().is_err());
        if matches!(
            self.outcome,
            Outcome::PublicationPanic | Outcome::PublicationAndCleanupPanic
        ) {
            panic!("controlled package publication panic");
        }
        let result = PackagePublication::publish(self.archive.as_mut().unwrap());
        if let Err(error) = &result {
            *self.primary.lock().unwrap() = Some(error.to_string());
        }
        result
    }
}

impl Drop for Publication {
    fn drop(&mut self) {
        let task = self.task.lock().unwrap().as_ref().unwrap().clone();
        let observed = task.try_lock().ok().map(|task| {
            (
                task.status.state.clone(),
                task.status.phase.clone(),
                task.cancel,
            )
        });
        self.entered.send(observed).unwrap();
        self.release.recv_timeout(WAIT).unwrap();
        // Exercise the real archive destructor before any injected provider panic.
        drop(self.archive.take());
        if matches!(
            self.outcome,
            Outcome::RenameAndCleanupFailure
                | Outcome::PublicationAndCleanupPanic
                | Outcome::SuccessCleanupPanic
                | Outcome::CancelCleanupPanic
        ) {
            panic!("controlled package cleanup panic");
        }
    }
}

fn staging_count(root: &Path) -> usize {
    std::fs::read_dir(root)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".ms-package-")
        })
        .count()
}

fn run(outcome: Outcome) {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().canonicalize().unwrap().join("result.msproj");
    let project = motion_model::Project::new(32, 32, 30, 30).unwrap();
    let snapshot = PackageSnapshot::new(root.path(), &project).unwrap();
    let (prepared_tx, prepared_rx) = mpsc::channel();
    let (proceed_tx, proceed_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let observed_task = Arc::new(Mutex::new(None));
    let publish_locked = Arc::new(Mutex::new(None));
    let primary = Arc::new(Mutex::new(None));
    let jobs = Arc::new(PackageJobs::default());
    jobs.start_export(
        "package-publication-boundary",
        Source {
            snapshot,
            prepared: prepared_tx,
            proceed: proceed_rx,
            publication: Publication {
                archive: None,
                task: observed_task.clone(),
                entered: entered_tx,
                release: release_rx,
                outcome,
                publish_locked: publish_locked.clone(),
                primary: primary.clone(),
            },
        },
        output.clone(),
        19,
    )
    .unwrap();
    let task = jobs.task("package-publication-boundary").unwrap();
    *observed_task.lock().unwrap() = Some(task.clone());
    prepared_rx.recv_timeout(WAIT).unwrap();
    assert_eq!(staging_count(root.path()), 1);
    let cancelled = outcome == Outcome::CancelCleanupPanic;
    if cancelled {
        assert_eq!(
            jobs.cancel("package-publication-boundary").unwrap().phase,
            "cancelling"
        );
    }
    if matches!(
        outcome,
        Outcome::RenameFailure | Outcome::RenameAndCleanupFailure
    ) {
        std::fs::create_dir(&output).unwrap();
        std::fs::write(output.join("foreign"), b"preserve this directory").unwrap();
    }
    proceed_tx.send(()).unwrap();
    let observed = entered_rx.recv_timeout(WAIT).unwrap();
    // Do not block on public APIs if a regression is holding the task mutex.
    let concurrent = observed.as_ref().map(|_| {
        let status = jobs.status("package-publication-boundary").unwrap();
        let cancel = if cancelled {
            None
        } else {
            let jobs = jobs.clone();
            Some(std::thread::spawn(move || {
                jobs.cancel("package-publication-boundary")
            }))
        };
        if cancel.is_some() {
            let began = Instant::now();
            // The waiting cancel owns a Condvar reference and releases the task
            // mutex. This checks actual wait entry before freeing the cleanup gate.
            loop {
                if let Ok(task) = task.try_lock() {
                    if Arc::strong_count(&task.completed) > 1 {
                        break;
                    }
                }
                assert!(
                    began.elapsed() < WAIT,
                    "cancel never released the mutex while waiting for cleanup"
                );
                std::thread::yield_now();
            }
        }
        let release = jobs.release("package-publication-boundary").unwrap_err();
        let next = jobs
            .start_export(
                "next-export",
                PackageSnapshot::new(root.path(), &project).unwrap(),
                root.path().join("next.msproj"),
                20,
            )
            .unwrap_err();
        (status, cancel, release, next)
    });
    release_tx.send(()).unwrap();
    let began = Instant::now();
    let done = loop {
        let status = jobs.status("package-publication-boundary").unwrap();
        if status.terminal() {
            break status;
        }
        assert!(
            began.elapsed() < WAIT,
            "package worker stranded its task: {status:?}"
        );
        std::thread::yield_now();
    };
    let phase = if cancelled {
        "cancelling"
    } else {
        "finalizing"
    };
    assert_eq!(observed, Some(("running".into(), phase.into(), cancelled)));
    let (pending, cancel, release, next) = concurrent.unwrap();
    assert_eq!(pending.phase, phase);
    if let Some(cancel) = cancel {
        let acknowledged = cancel.join().unwrap().unwrap();
        assert_eq!(acknowledged.state, done.state);
        assert_eq!(acknowledged.phase, done.phase);
    }
    assert_eq!(release, "wait for package task completion before release");
    assert_eq!(next, "at most one package task may run per session");
    assert_eq!(
        task.lock().unwrap().cancel,
        cancelled,
        "late cancellation changed the committed decision"
    );
    assert_eq!(
        *publish_locked.lock().unwrap(),
        (!cancelled).then_some(true)
    );
    assert_eq!(done.frozen_revision, 19);
    assert_eq!(
        staging_count(root.path()),
        0,
        "terminal status preceded real archive cleanup"
    );
    match outcome {
        Outcome::Success => {
            assert_eq!(done.state, "succeeded");
            assert_eq!(done.path.as_ref(), Some(&output));
            assert!(done.error.is_none());
            let restored =
                motion_core::storage::import_package(&output, &root.path().join("restored"))
                    .unwrap();
            assert_eq!(restored, project);
        }
        Outcome::CancelCleanupPanic => {
            assert_eq!(done.state, "cancelled");
            assert!(done
                .error
                .unwrap()
                .contains("controlled package cleanup panic"));
            assert!(!output.exists());
        }
        _ => {
            assert_eq!(done.state, "failed");
            let error = done.error.unwrap();
            if let Some(primary) = primary.lock().unwrap().as_ref() {
                assert!(
                    error.contains(primary),
                    "primary publication failure was replaced: {error}"
                );
                assert_eq!(
                    std::fs::read(output.join("foreign")).unwrap(),
                    b"preserve this directory"
                );
            }
            if matches!(
                outcome,
                Outcome::PublicationPanic | Outcome::PublicationAndCleanupPanic
            ) {
                assert!(error.contains("controlled package publication panic"));
            }
            if matches!(
                outcome,
                Outcome::RenameAndCleanupFailure
                    | Outcome::SuccessCleanupPanic
                    | Outcome::PublicationAndCleanupPanic
            ) {
                assert!(error.contains("controlled package cleanup panic"));
            }
            if outcome == Outcome::SuccessCleanupPanic {
                assert!(error.contains("archive published at"));
                assert_eq!(
                    motion_core::storage::import_package(&output, &root.path().join("restored"))
                        .unwrap(),
                    project
                );
            }
        }
    }
    jobs.release("package-publication-boundary").unwrap();
    // No poisoned lock, retained staging allocation, or permanently active task.
    assert!(!jobs.contains("package-publication-boundary"));
}

#[test]
fn failed_archive_publication_cleans_up_after_unlock_and_preserves_destination() {
    run(Outcome::RenameFailure);
}

#[test]
fn successful_archive_cleanup_keeps_task_active_without_allowing_late_cancellation() {
    run(Outcome::Success);
}

#[test]
fn publication_provider_panic_reaches_terminal_failure_without_poisoning_task() {
    run(Outcome::PublicationPanic);
}

#[test]
fn publication_failure_keeps_primary_cause_when_cleanup_provider_panics() {
    run(Outcome::RenameAndCleanupFailure);
}

#[test]
fn publication_and_cleanup_panics_are_caught_separately_without_aborting_worker() {
    run(Outcome::PublicationAndCleanupPanic);
}

#[test]
fn cleanup_provider_panic_reports_an_already_published_archive() {
    run(Outcome::SuccessCleanupPanic);
}

#[test]
fn cancellation_cleanup_provider_panic_preserves_terminal_cancellation_and_cause() {
    run(Outcome::CancelCleanupPanic);
}
