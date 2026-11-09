use super::*;

struct Capture(Mutex<File>);
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("{} {}", record.level(), record.args());
        let mut sink = self.0.lock().unwrap();
        writeln!(sink, "{line}").unwrap();
        sink.flush().unwrap();
        drop(sink);
        if record.level() == log::Level::Error {
            eprintln!("{line}");
        }
    }
    fn flush(&self) {
        self.0.lock().unwrap().flush().unwrap();
    }
}

#[test]
fn missing_video_paths_retain_native_io_identity_default_logs_and_explicit_recovery() {
    const CHILD: &str = "MOTION_VIDEO_SOURCE_ERROR_CHILD";
    const CASE: &str = "video_frames::observability_tests::source_errors::missing_video_paths_retain_native_io_identity_default_logs_and_explicit_recovery";
    let Some(directory) = std::env::var_os(CHILD) else {
        let directory = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CASE, "--nocapture", "--test-threads=1"])
            .env(CHILD, directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let file = std::fs::read_to_string(directory.path().join("events.log")).unwrap();
        assert!(!stdout.contains("video decode") && !stdout.contains("ERROR"));
        assert!(!stderr.contains("INFO"));
        assert_eq!(
            stderr
                .lines()
                .filter(|l| l.starts_with("ERROR video decode failed"))
                .count(),
            2
        );
        assert_eq!(
            file.lines()
                .filter(|l| l.starts_with("ERROR video decode failed"))
                .count(),
            2
        );
        for object in [21, 22] {
            let marker =
                format!("ERROR video decode failed object={object} source=1 sequence=1 pts_us=0 ");
            let error = file.lines().find(|l| l.starts_with(&marker)).unwrap();
            assert!(
                error.contains("phase=open")
                    && error.contains("speculative=false")
                    && error.contains("reason=")
            );
            assert!(stderr.lines().any(|l| l == error));
            let start = format!(
                "INFO video decoder open start object={object} source=1 sequence=1 pts_us=0 path=assets/test.mp4"
            );
            assert!(file.find(&start).unwrap() < file.find(&marker).unwrap());
        }
        if let Some(evidence) = std::env::var_os("MOTION_SOURCE_ERROR_EVIDENCE") {
            let evidence = PathBuf::from(evidence);
            std::fs::create_dir_all(&evidence).unwrap();
            std::fs::write(evidence.join("events.log"), file).unwrap();
            std::fs::write(evidence.join("child-stdout.txt"), output.stdout).unwrap();
            std::fs::write(evidence.join("child-stderr.txt"), output.stderr).unwrap();
        }
        return;
    };
    let directory = PathBuf::from(directory);
    let logger = Box::leak(Box::new(Capture(Mutex::new(
        File::create(directory.join("events.log")).unwrap(),
    ))));
    log::set_logger(logger).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    for (object, missing_root) in [(21, false), (22, true)] {
        let root = directory.join(object.to_string());
        let asset = fixture(&root);
        let source = root.join(&asset.path);
        let original = std::fs::read(&source).unwrap();
        let index = motion_media::video_cache_path(&root, &asset).unwrap();
        let original_index = std::fs::read(&index).unwrap();
        let (entered, _) = mpsc::channel();
        let (_, receive) = mpsc::channel();
        let backend = Arc::new(Backend(Arc::new(Gate {
            phase: "healthy",
            entered,
            release: Mutex::new(receive),
            error: None,
        })));
        let stream = Stream::new(root.clone(), asset, backend, object).unwrap();
        let thread = stream.worker.as_ref().unwrap().thread().id();
        let moved = directory.join(format!("{object}-moved"));
        if missing_root {
            std::fs::rename(&root, &moved).unwrap();
        } else {
            std::fs::remove_file(&source).unwrap();
        }
        stream.request(0, 1, false).unwrap();
        wait_until(|| stream.shared.0.lock().unwrap().error.is_some());
        let first = stream.prepared(1).err().expect("missing source must fail");
        let MediaError::Io(original_error) = first.primary() else {
            panic!("native IO cause lost: {first:?}");
        };
        assert_eq!(original_error.kind(), std::io::ErrorKind::NotFound);
        assert!(original_error.raw_os_error().is_some());
        if missing_root {
            std::fs::rename(&moved, &root).unwrap();
        } else {
            std::fs::write(&source, &original).unwrap();
        }
        // Restoring the file cannot silently clear the failed request or retry.
        for _ in 0..65 {
            assert_eq!(stream.request(0, 1, false).unwrap()["state"], "failed");
            let next = stream
                .prepared(1)
                .err()
                .expect("failed request must retain cause");
            let MediaError::Io(next) = next.primary() else {
                panic!("native cause lost");
            };
            assert!(Arc::ptr_eq(original_error, next));
        }
        stream.request(0, 2, false).unwrap();
        wait_until(|| stream.ready(2).is_ok());
        let frame = stream.ready(2).unwrap();
        assert_eq!((frame.pts, frame.width, frame.height), (0, 2, 2));
        assert!(
            matches!(&frame.pixels, crate::video_frame::VideoPixels::Rgba(bytes) if bytes == &vec![255; 16])
        );
        assert_eq!(stream.worker.as_ref().unwrap().thread().id(), thread);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(std::fs::read(&index).unwrap(), original_index);
        drop(stream);
    }
}
