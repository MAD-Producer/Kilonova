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
        for request in ["typed-media-io", "typed-media-audio"] {
            let began = Instant::now();
            let status = loop {
                let status = jobs.status(request).unwrap();
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
    for request in ["typed-media-io", "typed-media-audio"] {
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
