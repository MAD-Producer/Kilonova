use crate::audio_output::*;
use std::{fs, sync::mpsc, thread};

#[test]
fn mapped_pcm_read_failure_reaches_default_file_and_stderr_after_a_published_start() {
    const CHILD: &str = "MOTION_MAPPED_PCM_FAILURE_CHILD";
    const TEST: &str = "audio_output::producer_observation::tests::mapped_pcm_read_failure_reaches_default_file_and_stderr_after_a_published_start";
    if let Some(root) = std::env::var_os(CHILD) {
        use motion_core::{
            layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
            AudioAsset, AudioClip, Content, Layer, Project,
        };
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let mut project = Project::new(32, 32, 10, 1).unwrap();
        let asset = AudioAsset {
            id: 83,
            path: "assets/controlled.wav".into(),
            mime: "audio/wav".into(),
            bytes: 1,
            track: 0,
            sample_rate: 48_000,
            channels: 1,
            sample_frames: 4_800,
            duration_us: 100_000,
        };
        let cache = motion_media::cache_path(&root, &asset).unwrap();
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(&cache, 0.25f32.to_le_bytes().repeat(4_800)).unwrap();
        project.audio_assets.push(asset);
        let mut layer = Layer::solid(7, "Controlled mapped PCM", [0.; 2], [0.; 3], [1.; 4]);
        layer.content = Content::Audio {
            audio: AudioClip::new(83),
        };
        layer.time_mapping = Some(LayerTimeMapping {
            clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 1.).unwrap(),
            remap: None,
        });
        project.layers.push(layer);
        let mut mixer = motion_media::AudioMixer::new(project, &root).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&cache)
            .unwrap()
            .set_len(0)
            .unwrap();
        let shared = Arc::new(Shared::new());
        let (entered, entry) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let release = Release(release);
        let worker = spawn_producer(shared.clone(), move || {
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(15)).unwrap();
            let error = mixer.mix(0, &mut [0.; 20]).unwrap_err();
            match error.primary() {
                motion_media::Error::Io(cause) => {
                    assert_eq!(cause.kind(), std::io::ErrorKind::UnexpectedEof)
                }
                other => panic!("lost original mapped PCM cause: {other:?}"),
            }
            Err(error.to_string())
        })
        .unwrap();
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        let prefix = format!(
            "audio mixer start playback={} operation=audio_mix",
            shared.observation
        );
        let before = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert!(before.contains(&prefix), "{before}");
        assert!(!before.contains("audio mixer failed playback="));
        drop(release);
        finish(worker);
        assert!(shared.stop.load(Ordering::Acquire));
        assert!(shared.ring.pop().is_none());
        let original = shared.error.lock().unwrap().clone().unwrap();
        for field in [
            "audio mix: audio mapped cache read:",
            "asset=83",
            "first_sample=",
            "sample_frames=",
            "output_begin=0",
            "output_end=10",
        ] {
            assert!(original.contains(field), "{original}");
        }
        let after = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let errors: Vec<_> = after
            .lines()
            .filter(|line| line.contains(" ERROR "))
            .collect();
        assert_eq!(errors.len(), 1, "{after}");
        assert!(errors[0].contains(&original), "{after}");
        assert!(
            errors[0].contains("operation=audio_mix") && errors[0].contains("ms phase=mix_error")
        );
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
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("audio mixer failed playback=").count(),
        1,
        "{stderr}"
    );
    assert!(
        stderr.contains("asset=83")
            && stderr.contains("output_begin=0")
            && stderr.contains("output_end=10")
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("audio mapped cache"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("audio mixer start"));
    if let Some(destination) = std::env::var_os("MOTION_TIME_AUDIO_EVIDENCE") {
        let destination = std::path::PathBuf::from(destination).join("producer");
        fs::create_dir_all(&destination).unwrap();
        fs::copy(
            root.path().join("motion-studio.log"),
            destination.join("motion-studio.log"),
        )
        .unwrap();
        fs::write(destination.join("stderr.txt"), &output.stderr).unwrap();
        fs::write(destination.join("stdout.txt"), &output.stdout).unwrap();
    }
}

fn finish(worker: JoinHandle<()>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "mixer observation deadline exceeded"
        );
        thread::sleep(Duration::from_millis(5));
    }
    worker.join().unwrap();
}

struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn start_precedes_blocking_read_and_all_producer_exits_remain_observable() {
    const CHILD: &str = "MOTION_MIXER_LIFECYCLE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let stopped = Arc::new(Shared::new());
        stopped.stop.store(true, Ordering::Release);
        finish(spawn_producer(stopped.clone(), || panic!("stopped producer read source")).unwrap());
        assert!(!stopped.done.load(Ordering::Acquire));

        let eof = Arc::new(Shared::new());
        finish(spawn_producer(eof.clone(), || Ok(None)).unwrap());
        assert!(eof.done.load(Ordering::Acquire) && eof.ready.load(Ordering::Acquire));
        assert!(!eof.stop.load(Ordering::Acquire));

        let blocked = Arc::new(Shared::new());
        let (entered, entry) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let release = Release(release);
        let worker = spawn_producer(blocked.clone(), move || {
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(15)).unwrap();
            Ok(Some([0.25, -0.25]))
        })
        .unwrap();
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        // No flush on the test thread: start must be durable before next().
        let before = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let prefix = format!(
            "audio mixer start playback={} operation=audio_mix",
            blocked.observation
        );
        assert!(before.contains(&prefix), "{before}");
        assert!(!before.contains(&format!(
            "audio mixer stopped playback={}",
            blocked.observation
        )));
        blocked.stop.store(true, Ordering::Release);
        drop(release);
        finish(worker);
        assert!(blocked.ring.pop().is_none());
        assert!(!blocked.done.load(Ordering::Acquire));

        // Fill the real bounded queue. Thousands of frames still produce only
        // lifecycle records, and a stopped output does not refill it.
        let full = Arc::new(Shared::new());
        let (entered, entry) = mpsc::channel();
        let mut reads = 0;
        let worker = spawn_producer(full.clone(), move || {
            reads += 1;
            if reads == QUEUE_FRAMES + 1 {
                entered.send(()).unwrap();
            }
            Ok(Some([0.5, -0.5]))
        })
        .unwrap();
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        full.stop.store(true, Ordering::Release);
        worker.thread().unpark();
        finish(worker);
        assert!(full.ready.load(Ordering::Acquire));

        let failed = Arc::new(Shared::new());
        finish(
            spawn_producer(failed.clone(), || Err("controlled PCM read failed".into())).unwrap(),
        );
        assert_eq!(
            failed.error.lock().unwrap().as_deref(),
            Some("audio mix: controlled PCM read failed")
        );
        let panicked = Arc::new(Shared::new());
        finish(
            spawn_producer(panicked.clone(), || panic!("controlled mixer source panic")).unwrap(),
        );
        assert_eq!(
            panicked.error.lock().unwrap().as_deref(),
            Some("audio mixer worker panicked: controlled mixer source panic")
        );

        let text = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for (state, reason) in [
            (&stopped, "stop"),
            (&eof, "eof"),
            (&blocked, "stop_after_read"),
            (&full, "stop_after_read"),
            (&failed, "mix_error"),
            (&panicked, "panic"),
        ] {
            let line = text
                .lines()
                .find(|line| {
                    line.contains(&format!(
                        "audio mixer stopped playback={}",
                        state.observation
                    ))
                })
                .unwrap();
            assert!(line.contains(&format!("ms reason={reason}")), "{line}");
        }
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("audio mixer start playback="))
                .count(),
            6
        );
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("audio mixer stopped playback="))
                .count(),
            6
        );
        let failures: Vec<_> = text
            .lines()
            .filter(|line| line.contains("audio mixer failed playback="))
            .collect();
        assert_eq!(failures.len(), 2);
        assert!(
            failures.iter().all(|line| line.contains(" ERROR ")
                && line.contains("operation=audio_mix")
                && line.contains("ms phase=")),
            "{text}"
        );
        assert!(failures
            .iter()
            .any(|line| line
                .contains("phase=mix_error reason=audio mix: controlled PCM read failed")));
        assert!(failures.iter().any(|line| line.contains(
            "phase=panic reason=audio mixer worker panicked: controlled mixer source panic"
        )));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "audio_output::producer_observation::tests::start_precedes_blocking_read_and_all_producer_exits_remain_observable"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("audio mixer failed playback=").count(),
        2,
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("audio mixer "));
}
