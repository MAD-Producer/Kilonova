//! Actual asynchronous IO failure through the production default-level sinks.
use motion_media::{AudioJobs, DecodeAudio, ImportOptions, Limits};
use std::{
    fs,
    path::PathBuf,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

#[test]
fn media_provider_error_is_bounded_in_logs_but_complete_in_task_replies() {
    const CHILD: &str = "MOTION_MEDIA_TASK_BOUNDS_CHILD";
    const TEST: &str = "logging::media_task_bounds_tests::media_provider_error_is_bounded_in_logs_but_complete_in_task_replies";
    const ID: &str = "media-bounds\nforged\u{1b}";
    const SAFE_ID: &str = "media-bounds�forged�";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = super::init(root.clone()).unwrap();
        let decode: DecodeAudio =
            Arc::new(|_, _, _, _, _| panic!("opening must fail before decoding"));
        let jobs = AudioJobs::with_decoder(root.clone(), Limits::default(), decode).unwrap();
        let missing = root.join("missing-source.wav");
        let prefix = format!(
            "native-open\r\nERROR forged \u{1b} \u{85}\u{2028}\u{2029} {}TAIL-MARKER",
            "界".repeat(6000)
        );
        let (entered, entry) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let (expected, expectation) = mpsc::channel();
        jobs.start_with_source(
            move || {
                entered.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(15)).unwrap();
                let cause = fs::File::open(&missing).unwrap_err();
                assert_eq!(cause.kind(), std::io::ErrorKind::NotFound);
                let error = motion_media::Error::from(cause).context(prefix);
                expected.send(error.to_string()).unwrap();
                Err(error)
            },
            ImportOptions {
                request_id: ID.into(),
                at_frame: 0,
                track: None,
                name: "source".into(),
            },
            false,
        )
        .unwrap();
        entry.recv_timeout(Duration::from_secs(15)).unwrap();
        // A start record is flushed before the provider can return or even do IO.
        let before = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert!(before.contains(&format!(
            "media start request={SAFE_ID} operation=import_audio"
        )));
        assert!(!before.contains("media error request="));
        assert_eq!(jobs.status(ID).unwrap().phase, "opening");
        release.send(()).unwrap();
        let original = expectation.recv_timeout(Duration::from_secs(15)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let status = jobs.status(ID).unwrap();
            if status.state == "failed" {
                assert_eq!(status.error.as_deref(), Some(original.as_str()));
                assert_eq!(status.request_id, ID);
                let text = fs::read_to_string(root.join("motion-studio.log")).unwrap();
                if text.contains("media error request=") {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "task failure did not reach both state and log"
            );
            std::thread::yield_now();
        }
        // Polling terminal state does not emit the error again, or replace it.
        for _ in 0..60 {
            let reply = serde_json::to_value(jobs.status(ID).unwrap()).unwrap();
            assert_eq!(reply["request_id"], ID);
            assert_eq!(reply["error"], original);
        }
        jobs.release(ID).unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
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
    let file = fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let starts: Vec<_> = file
        .lines()
        .filter(|line| line.contains("media start request="))
        .collect();
    assert_eq!(starts.len(), 1);
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("media error request="))
        .collect();
    assert_eq!(errors.len(), 1, "{file}");
    let error = errors[0]
        .split("motion_media::task_logging: ")
        .nth(1)
        .unwrap();
    assert!(error.starts_with(&format!(
        "media error request={SAFE_ID} operation=import_audio phase=failed "
    )));
    let cause = error.split_once("ms: ").unwrap().1;
    assert!(cause.starts_with("native-open��ERROR forged � ��� "));
    assert_eq!(cause.chars().count(), 2048);
    assert!(cause.contains('…'));
    assert!(cause.contains("TAIL-MARKER: "));
    assert!(
        cause.ends_with("(os error 2)"),
        "native cause lost: {cause}"
    );
    assert!(!cause.chars().any(char::is_control));
    assert!(!cause.contains(['\u{2028}', '\u{2029}']));
    assert!(file.find(starts[0]).unwrap() < file.find(errors[0]).unwrap());
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.contains(error));
    assert!(!stderr.contains("media start"));
    for marker in [
        "media-bounds",
        "native-open",
        "界",
        "TAIL-MARKER",
        "logging initialized",
    ] {
        assert!(!stdout.contains(marker), "log leaked to stdout: {stdout}");
    }
}
