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
        let (task, _worker) = jobs.reserve("poison-audio", "test").unwrap();
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
