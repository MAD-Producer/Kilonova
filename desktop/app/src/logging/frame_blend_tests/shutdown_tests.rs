use super::*;

struct ReleaseOnDrop(mpsc::Sender<()>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn blocked_reader_drop_reports_timeout_to_default_sinks_and_an_explicit_new_reader_recovers() {
    const CHILD: &str = "MOTION_VIDEO_SHUTDOWN_DEFAULT_CHILD";
    const NAME: &str = "logging::frame_blend_tests::shutdown_tests::blocked_reader_drop_reports_timeout_to_default_sinks_and_an_explicit_new_reader_recovers";
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
        frames.request(&project, &root, 7, 1., 1).unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let flight = frames.metrics();
        assert_eq!(flight["decodeInFlight"][0]["phase"], "decode");
        let began = Instant::now();
        drop(frames);
        assert!(began.elapsed() < Duration::from_secs(2));
        flush();
        let text = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(
            text.contains("video shutdown timeout owner=reader"),
            "{text}"
        );
        release.send(()).unwrap();
        wait(|| {
            flush();
            fs::read_to_string(root.join("logs/motion-studio.log"))
                .unwrap()
                .contains("video decoder worker stopped object=7 source=1")
        });
        // A new explicit request uses a separate healthy reader. The product
        // does not retry or restart the decoder that exceeded its drop budget.
        let (release, released) = mpsc::channel();
        release.send(()).unwrap();
        // Retain the telemetry receiver during the healthy lookahead too.
        let (entered, _waiting) = mpsc::channel();
        let gate = Arc::new(Gate {
            entered,
            released: Mutex::new(released),
            fail: AtomicBool::new(false),
        });
        let mut healthy = VideoFrames::new(Arc::new(Backend(gate)));
        wait(|| healthy.request(&project, &root, 7, 0., 2).unwrap()["state"] == "ready");
        assert_eq!(
            healthy
                .frame(&project, 7, 2)
                .unwrap()
                .rgba()
                .unwrap()
                .as_ref(),
            &[255; 16]
        );
        healthy.finish().unwrap();
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
    assert!(
        text.find("video shutdown start owner=reader").unwrap()
            < text.find("video shutdown timeout owner=reader").unwrap()
    );
    for sink in [&text, &stderr] {
        let errors: Vec<_> = sink
            .lines()
            .filter(|line| line.contains("video shutdown timeout owner=reader"))
            .collect();
        assert_eq!(errors.len(), 1, "{sink}");
        for context in [
            "object=Some(7)",
            "source=Some(1)",
            "phase=join",
            "budget_ms=50",
            "disposition=detached",
        ] {
            assert!(errors[0].contains(context));
        }
        assert!(
            !sink.contains("video shutdown join failed")
                && !sink.contains("video decode failed")
                && !sink.contains("panic command=")
        );
    }
    assert_eq!(
        text.lines()
            .filter(|line| line.contains("video decoder worker started object=7 source=1"))
            .count(),
        2
    );
    assert_eq!(
        text.lines()
            .filter(|line| line.contains("video decoder worker stopped object=7 source=1"))
            .count(),
        2
    );
    assert!(!stdout.contains("video shutdown") && !stdout.contains("video decoder"));
    assert!(
        !stderr.contains("video shutdown start")
            && !stderr.contains("video decoder worker started")
    );
}
