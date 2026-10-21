use super::*;
use crate::poison_observation::tests::*;

#[test]
fn audio_workers_and_drop_observe_real_poison_without_changing_outcomes() {
    let Some(mode) = case(
        "jobs::poison_tests::audio_workers_and_drop_observe_real_poison_without_changing_outcomes",
        &[
            "import",
            "probe",
            "cache",
            "import_cancel",
            "probe_cancel",
            "cache_cancel",
            "drop",
        ],
    ) else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let (gate, entered, release) = Gate::new();
    let decoder_gate = gate.clone();
    let decode: DecodeAudio = Arc::new(move |_, _, _, _, _| decoder_gate.fail());
    let jobs = AudioJobs::with_decoder(directory.path().into(), Limits::default(), decode).unwrap();
    let observed: Arc<Mutex<Option<Arc<Mutex<Task>>>>> = Arc::new(Mutex::new(None));
    let target = observed.clone();
    let capture = Capture::install(move |text| {
        if text.contains("media poison recovery") || text.starts_with("media error") {
            if let Some(task) = target.lock().unwrap().clone() {
                unlocked(&task);
            }
        }
    });
    if mode == "drop" {
        let (task, _worker, _initial) = jobs.reserve("poison-audio", "test").unwrap();
        *observed.lock().unwrap() = Some(task.clone());
        poison(&task);
        poison(&jobs.tasks);
        drop(jobs);
        capture.first("audio.drop.registry", "session");
        capture.first("audio.drop.task", "poison-audio");
        assert!(recovered(&task).cancel.load(Ordering::Acquire));
        return;
    }
    let cache = mode.starts_with("cache");
    if cache {
        fs::create_dir_all(directory.path().join("assets")).unwrap();
        fs::write(directory.path().join("assets/test.wav"), b"1234").unwrap();
        jobs.prepare_cache(
            "poison-audio",
            motion_model::AudioAsset {
                id: 1,
                path: "assets/test.wav".into(),
                bytes: 4,
                mime: "audio/wav".into(),
                track: 0,
                sample_rate: 48000,
                channels: 1,
                sample_frames: 48000,
                duration_us: 1_000_000,
            },
        )
        .unwrap();
    } else {
        jobs.start_with_source(
            move || gate.fail(),
            ImportOptions {
                request_id: "poison-audio".into(),
                at_frame: 0,
                track: None,
                name: "test".into(),
            },
            mode.starts_with("probe"),
        )
        .unwrap();
    }
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let task = jobs.task("poison-audio").unwrap();
    *observed.lock().unwrap() = Some(task.clone());
    let cancelled = mode.ends_with("cancel");
    if cancelled {
        jobs.cancel("poison-audio").unwrap();
    }
    poison(&task);
    drop(release);
    capture.first(
        if cache {
            "audio.cache.result_lock"
        } else {
            "audio.import.result_lock"
        },
        "poison-audio",
    );
    let state = recovered(&task);
    assert_eq!(
        state.status.state,
        if cancelled { "cancelled" } else { "failed" }
    );
    if !cancelled {
        assert_eq!(
            state.status.error.as_deref(),
            Some("controlled native failure")
        );
    }
    assert!(state.prepared.is_none());
    drop(state);
    assert_eq!(
        jobs.status("poison-audio").unwrap_err().to_string(),
        "audio task poisoned"
    );
    let errors = capture.matching("media error request=poison-audio");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].1.contains("controlled native failure"));
    assert!(errors[0].1.contains(if mode.starts_with("probe") {
        "operation=probe_audio"
    } else if cache {
        "operation=prepare_audio"
    } else {
        "operation=import_audio"
    }));
}

#[test]
fn audio_spawn_failure_keeps_native_error_when_terminal_lock_is_poisoned() {
    let Some(mode) = case(
        "jobs::poison_tests::audio_spawn_failure_keeps_native_error_when_terminal_lock_is_poisoned",
        &[
            "import",
            "probe",
            "cache",
            "import_poison",
            "probe_poison",
            "cache_poison",
        ],
    ) else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let jobs = Arc::new(AudioJobs::new(directory.path().into(), Limits::default()).unwrap());
    let operation = if mode.starts_with("probe") {
        "probe_audio"
    } else if mode.starts_with("cache") {
        "prepare_audio"
    } else {
        "import_audio"
    };
    let baseline = WORKERS.load(Ordering::Acquire);
    let (task, worker, initial) = jobs.reserve("spawn-audio", operation).unwrap();
    assert_eq!(WORKERS.load(Ordering::Acquire), baseline + 1);
    let poisoned = mode.ends_with("poison");
    if poisoned {
        poison(&task);
    }
    let target = task.clone();
    let registry = jobs.clone();
    let capture = Capture::install(move |text| {
        if text.contains("media poison recovery") || text.starts_with("media error") {
            unlocked(&target);
            unlocked(&registry.tasks);
        }
    });
    // Inject the OS-error result at the shared spawn-failure boundary. Do not
    // exhaust native threads just to force Builder::spawn to fail on CI.
    let failure = std::io::Error::from_raw_os_error(12345);
    let reason = failure.to_string();
    let expected_kind = failure.kind();
    let error = mark_spawn_failed(
        &task,
        crate::task_logging::TaskLog::new(&initial.request_id, &initial.operation),
        failure,
    );
    match error {
        crate::Error::Io(cause) => {
            assert_eq!(cause.raw_os_error(), Some(12345));
            assert_eq!(cause.kind(), expected_kind);
            assert_eq!(cause.to_string(), reason);
        }
        other => panic!("original spawn error replaced: {other:?}"),
    }
    let state = task.lock().unwrap_or_else(|error| error.into_inner());
    assert_eq!(state.status.state, "failed");
    assert_eq!(state.status.phase, "failed");
    assert_eq!(state.status.error.as_deref(), Some(reason.as_str()));
    assert!(state.prepared.is_none());
    drop(state);
    // The admission snapshot is independent of later task mutation/poison.
    assert_eq!(initial.request_id, "spawn-audio");
    assert_eq!(initial.operation, operation);
    assert_eq!(initial.state, "running");
    assert_eq!(initial.phase, "queued");
    assert!(initial.error.is_none());
    if poisoned {
        capture.first("audio.start.spawn_failure_task", "spawn-audio");
        assert_eq!(
            jobs.status("spawn-audio").err().unwrap().to_string(),
            "audio task poisoned"
        );
    } else {
        assert!(capture.matching("media poison recovery").is_empty());
        assert_eq!(jobs.status("spawn-audio").unwrap().state, "failed");
        jobs.release("spawn-audio").unwrap();
    }
    let errors = capture.matching("media error request=spawn-audio");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].0, log::Level::Error);
    assert!(errors[0]
        .1
        .contains(&format!("operation={operation} phase=spawn ")));
    assert!(errors[0].1.ends_with(&reason));
    drop(worker);
    assert_eq!(WORKERS.load(Ordering::Acquire), baseline);
}
