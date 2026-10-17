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
