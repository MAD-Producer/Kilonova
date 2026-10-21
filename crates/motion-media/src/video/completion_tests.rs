use super::*;

pub(super) fn task() -> Arc<Mutex<Task>> {
    Arc::new(Mutex::new(Task {
        status: VideoTaskStatus {
            request_id: "probe-cleanup-video".into(),
            operation: "probe_video".into(),
            state: "running".into(),
            phase: "probing".into(),
            progress: Some(0.5),
            metadata: None,
            edit_result: None,
            error: None,
        },
        prepared: None,
        cancel: Arc::new(AtomicBool::new(false)),
    }))
}

pub(super) fn prepared(path: PathBuf) -> Prepared {
    Prepared {
        stage: Stage(path),
        probe: VideoProbe {
            asset: VideoAsset {
                id: 1,
                path: "assets/probe.mp4".into(),
                bytes: 64,
                mime: "video/avc".into(),
                track: 0,
                width: 8,
                height: 8,
                rotation: 0,
                display_width: 8,
                display_height: 8,
                video_start_us: 0,
                video_end_us: 1_000_000,
                duration_us: 1_000_000,
                frame_count: 1,
                variable_frame_rate: false,
                nominal_frame_rate: 1.0,
                color_standard: 1,
                color_range: 2,
                audio_asset: None,
            },
            timestamps: vec![0],
            audio_track: None,
            first_rgba: vec![255; 8 * 8 * 4],
            tracks: json!([]),
        },
        audio: None,
        options: VideoImportOptions {
            request_id: "probe-cleanup-video".into(),
            at_frame: 0,
            name: "probe".into(),
            track: None,
            audio_track: None,
            with_audio: false,
        },
    }
}

fn metadata(prepared: &Prepared) -> Value {
    json!({"video":prepared.probe.asset,"audio":prepared.audio,"audio_tracks":prepared.probe.tracks})
}

#[test]
fn probe_cleanup_failure_is_observed_outside_task_lock_before_terminal_status() {
    if !crate::registry_observation::tests::child(
        "video::completion_tests::probe_cleanup_failure_is_observed_outside_task_lock_before_terminal_status",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("not-a-directory");
    fs::write(&path, b"cleanup must report this filesystem error").unwrap();
    let task = task();
    let observed_task = task.clone();
    let observation = crate::jobs::completion_tests::watch_cleanup(&path, move || {
        observed_task
            .try_lock()
            .is_ok_and(|task| !task.status.terminal())
    });
    let prepared = prepared(path.clone());
    let expected = metadata(&prepared);
    finish_import(
        &task,
        Ok(Ok(prepared)),
        true,
        &crate::task_logging::TaskLog::new("probe-cleanup-video", "probe_video"),
    );
    assert_eq!(
        observation.load(Ordering::Acquire),
        1,
        "cleanup held the task lock or terminal status preceded cleanup"
    );
    let task = task.lock().unwrap();
    assert_eq!(task.status.state, "succeeded");
    assert_eq!(task.status.phase, "complete");
    assert_eq!(task.status.metadata.as_ref(), Some(&expected));
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
        let expected = metadata(&prepared);
        let task = task();
        finish_import(
            &task,
            Ok(Ok(prepared)),
            probe,
            &crate::task_logging::TaskLog::new("probe-cleanup-video", "test"),
        );
        let mut state = task.lock().unwrap();
        assert_eq!(state.status.metadata.as_ref(), Some(&expected));
        assert_eq!(state.status.progress, Some(1.0));
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
        &crate::task_logging::TaskLog::new("probe-cleanup-video", "probe_video"),
    );
    assert_eq!(task.lock().unwrap().status.state, "cancelled");
    assert!(task.lock().unwrap().status.metadata.is_none());
    assert!(!path.exists());

    let failed = self::task();
    finish_import(
        &failed,
        Ok(Err("original decode failure".into())),
        true,
        &crate::task_logging::TaskLog::new("probe-cleanup-video", "probe_video"),
    );
    assert_eq!(failed.lock().unwrap().status.state, "failed");
    assert_eq!(
        failed.lock().unwrap().status.error.as_deref(),
        Some("original decode failure")
    );
}

#[test]
fn teardown_cleanup_failure_is_observed_outside_task_lock() {
    if !crate::registry_observation::tests::child(
        "video::completion_tests::teardown_cleanup_failure_is_observed_outside_task_lock",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("not-a-directory");
    fs::write(&path, b"foreign file").unwrap();
    let jobs = VideoJobs::new(directory.path().into()).unwrap();
    let task = task();
    task.lock().unwrap().prepared = Some(prepared(path.clone()));
    let before = serde_json::to_value(&task.lock().unwrap().status).unwrap();
    jobs.tasks
        .lock()
        .unwrap()
        .insert("teardown-video".into(), task.clone());
    let observed_task = task.clone();
    crate::jobs::completion_tests::check_blocked_teardown_cleanup(
        &path,
        move || {
            observed_task
                .try_lock()
                .is_ok_and(|task| task.cancel.load(Ordering::Acquire) && task.prepared.is_none())
        },
        move || drop(jobs),
    );
    assert_eq!(
        serde_json::to_value(&task.lock().unwrap().status).unwrap(),
        before
    );
}

#[test]
fn teardown_removes_stages_and_cancels_tasks_without_prepared_resources() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("stage");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("source"), b"staged source").unwrap();
    let jobs = VideoJobs::new(directory.path().into()).unwrap();
    let ready = task();
    ready.lock().unwrap().prepared = Some(prepared(path.clone()));
    let pending = task();
    jobs.tasks.lock().unwrap().extend([
        ("ready".into(), ready.clone()),
        ("pending".into(), pending.clone()),
    ]);
    drop(jobs);
    assert!(!path.exists());
    for task in [ready, pending] {
        let task = task.lock().unwrap();
        assert!(task.cancel.load(Ordering::Acquire));
        assert!(task.prepared.is_none());
        assert!(task.status.error.is_none());
    }
}
