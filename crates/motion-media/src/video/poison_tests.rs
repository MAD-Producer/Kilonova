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
        let (task, _worker) = jobs.reserve("poison-video", "test").unwrap();
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
        jobs.status("poison-video").err().map(|error| error.to_string()).as_deref(),
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
