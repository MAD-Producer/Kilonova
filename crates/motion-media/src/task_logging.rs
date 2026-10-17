//! Sinks are supplied by the embedding host; never write stdout/stderr directly here.
use std::time::Instant;

/// Preserve a failure before cancellation discards a worker's caught result.
/// Expected cooperative cancellation is a terminal outcome, not a fault.
pub(crate) fn cancelled_error<T, E: std::fmt::Display>(
    result: &std::thread::Result<std::result::Result<T, E>>,
    expected: &[&str],
) -> Option<String> {
    match result {
        Ok(Err(error)) => {
            let reason = error.to_string();
            (!expected.contains(&reason.as_str())).then_some(reason)
        }
        Err(payload) => Some(format!(
            "worker panic: {}",
            crate::panic_reason_ref(payload.as_ref())
        )),
        Ok(Ok(_)) => None,
    }
}

#[derive(Clone)]
pub(crate) struct TaskLog {
    request: String,
    operation: String,
    began: Instant,
}

impl TaskLog {
    pub(crate) fn new(request: &str, operation: &str) -> Self {
        Self {
            request: request.into(),
            operation: operation.into(),
            began: Instant::now(),
        }
    }
    pub(crate) fn start(&self) {
        log::info!(
            "media start request={} operation={}",
            self.request,
            self.operation
        );
        log::logger().flush();
    }
    pub(crate) fn finish(&self, phase: &str, error: Option<&str>) {
        if let Some(error) = error {
            log::error!(
                "media error request={} operation={} phase={phase} {}ms: {error}",
                self.request,
                self.operation,
                self.began.elapsed().as_millis()
            );
        } else {
            log::debug!(
                "media worker done request={} operation={} phase={phase} {}ms",
                self.request,
                self.operation,
                self.began.elapsed().as_millis()
            );
        }
    }
}

#[cfg(test)]
mod cancellation_tests;

#[cfg(test)]
mod tests {
    use crate::{
        AudioJobs, DecodeAudio, ImportOptions, Limits, PackageJobs, VideoImportOptions, VideoJobs,
    };
    use std::{
        io::Read,
        sync::{mpsc, Arc, Mutex},
        thread::ThreadId,
        time::{Duration, Instant},
    };
    struct Event {
        level: log::Level,
        thread: ThreadId,
        text: String,
    }
    struct Capture {
        events: Mutex<Vec<Event>>,
        flushes: Mutex<Vec<ThreadId>>,
    }
    static CAPTURE: Capture = Capture {
        events: Mutex::new(Vec::new()),
        flushes: Mutex::new(Vec::new()),
    };
    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            let text = record.args().to_string();
            if text.contains("media-observation-") {
                self.events.lock().unwrap().push(Event {
                    level: record.level(),
                    thread: std::thread::current().id(),
                    text,
                });
            }
        }
        fn flush(&self) {
            self.flushes
                .lock()
                .unwrap()
                .push(std::thread::current().id());
        }
    }
    fn assert_started(id: &str, operation: &str) {
        let events = CAPTURE.events.lock().unwrap();
        let event = events
            .iter()
            .find(|e| {
                e.level == log::Level::Info
                    && e.text
                        .contains(&format!("media start request={id} operation={operation}"))
            })
            .expect("default Info start must precede provider entry");
        assert!(CAPTURE.flushes.lock().unwrap().contains(&event.thread));
        assert!(!events
            .iter()
            .any(|e| e.level == log::Level::Error && e.text.contains(id)));
    }
    fn wait_error(id: &str, operation: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if CAPTURE.events.lock().unwrap().iter().any(|e| {
                e.level == log::Level::Error
                    && e.text
                        .contains(&format!("request={id} operation={operation}"))
                    && e.text.contains("ms:")
            }) {
                return;
            }
            assert!(Instant::now() < deadline, "missing error for {id}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn audio_video_and_package_workers_log_default_starts_and_terminal_failures() {
        log::set_logger(&CAPTURE).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let root = tempfile::tempdir().unwrap();
        let decode: DecodeAudio = Arc::new(|_, _, _, _, _| Err("unused decoder".into()));
        let audio = AudioJobs::with_decoder(root.path().into(), Limits::default(), decode).unwrap();
        let (entered, entry) = mpsc::channel();
        let (release, released) = mpsc::channel();
        audio
            .start_with_source(
                move || {
                    entered.send(()).unwrap();
                    released.recv().unwrap();
                    Err("controlled provider open failure".into())
                },
                ImportOptions {
                    request_id: "media-observation-audio".into(),
                    at_frame: 0,
                    track: None,
                    name: "test".into(),
                },
                false,
            )
            .unwrap();
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_started("media-observation-audio", "import_audio");
        release.send(()).unwrap();
        wait_error("media-observation-audio", "import_audio");
        assert_eq!(
            audio.status("media-observation-audio").unwrap().state,
            "failed"
        );

        let video = VideoJobs::new(root.path().into()).unwrap();
        let (entered, entry) = mpsc::channel();
        let (release, released) = mpsc::channel();
        video
            .start(
                move || -> crate::Result<(Box<dyn Read + Send>, Option<u64>)> {
                    entered.send(()).unwrap();
                    released.recv().unwrap();
                    Err("controlled video open failure".into())
                },
                VideoImportOptions {
                    request_id: "media-observation-video".into(),
                    at_frame: 0,
                    name: "test".into(),
                    track: None,
                    audio_track: None,
                    with_audio: false,
                },
                false,
                Arc::new(|_, _, _, _| Err("unused probe".into())),
            )
            .unwrap();
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_started("media-observation-video", "import_video");
        release.send(()).unwrap();
        wait_error("media-observation-video", "import_video");
        assert_eq!(
            video.status("media-observation-video").unwrap().state,
            "failed"
        );

        let project = motion_core::Project::new(64, 64, 30, 30).unwrap();
        let snapshot = motion_core::storage::PackageSnapshot::new(root.path(), &project).unwrap();
        let blocked = root.path().join("blocked");
        std::fs::write(&blocked, b"file blocks output parent").unwrap();
        let packages = PackageJobs::default();
        packages
            .start_export(
                "media-observation-package",
                snapshot,
                blocked.join("out.msproj"),
                0,
            )
            .unwrap();
        wait_error("media-observation-package", "export_project");
        assert_eq!(
            packages.status("media-observation-package").unwrap().state,
            "failed"
        );
        let events = CAPTURE.events.lock().unwrap();
        assert!(events.iter().any(|e| e.level == log::Level::Info
            && e.text
                .contains("media start request=media-observation-package")));
        assert!(events
            .iter()
            .any(|e| e.level == log::Level::Error
                && e.text.contains("controlled provider open failure")));
        assert!(events
            .iter()
            .any(|e| e.level == log::Level::Error
                && e.text.contains("controlled video open failure")));
    }
}
