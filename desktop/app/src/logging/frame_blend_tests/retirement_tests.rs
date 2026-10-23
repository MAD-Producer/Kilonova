use super::*;

struct ReleaseOnDrop(mpsc::Sender<()>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn same_source_retirement_reports_default_threshold_suppression_and_fresh_episode_after_recovery() {
    const CHILD: &str = "MOTION_VIDEO_RETIREMENT_DEFAULT_CHILD";
    const NAME: &str = "logging::frame_blend_tests::retirement_tests::same_source_retirement_reports_default_threshold_suppression_and_fresh_episode_after_recovery";
    const MARKER: &str = "persistent degradation site=video_decoder_retirement count=60 ";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        let project = fixture(&root);
        let (entered, waiting) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let _release = ReleaseOnDrop(release.clone());
        let gate = Arc::new(Gate {
            entered,
            released: Mutex::new(released),
            fail: AtomicBool::new(false),
        });
        let mut frames = VideoFrames::new(Arc::new(Backend(gate)));
        let path = root.join("logs/motion-studio.log");
        for episode in 0..2 {
            let sequence = episode as u64 * 3 + 1;
            frames.request(&project, &root, 7, 1., sequence).unwrap();
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(frames.metrics()["decodeInFlight"][0]["phase"], "decode");
            frames.clear();
            for poll in 1..=120 {
                let status = frames
                    .request(&project, &root, 7, 1., sequence + 1)
                    .unwrap();
                assert_eq!(status["reason"], "decoder_retirement");
                assert_eq!(status["blocked_object"], 7);
                let metrics = frames.metrics();
                assert_eq!(metrics["streams"], 0);
                assert_eq!(metrics["retiringWorkers"][0]["object"], 7);
                assert_eq!(metrics["retiringWorkers"][0]["source"], 1);
                assert_eq!(metrics["retiringWorkers"][0]["phase"], "retirement");
                if matches!(poll, 59 | 60 | 120) {
                    flush();
                    let text = fs::read_to_string(&path).unwrap();
                    assert_eq!(
                        text.matches(MARKER).count(),
                        episode + usize::from(poll >= 60),
                        "{text}"
                    );
                }
            }
            release.send(()).unwrap();
            // This explicit new target opens only after native closure. Frame
            // zero is healthy and does not enter the controlled frame-one gate.
            wait(|| {
                frames
                    .request(&project, &root, 7, 0., sequence + 2)
                    .unwrap()["state"]
                    == "ready"
            });
            assert_eq!(
                frames
                    .frame(&project, 7, sequence + 2)
                    .unwrap()
                    .rgba()
                    .unwrap()
                    .as_ref(),
                &[255; 16]
            );
            assert_eq!(frames.metrics()["retiringStreams"], 0);
            assert_eq!(
                frames.metrics()["retiringWorkers"]
                    .as_array()
                    .unwrap()
                    .len(),
                0
            );
        }
        frames.finish().unwrap();
        flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stderr}\n{stdout}");
    let text = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    assert!(text.contains("logging initialized level=INFO"));
    for sink in [&text, &stderr] {
        let errors: Vec<_> = sink.lines().filter(|line| line.contains(MARKER)).collect();
        assert_eq!(errors.len(), 2, "{sink}");
        for line in errors {
            for context in [
                "object=7",
                "source=1",
                "sequence=",
                "blocked_object=7",
                "blocked_source=1",
                "phase=retirement",
                "elapsed_ms=",
            ] {
                assert!(line.contains(context), "{line}");
            }
        }
        assert_eq!(
            sink.lines()
                .filter(|line| line.starts_with("ERROR ") || line.contains(" ERROR "))
                .count(),
            2,
            "{sink}"
        );
        assert!(!sink.contains("video shutdown timeout") && !sink.contains("video decode failed"));
    }
    assert_eq!(
        text.matches("video decoder worker started object=7 source=1")
            .count(),
        3
    );
    assert_eq!(
        text.matches("video decoder worker stopped object=7 source=1")
            .count(),
        3
    );
    assert!(!text.contains("DEBUG") && !text.contains("count=1 "));
    assert!(!stdout.contains("video decoder") && !stdout.contains("video_decoder_retirement"));
    assert!(!stderr.contains("worker started") && !stderr.contains("open start"));
}
