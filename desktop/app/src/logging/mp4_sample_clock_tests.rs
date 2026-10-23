use super::*;
use motion_media::{AudioJobs, ImportOptions, Limits};
use std::time::Instant;

fn invalid_run(bytes: &mut [u8]) {
    let mut at = 0;
    while at < bytes.len() {
        let size = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        assert!(size >= 8 && at + size <= bytes.len());
        let kind = &bytes[at + 4..at + 8];
        if kind == b"stts" {
            assert_eq!(size, 24);
            assert_eq!(&bytes[at + 12..at + 16], &1u32.to_be_bytes());
            bytes[at + 16..at + 20].copy_from_slice(&u32::MAX.to_be_bytes());
            return;
        }
        if matches!(kind, b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl") {
            invalid_run(&mut bytes[at + 8..at + size]);
        }
        at += size;
    }
}

#[test]
fn actual_mp4_clock_failure_keeps_default_sinks_and_recovers_without_panic() {
    const CHILD: &str = "MOTION_MP4_SAMPLE_CLOCK_TEST_CHILD";
    const TEST: &str = "logging::mp4_sample_clock_tests::actual_mp4_clock_failure_keeps_default_sinks_and_recovers_without_panic";
    const FAILED: &str = "mp4-clock-invalid";
    const RECOVERED: &str = "mp4-clock-recovered";
    const CAUSE: &str = "invalid MP4 sample clock track=0 atom=stts phase=preflight: timing and size sample counts disagree";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _guard = init(root.join("logs")).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/motion-media/tests/fixtures/tone-stereo-48000.m4a");
        let valid = fs::read(fixture).unwrap();
        let mut invalid = valid.clone();
        invalid_run(&mut invalid);
        assert_ne!(invalid, valid);
        let jobs = AudioJobs::new(root.clone(), Limits::default()).unwrap();
        for (request, bytes, expected) in
            [(FAILED, invalid, "failed"), (RECOVERED, valid, "succeeded")]
        {
            jobs.start(
                Box::new(std::io::Cursor::new(bytes)),
                None,
                ImportOptions {
                    request_id: request.into(),
                    name: "sample clock test".into(),
                    at_frame: 0,
                    track: Some(0),
                },
                true,
            )
            .unwrap();
            let began = Instant::now();
            loop {
                let status = jobs.status(request).unwrap();
                if status.state != "running" {
                    assert_eq!(status.state, expected, "{status:?}");
                    if request == FAILED {
                        assert_eq!(status.error.as_deref(), Some(CAUSE));
                        assert!(status.metadata.is_none());
                    } else {
                        assert!(status.error.is_none());
                        assert_eq!(status.metadata.unwrap().sample_frames, 96_000);
                    }
                    break;
                }
                assert!(began.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(5));
            }
            if request == FAILED {
                loop {
                    if fs::read_to_string(root.join("logs/motion-studio.log"))
                        .unwrap()
                        .contains(&format!("media error request={FAILED} "))
                    {
                        break;
                    }
                    assert!(began.elapsed() < Duration::from_secs(10));
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            jobs.release(request).unwrap();
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
    let text = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{stderr}\n{stdout}");
    assert!(text.contains("logging initialized level=INFO"));
    let start = format!("media start request={FAILED} ");
    let error = format!("media error request={FAILED} ");
    assert!(text.find(&start).unwrap() < text.find(&error).unwrap());
    assert_eq!(text.lines().filter(|line| line.contains(&start)).count(), 1);
    for sink in [&text, &stderr] {
        let failures: Vec<_> = sink.lines().filter(|line| line.contains(&error)).collect();
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("operation=probe_audio phase=failed "));
        assert!(failures[0].contains("ms: ") && failures[0].contains(CAUSE));
        assert!(!sink.contains("panic command=") && !sink.contains("worker panic"));
        assert!(!sink.contains(&format!("media error request={RECOVERED} ")));
    }
    assert!(!stderr.contains(&start));
    assert!(!stdout.contains(CAUSE) && !stdout.contains("media start request="));
}
