use crate::audio_output::*;
use std::{fs, sync::mpsc};

#[test]
fn mapped_rate_overflow_publishes_start_then_uses_default_error_outputs_and_recovers() {
    const CHILD: &str = "MOTION_MAPPED_RATE_FAILURE_CHILD";
    const TEST: &str = "audio_output::producer_observation::mapped_rate_tests::mapped_rate_overflow_publishes_start_then_uses_default_error_outputs_and_recovers";
    if let Some(root) = std::env::var_os(CHILD) {
        use motion_core::{
            layer_time::{LayerTimeMapping, TimeRemap, TimeRemapKey},
            AudioAsset, AudioClip, Content, Layer, Project,
        };
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let mut project = Project::new(32, 32, 30, 3).unwrap();
        let asset = AudioAsset {
            id: 83,
            path: "assets/rate.wav".into(),
            mime: "audio/wav".into(),
            bytes: 1,
            track: 0,
            sample_rate: 192_000,
            channels: 1,
            sample_frames: 19_200,
            duration_us: 100_000,
        };
        let path = motion_media::cache_path(&root, &asset).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, 0.125f32.to_le_bytes().repeat(19_200)).unwrap();
        project.audio_assets.push(asset);
        let mut layer = Layer::solid(7, "Rate guard", [0.; 2], [0.; 3], [1.; 4]);
        layer.content = Content::Audio {
            audio: AudioClip::new(83),
        };
        layer.time_mapping = Some(LayerTimeMapping {
            clock: Default::default(),
            remap: Some(
                TimeRemap::new(
                    [(0., 0.), (1e-308, 1.)]
                        .into_iter()
                        .map(|(frame, source_frame)| TimeRemapKey {
                            frame,
                            source_frame,
                            easing: Default::default(),
                        })
                        .collect(),
                )
                .unwrap(),
            ),
        });
        project.layers.push(layer);
        let mut mixer = motion_media::AudioMixer::new(project.clone(), &root).unwrap();
        let shared = Arc::new(Shared::new());
        let (entered, entry) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let worker = spawn_producer(shared.clone(), move || {
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(15)).unwrap();
            let error = mixer.mix(0, &mut [0.; 20]).unwrap_err();
            assert!(error
                .primary()
                .to_string()
                .contains("native PCM derivative is non-finite"));
            Err(error.to_string())
        })
        .unwrap();
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        let before = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert!(before.contains(&format!(
            "audio mixer start playback={} operation=audio_mix",
            shared.observation
        )));
        assert!(!before.contains(" ERROR "));
        release.send(()).unwrap();
        let (ended, done) = mpsc::channel();
        std::thread::spawn(move || ended.send(worker.join()).unwrap());
        done.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        assert!(shared.stop.load(Ordering::Acquire));
        assert!(shared.ring.pop().is_none());
        let error = shared.error.lock().unwrap().clone().unwrap();
        for context in [
            "asset=83",
            "output_sample=0",
            "phase=map",
            "native PCM derivative is non-finite",
        ] {
            assert!(error.contains(context), "{error}");
        }
        let text = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let failures: Vec<_> = text
            .lines()
            .filter(|line| line.contains(" ERROR "))
            .collect();
        assert_eq!(failures.len(), 1, "{text}");
        assert!(failures[0].contains(&error));
        assert!(
            failures[0].contains("operation=audio_mix")
                && failures[0].contains("ms phase=mix_error")
        );
        // A new valid project snapshot uses the same real mixer and producer.
        project.layers[0].time_mapping = None;
        let mut mixer = motion_media::AudioMixer::new(project, &root).unwrap();
        let healthy = Arc::new(Shared::new());
        let mut emitted = false;
        let worker = spawn_producer(healthy.clone(), move || {
            if emitted {
                return Ok(None);
            }
            let mut pcm = [0.; 20];
            mixer
                .mix(128, &mut pcm)
                .map_err(|error| error.to_string())?;
            assert!(pcm.iter().all(|value| (*value - 0.125).abs() < 1e-6));
            emitted = true;
            Ok(Some([pcm[0], pcm[1]]))
        })
        .unwrap();
        worker.join().unwrap();
        assert!(healthy.error.lock().unwrap().is_none());
        assert!(healthy.done.load(Ordering::Acquire));
        assert!(healthy.ring.pop().is_some());
        let after = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert_eq!(after.matches("audio mixer start playback=").count(), 2);
        assert_eq!(after.matches(" ERROR ").count(), 1);
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
    assert!(stderr.contains("native PCM derivative is non-finite") && stderr.contains("asset=83"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("native PCM derivative") && !stdout.contains("audio mixer "));
    if let Some(destination) = std::env::var_os("MOTION_MAPPED_RATE_EVIDENCE") {
        let destination = std::path::PathBuf::from(destination);
        fs::create_dir_all(&destination).unwrap();
        fs::copy(
            root.path().join("motion-studio.log"),
            destination.join("motion-studio.log"),
        )
        .unwrap();
        fs::write(destination.join("stdout.txt"), &output.stdout).unwrap();
        fs::write(destination.join("stderr.txt"), &output.stderr).unwrap();
    }
}
