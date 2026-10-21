use super::*;
use motion_media::{AudioJobs, ImportOptions, Limits};
use std::time::Instant;

#[test]
fn real_media_io_and_codec_faults_keep_default_error_channels() {
    const CHILD: &str = "MOTION_TYPED_MEDIA_FILE_TEST_CHILD";
    const TEST: &str =
        "logging::media_error_tests::real_media_io_and_codec_faults_keep_default_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        let jobs = AudioJobs::new(root.clone(), Limits::default()).unwrap();
        let options = |request: &str| ImportOptions {
            request_id: request.into(),
            name: "failure fixture".into(),
            at_frame: 0,
            track: None,
        };
        let missing = root.join("missing.wav");
        jobs.start_with_source(
            motion_host::ops::media::file_opener(missing),
            options("typed-media-io"),
            false,
        )
        .unwrap();
        let invalid = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/motion-media/tests/fixtures/invalid.bin");
        jobs.start(
            Box::new(File::open(&invalid).unwrap()),
            None,
            options("typed-media-audio"),
            false,
        )
        .unwrap();
        let invalid_progress = AudioJobs::with_decoder(
            root.clone(),
            Limits::default(),
            std::sync::Arc::new({
                let first = std::sync::atomic::AtomicBool::new(true);
                move |source, pcm, track, limit, check| {
                    if first.swap(false, std::sync::atomic::Ordering::AcqRel) {
                        check(Some(f64::NAN))?;
                        panic!("non-finite progress must reject the decoder");
                    }
                    motion_media::decode_audio(source, pcm, track, limit, check)
                }
            }),
        ).unwrap();
        invalid_progress.start(
            Box::new(std::io::Cursor::new(vec![1; 16])),
            Some(16),
            options("typed-media-progress"),
            false,
        ).unwrap();
        for request in ["typed-media-io", "typed-media-audio", "typed-media-progress"] {
            let began = Instant::now();
            let status = loop {
                let status = if request == "typed-media-progress" {
                    invalid_progress.status(request).unwrap()
                } else {
                    jobs.status(request).unwrap()
                };
                if status.state != "running" {
                    break status;
                }
                assert!(began.elapsed() < Duration::from_secs(10), "{request}");
                std::thread::sleep(Duration::from_millis(5));
            };
            assert_eq!(status.state, "failed");
            let reason = status.error.as_ref().unwrap();
            assert!(serde_json::to_value(&status).unwrap()["error"].is_string());
            if request == "typed-media-io" {
                assert!(
                    reason.contains("open media ") && reason.contains("missing.wav"),
                    "{reason}"
                );
            } else if request == "typed-media-progress" {
                assert_eq!(reason, "invalid media task progress: expected a finite ratio or unknown");
                assert!(status.progress.is_none_or(f64::is_finite));
            } else {
                assert!(
                    reason.contains(
                        "unsupported feature: core (probe): no suitable format reader found"
                    ),
                    "{reason}"
                );
            }
            // Terminal state is published before the completion record; wait
            // for the actual native file sink, not just an in-memory status.
            loop {
                let text = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
                if text.contains(&format!("media error request={request}")) {
                    assert!(text.contains(reason));
                    break;
                }
                assert!(began.elapsed() < Duration::from_secs(10), "{request}");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let valid = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/motion-media/tests/fixtures/tone-stereo-48000.wav");
        invalid_progress.release("typed-media-progress").unwrap();
        invalid_progress.start(
            Box::new(File::open(valid).unwrap()),
            None,
            options("typed-media-progress-recovered"),
            true,
        ).unwrap();
        let began = Instant::now();
        loop {
            let status = invalid_progress.status("typed-media-progress-recovered").unwrap();
            if status.state != "running" {
                assert_eq!(status.state, "succeeded", "{status:?}");
                assert_eq!(status.progress, Some(1.0));
                assert!(status.error.is_none());
                break;
            }
            assert!(began.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(5));
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--test-threads=1"])
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
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("logging initialized level=INFO"));
    let recovery_error = "media error request=typed-media-progress-recovered ";
    assert!(!text.contains(recovery_error));
    assert!(!stderr.contains(recovery_error));
    assert!(!stdout.contains(recovery_error));
    for request in ["typed-media-io", "typed-media-audio", "typed-media-progress"] {
        let marker = format!("media error request={request} ");
        let file: Vec<_> = text.lines().filter(|line| line.contains(&marker)).collect();
        let console: Vec<_> = stderr
            .lines()
            .filter(|line| line.contains(&marker))
            .collect();
        assert_eq!(file.len(), 1);
        assert_eq!(console.len(), 1);
        assert!(
            file[0].contains("operation=import_audio phase=failed ") && file[0].contains("ms: ")
        );
        assert_eq!(
            file[0].split_once(&marker).unwrap().1,
            console[0].split_once(&marker).unwrap().1
        );
        assert!(!stdout.contains(&marker));
        let start = format!("media start request={request} ");
        assert_eq!(text.lines().filter(|line| line.contains(&start)).count(), 1);
        assert!(!stderr.contains(&start) && !stdout.contains(&start));
    }
}
