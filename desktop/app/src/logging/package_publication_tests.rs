use super::*;
use motion_core::storage::{PackageSnapshot, PreparedPackage};
use motion_media::PackageJobs;
use motion_model::{PackageExportSource, PackagePublication};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, Sender},
    time::Instant,
};

struct Source {
    snapshot: PackageSnapshot,
    ready: Sender<()>,
    proceed: Receiver<()>,
    mode: &'static str,
    log: PathBuf,
    id: String,
}

struct Publication {
    archive: Option<PreparedPackage>,
    mode: &'static str,
    directory: PathBuf,
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
        // Actual default-level start must be on disk before entering the provider.
        assert!(fs::read_to_string(self.log).unwrap().contains(&format!(
            "media start request={} operation=export_project",
            self.id
        )));
        let archive = self.snapshot.prepare(output, progress)?;
        self.ready.send(()).unwrap();
        self.proceed.recv_timeout(Duration::from_secs(10)).unwrap();
        Ok(Publication {
            archive: Some(archive),
            mode: self.mode,
            directory: output.parent().unwrap().to_path_buf(),
        })
    }
}

impl PackagePublication for Publication {
    fn publish(&mut self) -> motion_model::Result<PathBuf> {
        if self.mode.starts_with("publish") {
            panic!("native package publication provider panic");
        }
        PackagePublication::publish(self.archive.as_mut().unwrap())
    }
}

impl Drop for Publication {
    fn drop(&mut self) {
        if self.mode == "rename-cleanup-panic" {
            let stage = fs::read_dir(&self.directory)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".ms-package-")
                })
                .unwrap();
            assert!(stage
                .canonicalize()
                .unwrap()
                .starts_with(self.directory.canonicalize().unwrap()));
            // Obstruct only the test-owned archive so the actual destructor's
            // remove_file fails. Foreign contents must survive best-effort cleanup.
            fs::remove_file(&stage).unwrap();
            fs::create_dir(&stage).unwrap();
            fs::write(stage.join("foreign"), b"preserve cleanup obstruction").unwrap();
        }
        // Do real cleanup first; this provider failure must not replace a rename error.
        drop(self.archive.take());
        if self.mode.ends_with("cleanup-panic") {
            panic!("native package cleanup provider panic");
        }
    }
}

#[test]
fn archive_publication_and_cleanup_faults_reach_default_channels_without_stranded_tasks() {
    const CHILD: &str = "MOTION_PACKAGE_PUBLICATION_FILE_TEST_CHILD";
    const TEST: &str = "logging::package_publication_tests::archive_publication_and_cleanup_faults_reach_default_channels_without_stranded_tasks";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        let log = root.join("logs/motion-studio.log");
        let mut errors = Vec::new();
        for mode in [
            "rename",
            "publish-panic",
            "publish-cleanup-panic",
            "rename-cleanup-panic",
            "success-cleanup-panic",
            "cancel-cleanup-panic",
        ] {
            let project_root = root.join(mode);
            fs::create_dir_all(&project_root).unwrap();
            let project = motion_core::Project::new(32, 32, 30, 30).unwrap();
            let output = project_root.join("output.msproj");
            let id = format!("native-package-{mode}");
            let jobs = PackageJobs::default();
            let (ready_tx, ready_rx) = mpsc::channel();
            let (proceed_tx, proceed_rx) = mpsc::channel();
            jobs.start_export(
                &id,
                Source {
                    snapshot: PackageSnapshot::new(&project_root, &project).unwrap(),
                    ready: ready_tx,
                    proceed: proceed_rx,
                    mode,
                    log: log.clone(),
                    id: id.clone(),
                },
                output.clone(),
                25,
            )
            .unwrap();
            ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            if mode.starts_with("rename") {
                fs::create_dir(&output).unwrap();
                fs::write(output.join("foreign"), b"must remain intact").unwrap();
            }
            if mode.starts_with("cancel") {
                assert_eq!(jobs.cancel(&id).unwrap().phase, "cancelling");
            }
            proceed_tx.send(()).unwrap();
            let began = Instant::now();
            let done = loop {
                let status = jobs.status(&id).unwrap();
                if status.state != "running" {
                    break status;
                }
                assert!(
                    began.elapsed() < Duration::from_secs(10),
                    "stranded package: {status:?}"
                );
                std::thread::sleep(Duration::from_millis(1));
            };
            assert_eq!(
                done.state,
                if mode.starts_with("cancel") {
                    "cancelled"
                } else {
                    "failed"
                }
            );
            let error = done.error.unwrap();
            if mode.contains("cleanup-panic") {
                assert!(error.contains("native package cleanup provider panic"));
            }
            if mode.starts_with("publish") {
                assert!(error.contains("native package publication provider panic"));
            }
            if mode.starts_with("rename") {
                assert!(error.contains("os error"));
                assert_eq!(
                    fs::read(output.join("foreign")).unwrap(),
                    b"must remain intact"
                );
            }
            if mode.starts_with("success") {
                assert!(error.contains("archive published at"));
                assert_eq!(
                    motion_core::storage::import_package(&output, &project_root.join("restored"))
                        .unwrap(),
                    project
                );
            }
            let stages: Vec<_> = fs::read_dir(&project_root)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".ms-package-")
                })
                .collect();
            if mode == "rename-cleanup-panic" {
                assert_eq!(stages.len(), 1);
                assert_eq!(
                    fs::read(stages[0].join("foreign")).unwrap(),
                    b"preserve cleanup obstruction"
                );
            } else {
                assert!(stages.is_empty());
            }
            // Terminal state and completion logging occur in order on the real worker.
            while !fs::read_to_string(&log)
                .unwrap()
                .contains(&format!("media error request={id}"))
            {
                assert!(
                    began.elapsed() < Duration::from_secs(10),
                    "missing package failure log"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            jobs.release(&id).unwrap();
            errors.push((id, error));
        }
        fs::write(
            root.join("errors.json"),
            serde_json::to_vec_pretty(&errors).unwrap(),
        )
        .unwrap();
        return;
    }

    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let errors: Vec<(String, String)> =
        serde_json::from_slice(&fs::read(root.path().join("errors.json")).unwrap()).unwrap();
    assert_eq!(errors.len(), 6);
    assert!(text.contains("logging initialized level=INFO"));
    for (id, error) in errors {
        for sink in [&text, &stderr] {
            let records: Vec<_> = sink
                .lines()
                .filter(|line| line.contains(&format!("media error request={id} ")))
                .collect();
            assert_eq!(
                records.len(),
                1,
                "duplicate or missing failure for {id}: {sink}"
            );
            assert!(records[0].contains(&error));
            assert!(records[0].contains("operation=export_project phase="));
            assert!(records[0].contains("ms:"));
        }
        assert_eq!(
            text.lines()
                .filter(|line| line.contains(&format!("media start request={id} ")))
                .count(),
            1
        );
        assert!(!stdout.contains(&id));
        assert!(!stdout.contains(&error));
    }
    assert!(!stderr.contains("media start"));
    assert_eq!(
        text.lines()
            .filter(|line| line.contains("project temporary cleanup failed:"))
            .count(),
        1
    );
    assert!(!stderr.contains("project temporary cleanup failed:"));
    assert!(!stdout.contains("project temporary cleanup failed:"));
    assert!(!text.contains("PoisonError"));
    assert!(!stderr.contains("PoisonError"));
}
