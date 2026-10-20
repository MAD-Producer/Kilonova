use super::*;
use std::sync::atomic::AtomicUsize;

struct Capture {
    path: String,
    unlocked: Box<dyn Fn() -> bool + Send + Sync>,
    observation: Arc<AtomicUsize>,
}

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if record.level() == log::Level::Warn
            && text.contains("media stage cleanup failed")
            && text.contains(&self.path)
        {
            self.observation
                .store(if (self.unlocked)() { 1 } else { 2 }, Ordering::Release);
        }
    }
    fn flush(&self) {}
}

pub(crate) fn watch_cleanup(
    path: &Path,
    unlocked: impl Fn() -> bool + Send + Sync + 'static,
) -> Arc<AtomicUsize> {
    let observation = Arc::new(AtomicUsize::new(0));
    let capture = Box::leak(Box::new(Capture {
        path: path.display().to_string(),
        unlocked: Box::new(unlocked),
        observation: observation.clone(),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    observation
}

fn task() -> Arc<Mutex<Task>> {
    Arc::new(Mutex::new(Task {
        status: TaskStatus {
            request_id: "probe-cleanup-audio".into(),
            operation: "probe_audio".into(),
            state: "running".into(),
            phase: "waveform".into(),
            progress: 0.5,
            metadata: None,
            edit_result: None,
            error: None,
        },
        prepared: None,
        cancel: Arc::new(AtomicBool::new(false)),
    }))
}

fn prepared(path: PathBuf) -> Prepared {
    Prepared {
        stage: Stage { path },
        asset: AudioAsset {
            id: 1,
            path: "assets/probe.wav".into(),
            mime: "audio/wav".into(),
            bytes: 64,
            track: 0,
            sample_rate: 48_000,
            channels: 1,
            sample_frames: 48_000,
            duration_us: 1_000_000,
        },
        options: ImportOptions {
            request_id: "probe-cleanup-audio".into(),
            at_frame: 0,
            track: None,
            name: "probe".into(),
        },
    }
}

#[test]
fn probe_cleanup_failure_is_observed_outside_task_lock_before_terminal_status() {
    if !crate::registry_observation::tests::child(
        "jobs::completion_tests::probe_cleanup_failure_is_observed_outside_task_lock_before_terminal_status",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("not-a-directory");
    fs::write(&path, b"cleanup must report this filesystem error").unwrap();
    let task = task();
    let observed_task = task.clone();
    let observation = watch_cleanup(&path, move || {
        observed_task
            .try_lock()
            .is_ok_and(|task| !task.status.terminal())
    });
    let prepared = prepared(path.clone());
    let metadata = prepared.asset.clone();
    finish_import(
        &task,
        Ok(Ok(prepared)),
        true,
        &crate::task_logging::TaskLog::new("probe-cleanup-audio", "probe_audio"),
    );
    assert_eq!(
        observation.load(Ordering::Acquire),
        1,
        "cleanup held the task lock or terminal status preceded cleanup"
    );
    let task = task.lock().unwrap();
    assert_eq!(task.status.state, "succeeded");
    assert_eq!(task.status.phase, "complete");
    assert_eq!(task.status.metadata.as_ref(), Some(&metadata));
    assert!(task.status.error.is_none());
    assert!(task.prepared.is_none());
    assert!(path.is_file(), "cleanup failure must not be concealed");
}

#[test]
fn completion_preserves_probe_import_cancellation_and_failure_contracts() {
    for probe in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stage");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("source"), b"staged source").unwrap();
        let prepared = prepared(path.clone());
        let metadata = prepared.asset.clone();
        let task = task();
        finish_import(
            &task,
            Ok(Ok(prepared)),
            probe,
            &crate::task_logging::TaskLog::new("probe-cleanup-audio", "test"),
        );
        let mut state = task.lock().unwrap();
        assert_eq!(state.status.metadata.as_ref(), Some(&metadata));
        assert_eq!(state.status.progress, 1.0);
        assert!(state.status.error.is_none());
        if probe {
            assert_eq!(state.status.state, "succeeded");
            assert_eq!(state.status.phase, "complete");
            assert!(state.prepared.is_none());
            assert!(!path.exists());
        } else {
            assert_eq!(state.status.state, "ready");
            assert_eq!(state.status.phase, "awaiting_commit");
            assert!(path.join("source").is_file());
            let retained = state.prepared.take().unwrap();
            drop(state);
            drop(retained);
            assert!(!path.exists());
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cancelled-stage");
    fs::create_dir(&path).unwrap();
    let task = task();
    {
        let mut state = task.lock().unwrap();
        state.cancel.store(true, Ordering::Release);
        state.status.state = "cancelled".into();
        state.status.phase = "cancelled".into();
    }
    finish_import(
        &task,
        Ok(Ok(prepared(path.clone()))),
        true,
        &crate::task_logging::TaskLog::new("probe-cleanup-audio", "probe_audio"),
    );
    assert_eq!(task.lock().unwrap().status.state, "cancelled");
    assert!(task.lock().unwrap().status.metadata.is_none());
    assert!(!path.exists());

    let failed = self::task();
    finish_import(
        &failed,
        Ok(Err("original decode failure".into())),
        true,
        &crate::task_logging::TaskLog::new("probe-cleanup-audio", "probe_audio"),
    );
    assert_eq!(failed.lock().unwrap().status.state, "failed");
    assert_eq!(
        failed.lock().unwrap().status.error.as_deref(),
        Some("original decode failure")
    );
}
