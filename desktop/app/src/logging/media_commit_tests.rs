use super::*;
use motion_core::{Engine, Project, VideoAsset};
use motion_media::{AudioJobs, ImportOptions, Limits, VideoImportOptions, VideoJobs, VideoProbe};
use std::{path::Path, sync::Arc, time::Instant};

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures")
        .join(relative)
}

fn wait_ready(mut state: impl FnMut() -> String) {
    let began = Instant::now();
    loop {
        let value = state();
        if value != "running" {
            assert_eq!(value, "ready");
            return;
        }
        assert!(began.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn audio(root: &Path) -> AudioJobs {
    let jobs = AudioJobs::new(root.into(), Limits::default()).unwrap();
    jobs.start(
        Box::new(File::open(fixture("tone-stereo-48000.wav")).unwrap()),
        None,
        ImportOptions {
            request_id: "commit-observation-audio".into(),
            at_frame: 0,
            track: None,
            name: "test".into(),
        },
        false,
    )
    .unwrap();
    wait_ready(|| jobs.status("commit-observation-audio").unwrap().state);
    jobs
}

fn video(root: &Path) -> VideoJobs {
    let jobs = VideoJobs::new(root.into()).unwrap();
    jobs.start(
        || {
            let source = File::open(fixture("video/sound-24fps.mp4")).unwrap();
            let bytes = source.metadata().unwrap().len();
            Ok((Box::new(source), Some(bytes)))
        },
        VideoImportOptions {
            request_id: "commit-observation-video".into(),
            at_frame: 0,
            name: "test".into(),
            track: None,
            audio_track: None,
            with_audio: false,
        },
        false,
        // Only probing is supplied. Staging, cache writes, commit, publication
        // failure and rollback below execute the actual media implementation.
        Arc::new(|source, _, _, check| {
            check()?;
            Ok(VideoProbe {
                asset: VideoAsset {
                    id: 1,
                    path: "assets/probe.mp4".into(),
                    bytes: fs::metadata(source).unwrap().len(),
                    mime: "video/avc".into(),
                    track: 0,
                    width: 64,
                    height: 64,
                    rotation: 0,
                    display_width: 64,
                    display_height: 64,
                    video_start_us: 0,
                    video_end_us: 33_333,
                    duration_us: 33_333,
                    frame_count: 1,
                    variable_frame_rate: false,
                    nominal_frame_rate: 30.,
                    color_standard: 1,
                    color_range: 2,
                    audio_asset: None,
                },
                timestamps: vec![0],
                audio_track: None,
                first_rgba: vec![255; 64 * 64 * 4],
                tracks: serde_json::json!([]),
            })
        }),
    )
    .unwrap();
    wait_ready(|| jobs.status("commit-observation-video").unwrap().state);
    jobs
}

#[test]
fn media_commit_ok_failures_reach_default_file_and_console_without_replay_errors() {
    const CHILD: &str = "MOTION_MEDIA_COMMIT_FILE_TEST_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        for kind in ["audio", "video"] {
            let project_root = root.join(kind);
            fs::create_dir_all(&project_root).unwrap();
            let mut engine = Engine::new(Project::new(64, 64, 30, 90).unwrap()).unwrap();
            let before = engine.snapshot();
            let id = format!("commit-observation-{kind}");
            let audio = (kind == "audio").then(|| audio(&project_root));
            let video = (kind == "video").then(|| video(&project_root));
            let blocked = project_root.join("assets");
            fs::write(&blocked, b"original asset directory blocker").unwrap();
            let reason = fs::create_dir_all(&blocked).unwrap_err().to_string();
            for _ in 0..120 {
                let (state, error) = if let Some(jobs) = &audio {
                    let status = jobs.commit(&id, &mut engine).unwrap();
                    (status.state, status.error)
                } else {
                    let status = video.as_ref().unwrap().commit(&id, &mut engine).unwrap();
                    (status.state, status.error)
                };
                assert_eq!(state, "failed");
                assert_eq!(error.as_deref(), Some(reason.as_str()));
            }
            assert_eq!(engine.snapshot(), before);
            assert!(!engine.can_undo());
            assert_eq!(
                fs::read(&blocked).unwrap(),
                b"original asset directory blocker"
            );
            // Repair the external obstruction and use a fresh import. The old
            // terminal result remains failed; observations never retry it.
            fs::remove_file(&blocked).unwrap();
            if kind == "audio" {
                let jobs = self::audio(&project_root);
                assert_eq!(jobs.commit(&id, &mut engine).unwrap().state, "succeeded");
            } else {
                let jobs = self::video(&project_root);
                assert_eq!(jobs.commit(&id, &mut engine).unwrap().state, "succeeded");
            }
            let text = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
            assert!(text.contains(&reason));
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "logging::media_commit_tests::media_commit_ok_failures_reach_default_file_and_console_without_replay_errors",
        ])
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
    assert!(text.contains("logging initialized level=INFO"));
    let stderr = String::from_utf8(output.stderr).unwrap();
    for kind in ["audio", "video"] {
        let operation = format!("operation=commit_{kind}");
        let errors: Vec<_> = text
            .lines()
            .filter(|line| line.contains(" ERROR ") && line.contains(&operation))
            .collect();
        assert_eq!(
            errors.len(),
            1,
            "a terminal failure must not be logged again on replay"
        );
        assert!(errors[0].contains(&format!("request=commit-observation-{kind}")));
        assert!(errors[0].contains("phase=failed ") && errors[0].contains("ms: "));
        assert_eq!(
            stderr
                .lines()
                .filter(|line| line.contains(&operation))
                .count(),
            1
        );
        assert_eq!(
            text.lines()
                .filter(|line| line.contains(" INFO ")
                    && line.contains("media start ")
                    && line.contains(&operation))
                .count(),
            121
        );
    }
    assert!(!stderr.contains("media start "));
    assert!(!String::from_utf8(output.stdout).unwrap().contains("media "));
}

#[test]
fn media_teardown_cleanup_warnings_reach_default_file_without_console_noise() {
    const CHILD: &str = "MOTION_MEDIA_TEARDOWN_FILE_TEST_CHILD";
    const TEST: &str = "logging::media_commit_tests::media_teardown_cleanup_warnings_reach_default_file_without_console_noise";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        // Import jobs canonicalize their root. Exercise an alias explicitly so
        // log matching does not depend on the runner's temporary-path spelling
        // (for example, a Windows short name versus its canonical long name).
        let alias = root.join("path-alias");
        fs::create_dir_all(&alias).unwrap();
        for kind in ["audio", "video"] {
            let project_root = alias.join("..").join(kind);
            fs::create_dir_all(&project_root).unwrap();
            let project_root = project_root.canonicalize().unwrap();
            let audio = (kind == "audio").then(|| audio(&project_root));
            let video = (kind == "video").then(|| video(&project_root));
            let prefix = if kind == "audio" {
                ".media-import-"
            } else {
                ".video-import-"
            };
            let stages: Vec<_> = fs::read_dir(&project_root)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(prefix)
                })
                .collect();
            assert_eq!(stages.len(), 1);
            let stage = &stages[0];
            assert!(stage
                .canonicalize()
                .unwrap()
                .starts_with(project_root.canonicalize().unwrap()));
            // The actual import worker retained this tree for commit. Replace
            // only that test-owned tree with a file to force a real OS failure.
            fs::remove_dir_all(stage).unwrap();
            fs::write(stage, b"foreign file that cleanup must preserve").unwrap();
            drop(audio);
            drop(video);
            assert_eq!(
                fs::read(stage).unwrap(),
                b"foreign file that cleanup must preserve"
            );
            let text = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
            let warnings: Vec<_> = text
                .lines()
                .filter(|line| {
                    line.contains("media stage cleanup failed")
                        && line.contains(&stage.display().to_string())
                })
                .collect();
            assert_eq!(warnings.len(), 1, "stage={} logs={text}", stage.display());
            assert!(warnings[0].contains(" WARN ") && warnings[0].contains("reason="));
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    assert!(text.contains("logging initialized level=INFO"));
    assert_eq!(text.matches("media stage cleanup failed").count(), 2);
    assert!(
        !text.contains(" ERROR "),
        "cleanup remains best effort: {text}"
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("media "));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("media "));
}

struct PanickingImportSink;
impl motion_model::EditSink for PanickingImportSink {
    fn project(&self) -> &Project {
        panic!("controlled native import provider panic");
    }
    fn apply_batch_saved(
        &mut self,
        _: Vec<motion_core::Command>,
        _: &Path,
    ) -> motion_core::Result<Vec<motion_core::EditResult>> {
        panic!("must not publish after a provider panic");
    }
}

struct FailingImportSave(Project);
impl motion_model::EditSink for FailingImportSave {
    fn project(&self) -> &Project {
        &self.0
    }
    fn apply_batch_saved(
        &mut self,
        _: Vec<motion_core::Command>,
        root: &Path,
    ) -> motion_core::Result<Vec<motion_core::EditResult>> {
        let files: Vec<_> = fs::read_dir(root.join("assets"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(files.len(), 1);
        // A provider failure and a concurrent filesystem obstruction together
        // exercise the real import rollback, including its secondary failure.
        assert!(files[0]
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap()));
        fs::remove_file(&files[0]).unwrap();
        fs::create_dir(&files[0]).unwrap();
        Err(motion_core::Error::Invalid(
            "controlled import save failure".into(),
        ))
    }
}

#[test]
fn import_rollback_keeps_primary_and_secondary_causes_in_default_channels() {
    const CHILD: &str = "MOTION_MEDIA_ROLLBACK_FILE_TEST_CHILD";
    const TEST: &str = "logging::media_commit_tests::import_rollback_keeps_primary_and_secondary_causes_in_default_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        for kind in ["audio", "video"] {
            let project_root = root.join(kind);
            fs::create_dir_all(&project_root).unwrap();
            let mut sink = FailingImportSave(Project::new(64, 64, 30, 90).unwrap());
            let before = serde_json::to_value(&sink.0).unwrap();
            let id = format!("commit-observation-{kind}");
            let error = if kind == "audio" {
                audio(&project_root)
                    .commit(&id, &mut sink)
                    .unwrap()
                    .error
                    .unwrap()
            } else {
                video(&project_root)
                    .commit(&id, &mut sink)
                    .unwrap()
                    .error
                    .unwrap()
            };
            assert!(
                error.starts_with("controlled import save failure; rollback remove "),
                "{error}"
            );
            assert_eq!(serde_json::to_value(&sink.0).unwrap(), before);
            let retained: Vec<_> = fs::read_dir(project_root.join("assets"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            assert_eq!(retained.len(), 1);
            assert!(
                retained[0].is_dir(),
                "rollback must not delete a foreign directory"
            );
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("logging initialized level=INFO"));
    let warnings: Vec<_> = text
        .lines()
        .filter(|line| line.contains(" WARN ") && line.contains("rollback remove "))
        .collect();
    assert_eq!(warnings.len(), 2);
    for kind in ["audio", "video"] {
        let marker = format!("operation=commit_{kind} phase=failed");
        let errors: Vec<_> = text
            .lines()
            .filter(|line| line.contains(" ERROR ") && line.contains(&marker))
            .collect();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("controlled import save failure; rollback remove "));
        let console: Vec<_> = stderr
            .lines()
            .filter(|line| line.contains(&marker))
            .collect();
        assert_eq!(console.len(), 1);
        assert!(console[0].contains("controlled import save failure; rollback remove "));
    }
    assert!(!stdout.contains("rollback remove ") && !stdout.contains("media "));
}

#[test]
fn media_resource_cleanup_and_poisoned_reservation_preserve_default_channels() {
    const CHILD: &str = "MOTION_MEDIA_RESOURCE_FILE_TEST_CHILD";
    const TEST: &str = "logging::media_commit_tests::media_resource_cleanup_and_poisoned_reservation_preserve_default_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        for mode in ["cancel", "commit", "panic"] {
            for kind in ["audio", "video"] {
                let project_root = root.join(format!("{mode}-{kind}"));
                fs::create_dir_all(&project_root).unwrap();
                let audio = (kind == "audio").then(|| audio(&project_root));
                let video = (kind == "video").then(|| video(&project_root));
                let id = format!("commit-observation-{kind}");
                let audio_metadata = audio
                    .as_ref()
                    .map(|jobs| jobs.status(&id).unwrap().metadata.unwrap());
                let video_metadata = video.as_ref().map(|jobs| {
                    serde_json::from_value::<VideoAsset>(
                        jobs.status(&id).unwrap().metadata.unwrap()["video"].clone(),
                    )
                    .unwrap()
                });
                let prefix = if kind == "audio" {
                    ".media-import-"
                } else {
                    ".video-import-"
                };
                let stages: Vec<_> = fs::read_dir(&project_root)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .filter(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with(prefix)
                    })
                    .collect();
                assert_eq!(stages.len(), 1);
                let stage = &stages[0];
                assert!(stage
                    .canonicalize()
                    .unwrap()
                    .starts_with(project_root.canonicalize().unwrap()));
                fs::remove_dir_all(stage).unwrap();
                fs::write(stage, b"foreign file").unwrap();
                if mode == "cancel" {
                    let state = if let Some(jobs) = &audio {
                        jobs.cancel(&id).unwrap().state
                    } else {
                        video.as_ref().unwrap().cancel(&id).unwrap().state
                    };
                    assert_eq!(state, "cancelled");
                } else if mode == "commit" {
                    let mut engine = Engine::new(Project::new(64, 64, 30, 90).unwrap()).unwrap();
                    let (state, error) = if let Some(jobs) = &audio {
                        let status = jobs.commit(&id, &mut engine).unwrap();
                        (status.state, status.error)
                    } else {
                        let status = video.as_ref().unwrap().commit(&id, &mut engine).unwrap();
                        (status.state, status.error)
                    };
                    assert_eq!(state, "failed");
                    assert!(error.is_some());
                    assert!(engine.project().layers.is_empty());
                } else {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        if let Some(jobs) = &audio {
                            let _ = jobs.commit(&id, &mut PanickingImportSink);
                        } else {
                            let _ = video
                                .as_ref()
                                .unwrap()
                                .commit(&id, &mut PanickingImportSink);
                        }
                    }));
                    assert_eq!(
                        motion_media::panic_reason(result.unwrap_err()),
                        "controlled native import provider panic"
                    );
                    let error = if let Some(jobs) = &audio {
                        jobs.prepare_cache("next", audio_metadata.unwrap()).err()
                    } else {
                        video
                            .as_ref()
                            .unwrap()
                            .prepare_cache(
                                "next",
                                video_metadata.unwrap(),
                                Arc::new(|_, _, _, _| {
                                    panic!("poisoned reservation must not invoke the decoder")
                                }),
                            )
                            .err()
                    };
                    assert_eq!(
                        error.map(|error| error.to_string()).as_deref(),
                        Some(format!("{kind} task poisoned").as_str())
                    );
                }
                drop(audio);
                drop(video);
                assert_eq!(fs::read(stage).unwrap(), b"foreign file");
            }
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("logging initialized level=INFO"));
    assert_eq!(text.matches("media stage cleanup failed").count(), 6);
    assert!(!stderr.contains("media stage cleanup failed"));
    assert!(!stdout.contains("media "));
    assert_eq!(
        text.matches("controlled native import provider panic")
            .count(),
        2
    );
    assert_eq!(
        stderr
            .matches("controlled native import provider panic")
            .count(),
        2
    );
    assert!(!stdout.contains("controlled native import provider panic"));
    assert!(!text.contains("PoisonError") && !stderr.contains("PoisonError"));
    for kind in ["audio", "video"] {
        let marker = format!("operation=commit_{kind} phase=failed");
        assert_eq!(text.matches(&marker).count(), 1);
        assert_eq!(stderr.matches(&marker).count(), 1);
        assert!(!stdout.contains(&marker));
    }
}
