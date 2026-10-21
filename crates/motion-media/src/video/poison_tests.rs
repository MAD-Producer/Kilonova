use super::*;
use crate::poison_observation::tests::*;
use std::time::Duration;

#[test]
fn video_workers_and_drop_observe_real_poison_without_changing_outcomes() {
    let Some(mode) = case(
        "video::poison_tests::video_workers_and_drop_observe_real_poison_without_changing_outcomes",
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
    let jobs = VideoJobs::new(directory.path().into()).unwrap();
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
        let (task, _worker, _initial) = jobs.reserve("poison-video", "test").unwrap();
        *observed.lock().unwrap() = Some(task.clone());
        poison(&task);
        poison(&jobs.tasks);
        drop(jobs);
        capture.first("video.drop.registry", "session");
        capture.first("video.drop.task", "poison-video");
        assert!(recovered(&task).cancel.load(Ordering::Acquire));
        return;
    }
    let (gate, entered, release) = Gate::new();
    let cache = mode.starts_with("cache");
    if cache {
        fs::create_dir_all(directory.path().join("assets")).unwrap();
        fs::write(directory.path().join("assets/test.mp4"), b"1234").unwrap();
        let asset = serde_json::from_value(json!({
            "id":1,"path":"assets/test.mp4","bytes":4,"mime":"video/avc","track":0,
            "width":2,"height":2,"rotation":0,"display_width":2,"display_height":2,
            "video_start_us":0,"video_end_us":66666,"duration_us":66666,"frame_count":2,
            "variable_frame_rate":false,"nominal_frame_rate":30.0,"color_standard":1,
            "color_range":1,"audio_asset":null
        }))
        .unwrap();
        jobs.prepare_cache(
            "poison-video",
            asset,
            Arc::new(move |_, _, _, _| gate.fail()),
        )
        .unwrap();
    } else {
        jobs.start(
            move || gate.fail(),
            VideoImportOptions {
                request_id: "poison-video".into(),
                at_frame: 0,
                name: "test".into(),
                track: None,
                audio_track: None,
                with_audio: false,
            },
            mode.starts_with("probe"),
            Arc::new(|_, _, _, _| Err("unused probe".into())),
        )
        .unwrap();
    }
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let task = jobs.task("poison-video").unwrap();
    *observed.lock().unwrap() = Some(task.clone());
    let cancelled = mode.ends_with("cancel");
    if cancelled {
        jobs.cancel("poison-video").unwrap();
    }
    poison(&task);
    drop(release);
    capture.first(
        if cache {
            "video.cache.result_lock"
        } else {
            "video.import.result_lock"
        },
        "poison-video",
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
        jobs.status("poison-video")
            .err()
            .map(|error| error.to_string())
            .as_deref(),
        Some("video task poisoned")
    );
    let errors = capture.matching("media error request=poison-video");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].1.contains("controlled native failure"));
    assert!(errors[0].1.contains(if mode.starts_with("probe") {
        "operation=probe_video"
    } else if cache {
        "operation=prepare_video"
    } else {
        "operation=import_video"
    }));
}

#[test]
fn video_spawn_failure_keeps_native_error_when_terminal_lock_is_poisoned() {
    let Some(mode) = case(
        "video::poison_tests::video_spawn_failure_keeps_native_error_when_terminal_lock_is_poisoned",
        &["import", "probe", "cache", "import_poison", "probe_poison", "cache_poison"],
    ) else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let jobs = Arc::new(VideoJobs::new(directory.path().into()).unwrap());
    let operation = if mode.starts_with("probe") {
        "probe_video"
    } else if mode.starts_with("cache") {
        "prepare_video"
    } else {
        "import_video"
    };
    let baseline = WORKERS.load(Ordering::Acquire);
    let (task, worker, initial) = jobs.reserve("spawn-video", operation).unwrap();
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
    assert_eq!(initial.request_id, "spawn-video");
    assert_eq!(initial.operation, operation);
    assert_eq!(initial.state, "running");
    assert_eq!(initial.phase, "queued");
    assert!(initial.error.is_none());
    if poisoned {
        capture.first("video.start.spawn_failure_task", "spawn-video");
        assert_eq!(
            jobs.status("spawn-video").err().unwrap().to_string(),
            "video task poisoned"
        );
    } else {
        assert!(capture.matching("media poison recovery").is_empty());
        assert_eq!(jobs.status("spawn-video").unwrap().state, "failed");
        jobs.release("spawn-video").unwrap();
    }
    let errors = capture.matching("media error request=spawn-video");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].0, log::Level::Error);
    assert!(errors[0]
        .1
        .contains(&format!("operation={operation} phase=spawn ")));
    assert!(errors[0].1.ends_with(&reason));
    drop(worker);
    assert_eq!(WORKERS.load(Ordering::Acquire), baseline);
}
