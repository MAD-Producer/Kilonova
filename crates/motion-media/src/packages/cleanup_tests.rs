use super::*;
use motion_core::storage::{PackageSnapshot, PreparedPackage};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);
type Observation = Option<(String, String, bool)>;

#[derive(Clone, Copy)]
enum Boundary {
    Cleanup,
    Publication,
}

struct Gate {
    task: Arc<Mutex<Option<Arc<Mutex<Task>>>>>,
    entered: Sender<Observation>,
    release: Receiver<()>,
    boundary: Boundary,
}

impl Gate {
    fn wait(&self) {
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
    }
}

struct ControlledSource {
    snapshot: PackageSnapshot,
    prepared: Sender<()>,
    proceed: Receiver<()>,
    gate: Gate,
    publications: Arc<AtomicUsize>,
}

struct ControlledPublication {
    archive: Option<PreparedPackage>,
    gate: Gate,
    publications: Arc<AtomicUsize>,
}

impl PackageExportSource for ControlledSource {
    type Prepared = ControlledPublication;

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
    ) -> motion_model::Result<Self::Prepared> {
        // Use the actual archive writer and destructor. Only scheduling is controlled.
        let archive = self.snapshot.prepare(output, progress)?;
        self.prepared.send(()).unwrap();
        self.proceed.recv_timeout(WAIT).unwrap();
        Ok(ControlledPublication {
            archive: Some(archive),
            gate: self.gate,
            publications: self.publications,
        })
    }
}

impl PackagePublication for ControlledPublication {
    fn publish(&mut self) -> motion_model::Result<PathBuf> {
        if matches!(self.gate.boundary, Boundary::Publication) {
            self.gate.wait();
        }
        self.publications.fetch_add(1, Ordering::AcqRel);
        PackagePublication::publish(self.archive.as_mut().unwrap())
    }
}

impl Drop for ControlledPublication {
    fn drop(&mut self) {
        if self.archive.is_some() && matches!(self.gate.boundary, Boundary::Cleanup) {
            self.gate.wait();
        }
        // PreparedPackage then removes the real staging archive through its Drop.
    }
}

fn staging_files(root: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".ms-package-")
        })
        .collect()
}

fn run(boundary: Boundary) {
    let directory = tempfile::tempdir().unwrap();
    let project = motion_model::Project::new(32, 32, 30, 30).unwrap();
    let output = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("result.msproj");
    std::fs::write(&output, b"previous destination").unwrap();
    let snapshot = PackageSnapshot::new(directory.path(), &project).unwrap();
    let (prepared_tx, prepared_rx) = mpsc::channel();
    let (proceed_tx, proceed_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let observed_task = Arc::new(Mutex::new(None));
    let publications = Arc::new(AtomicUsize::new(0));
    let jobs = Arc::new(PackageJobs::default());
    jobs.start_export(
        "controlled-cleanup",
        ControlledSource {
            snapshot,
            prepared: prepared_tx,
            proceed: proceed_rx,
            gate: Gate {
                task: observed_task.clone(),
                entered: entered_tx,
                release: release_rx,
                boundary,
            },
            publications: publications.clone(),
        },
        output.clone(),
        17,
    )
    .unwrap();
    let task = jobs.task("controlled-cleanup").unwrap();
    *observed_task.lock().unwrap() = Some(task);
    prepared_rx.recv_timeout(WAIT).unwrap();
    let staged = staging_files(directory.path());
    assert_eq!(
        staged.len(),
        1,
        "the actual archive writer must have staged a file"
    );
    if matches!(boundary, Boundary::Cleanup) {
        let acknowledged = jobs.cancel("controlled-cleanup").unwrap();
        assert_eq!(acknowledged.state, "running");
        assert_eq!(acknowledged.phase, "cancelling");
    }
    proceed_tx.send(()).unwrap();
    let observation = entered_rx.recv_timeout(WAIT).unwrap();
    let cancel = if matches!(boundary, Boundary::Publication) {
        let jobs = jobs.clone();
        Some(std::thread::spawn(move || {
            jobs.cancel("controlled-cleanup")
        }))
    } else {
        None
    };
    // Only call lock-taking APIs after the nonblocking observation proved access.
    let concurrent = observation.as_ref().map(|_| {
        let status = jobs.status("controlled-cleanup").unwrap();
        let release_refused = jobs.release("controlled-cleanup").is_err();
        let next = jobs.start_export(
            "next-export",
            PackageSnapshot::new(directory.path(), &project).unwrap(),
            directory.path().join("next.msproj"),
            18,
        );
        (status, release_refused, next.err())
    });
    let archive_existed_during_gate = staged[0].is_file();
    let unchanged_during_gate = std::fs::read(&output).unwrap() == b"previous destination";
    // Release before assertions: the negative baseline must not strand a worker.
    release_tx.send(()).unwrap();
    let began = Instant::now();
    let done = loop {
        let status = jobs.status("controlled-cleanup").unwrap();
        if status.terminal() {
            break status;
        }
        assert!(
            began.elapsed() < WAIT,
            "completion never reached a terminal state"
        );
        std::thread::yield_now();
    };
    assert!(archive_existed_during_gate);
    assert!(unchanged_during_gate);
    assert!(staging_files(directory.path()).is_empty());
    assert!(done.error.is_none());
    assert_eq!(done.frozen_revision, 17);
    match boundary {
        Boundary::Cleanup => {
            assert_eq!(
                observation,
                Some(("running".into(), "cancelling".into(), true)),
                "cancelled archive cleanup held the task lock or published a terminal state early"
            );
            let (status, release_refused, next_error) = concurrent.unwrap();
            assert_eq!(status.state, "running");
            assert_eq!(status.phase, "cancelling");
            assert!(release_refused);
            assert_eq!(
                next_error.as_deref(),
                Some("at most one package task may run per session")
            );
            assert_eq!(publications.load(Ordering::Acquire), 0);
            assert_eq!(done.state, "cancelled");
            assert_eq!(done.phase, "cancelled");
            assert!(done.path.is_none());
            assert_eq!(std::fs::read(&output).unwrap(), b"previous destination");
        }
        Boundary::Publication => {
            assert_eq!(
                observation, None,
                "publication must retain the cancellation lock"
            );
            let acknowledged = cancel.unwrap().join().unwrap().unwrap();
            assert_eq!(acknowledged.state, "succeeded");
            assert_eq!(publications.load(Ordering::Acquire), 1);
            assert_eq!(done.state, "succeeded");
            assert_eq!(done.path.as_ref(), Some(&output));
            let restored =
                motion_core::storage::import_package(&output, &directory.path().join("restored"))
                    .unwrap();
            assert_eq!(restored, project);
        }
    }
    jobs.release("controlled-cleanup").unwrap();
    assert!(!jobs.contains("controlled-cleanup"));
}

#[test]
fn cancelling_archive_cleanup_releases_task_lock_before_terminal_status() {
    run(Boundary::Cleanup);
}

#[test]
fn successful_archive_publication_remains_atomic_with_cancellation() {
    run(Boundary::Publication);
}
