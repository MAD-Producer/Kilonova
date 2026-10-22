use super::*;

#[test]
fn actual_zero_clock_import_reaches_task_status_and_default_sinks_without_panic() {
    const CHILD: &str = "MOTION_AUDIO_CLOCK_ERROR_CHILD";
    const NAME: &str = "engine::failure::audio_clock_tests::actual_zero_clock_import_reaches_task_status_and_default_sinks_without_panic";
    const REQUEST: &str = "invalid-audio-clock";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let original = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/motion-media/tests/fixtures/tone-stereo-48000.m4a"
        ))
        .unwrap();
        let offsets: Vec<_> = original
            .windows(4)
            .enumerate()
            .filter_map(|(i, bytes)| (bytes == b"mdhd").then_some(i))
            .collect();
        assert_eq!(offsets.len(), 1);
        let mut malformed = original.clone();
        malformed[offsets[0] + 16..offsets[0] + 20].fill(0);
        let input = root.join("invalid.m4a");
        std::fs::write(&input, malformed).unwrap();
        let engine = Engine::start(
            root.join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine.state().unwrap();
        let start = engine.media(serde_json::json!({"op":"probe_media", "request_id":REQUEST,"path":input,"kind":"audio"}).to_string()).unwrap();
        assert!(matches!(
            start["state"].as_str(),
            Some("pending" | "running")
        ));
        let deadline = Instant::now() + Duration::from_secs(5);
        let failed = loop {
            let status = engine
                .media(serde_json::json!({"op":"media_status", "request_id":REQUEST}).to_string())
                .unwrap();
            if status["state"] == "failed" {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "audio task did not reject its clock: {status}"
            );
            std::thread::sleep(Duration::from_millis(2));
        };
        let error = failed["error"].as_str().unwrap();
        assert!(
            error.contains("zero MP4 timebase atom=mdhd track=Some(0)"),
            "{error}"
        );
        assert!(!error.contains("panic") && !error.contains("worker failed"));
        std::fs::write(root.join("reply.txt"), error).unwrap();
        engine
            .media(serde_json::json!({"op":"release_media_task", "request_id":REQUEST}).to_string())
            .unwrap();
        let mut after = engine.state().unwrap();
        const COUNTER: &str = "/desktop/diagnostics/droppedRecords";
        let prior = before.pointer(COUNTER).unwrap().as_u64().unwrap();
        let current = after.pointer(COUNTER).unwrap().as_u64().unwrap();
        // Diagnostic recording is intentionally nonblocking and can contend
        // with its reporter. It is observation state, not project mutation.
        assert!(current >= prior, "diagnostic loss counter moved backwards");
        std::fs::write(
            root.join("state-counters.json"),
            serde_json::json!({"before":prior,"after":current}).to_string(),
        )
        .unwrap();
        *after.pointer_mut(COUNTER).unwrap() = serde_json::json!(prior);
        assert_eq!(after, before);
        std::fs::write(&input, &original).unwrap();
        engine.media(serde_json::json!({"op":"probe_media", "request_id":"valid-audio-clock","path":input,"kind":"audio"}).to_string()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = engine
                .media(
                    serde_json::json!({"op":"media_status", "request_id":"valid-audio-clock"})
                        .to_string(),
                )
                .unwrap();
            if status["state"] == "succeeded" {
                assert_eq!(status["metadata"]["sample_frames"], 96000);
                break;
            }
            assert!(status["state"] != "failed", "{status}");
            assert!(
                Instant::now() < deadline,
                "valid audio did not finish: {status}"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        engine
            .media(
                serde_json::json!({"op":"release_media_task", "request_id":"valid-audio-clock"})
                    .to_string(),
            )
            .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME])
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
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let reply = std::fs::read_to_string(root.path().join("reply.txt")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let marker = format!("media error request={REQUEST} operation=probe_audio phase=failed ");
    for sink in [&file, &stderr] {
        assert_eq!(sink.matches(&marker).count(), 1, "{sink}");
        let error = sink.lines().find(|line| line.contains(&marker)).unwrap();
        assert!(error.contains(&reply) && error.contains("ms:"), "{error}");
        assert!(!sink.contains("panicked") && !sink.contains("TimeBase cannot"));
    }
    let start = format!("media start request={REQUEST} operation=probe_audio");
    assert!(file.find(&start).unwrap() < file.find(&marker).unwrap());
    assert_eq!(file.matches("media error ").count(), 1, "{file}");
    assert!(!stderr.contains(&start) && !stderr.contains("logging initialized"));
    for marker in [&reply, &start, "logging initialized", "media error "] {
        assert!(!stdout.contains(marker), "{stdout}");
    }
}
