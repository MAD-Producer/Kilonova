use super::*;
use motion_media::{AudioJobs, ImportOptions, Limits};
use std::time::Instant;

fn change(bytes: &mut [u8], target: &[u8; 4], field: usize, value: u32) -> bool {
    let mut at = 0;
    while at < bytes.len() {
        let size = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        assert!(size >= 8 && at + size <= bytes.len());
        let kind = &bytes[at + 4..at + 8];
        if kind == target {
            bytes[at + 8 + field..at + 12 + field].copy_from_slice(&value.to_be_bytes());
            return true;
        }
        if matches!(kind, b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl")
            && change(&mut bytes[at + 8..at + size], target, field, value)
        {
            return true;
        }
        at += size;
    }
    false
}

#[test]
fn actual_mp4_mapping_failures_reach_default_sinks_and_the_worker_recovers() {
    const CHILD: &str = "MOTION_MP4_SAMPLE_LAYOUT_TEST_CHILD";
    const TEST: &str = "logging::mp4_sample_layout_tests::actual_mp4_mapping_failures_reach_default_sinks_and_the_worker_recovers";
    const FAILURES: [(&str, &str); 2] = [
        ("mp4-map-zero", "invalid MP4 sample layout track=0 atom=stsc phase=preflight: first chunk must start at one"),
        ("mp4-map-range", "invalid MP4 sample layout track=0 atom=stco phase=preflight: chunk payload outside file"),
    ];
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _guard = init(root.join("logs")).unwrap();
        let valid = fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../crates/motion-media/tests/fixtures/tone-stereo-48000.m4a"),
        )
        .unwrap();
        let mut zero = valid.clone();
        assert!(change(&mut zero, b"stsc", 8, 0));
        let mut range = valid.clone();
        assert!(change(&mut range, b"stsz", 12, u32::MAX));
        let jobs = AudioJobs::new(root.clone(), Limits::default()).unwrap();
        for (request, bytes, cause) in [
            (FAILURES[0].0, zero, Some(FAILURES[0].1)),
            (FAILURES[1].0, range, Some(FAILURES[1].1)),
            ("mp4-map-recovered", valid, None),
        ] {
            jobs.start(
                Box::new(std::io::Cursor::new(bytes)),
                None,
                ImportOptions {
                    request_id: request.into(),
                    name: "sample layout test".into(),
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
                    assert_eq!(status.error.as_deref(), cause, "{status:?}");
                    if cause.is_some() {
                        assert_eq!(status.state, "failed");
                        assert!(status.metadata.is_none());
                    } else {
                        assert_eq!(status.state, "succeeded");
                        assert_eq!(status.metadata.unwrap().sample_frames, 96_000);
                    }
                    break;
                }
                assert!(began.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(5));
            }
            if cause.is_some() {
                while !fs::read_to_string(root.join("logs/motion-studio.log"))
                    .unwrap()
                    .contains(&format!("media error request={request} "))
                {
                    assert!(began.elapsed() < Duration::from_secs(10));
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            jobs.release(request).unwrap();
        }
        drop(jobs);
        flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{stderr}\n{stdout}");
    let text = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    assert!(text.contains("logging initialized level=INFO"));
    for (request, cause) in FAILURES {
        let start = format!("media start request={request} ");
        let failed = format!("media error request={request} ");
        assert!(text.find(&start).unwrap() < text.find(&failed).unwrap());
        assert_eq!(text.lines().filter(|line| line.contains(&start)).count(), 1);
        for sink in [&text, &stderr] {
            let lines: Vec<_> = sink.lines().filter(|line| line.contains(&failed)).collect();
            assert_eq!(lines.len(), 1);
            assert!(
                lines[0].contains(cause)
                    && lines[0].contains("operation=probe_audio phase=failed ")
                    && lines[0].contains("ms: ")
            );
            assert!(!sink.contains("worker panic") && !sink.contains("panic command="));
            assert!(!sink.contains("media error request=mp4-map-recovered "));
        }
        assert!(!stdout.contains(cause) && !stdout.contains(&failed) && !stderr.contains(&start));
    }
    assert!(text.contains("media start request=mp4-map-recovered "));
    assert!(!stdout.contains("media start request="));
}
