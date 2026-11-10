use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_install_keeps_default_foreground_quiet_and_preserves_worker_start() {
    const NAME: &str = "movie::prerender::coordinator::tests::foreground_logging::native_install_keeps_default_foreground_quiet_and_preserves_worker_start";
    const CHILD: &str = "MOTION_PRERENDER_INSTALL_LOG_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    struct Capture {
        foreground: std::thread::ThreadId,
        measuring: AtomicBool,
        foreground_flushes: AtomicUsize,
        foreground_records: Mutex<Vec<String>>,
        worker_start: AtomicBool,
    }
    impl log::Log for Capture {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.level() <= log::Level::Info
        }
        fn log(&self, record: &log::Record<'_>) {
            if !self.enabled(record.metadata()) {
                return;
            }
            let text = record.args().to_string();
            if std::thread::current().id() != self.foreground {
                if text.contains("prerender_worker operation=render") {
                    self.worker_start.store(true, Ordering::Release);
                }
            } else if self.measuring.load(Ordering::Acquire) {
                let mut records = self.foreground_records.lock().unwrap();
                assert!(records.len() < 32, "foreground capture must remain bounded");
                records.push(text.chars().take(512).collect());
            }
        }
        fn flush(&self) {
            if std::thread::current().id() == self.foreground
                && self.measuring.load(Ordering::Acquire)
            {
                self.foreground_flushes.fetch_add(1, Ordering::AcqRel);
            }
        }
    }
    let logger = Box::leak(Box::new(Capture {
        foreground: std::thread::current().id(),
        measuring: AtomicBool::new(false),
        foreground_flushes: AtomicUsize::new(0),
        foreground_records: Mutex::new(Vec::new()),
        worker_start: AtomicBool::new(false),
    }));
    log::set_logger(logger).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    prepare(&mut owner, &session, &renderer);
    owner.advance_playback(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    logger.measuring.store(true, Ordering::Release);
    let installed = drain(&mut owner, &renderer);
    logger.measuring.store(false, Ordering::Release);
    assert!(
        installed,
        "the real completed native frame must be installed"
    );
    assert_eq!(
        logger.foreground_flushes.load(Ordering::Acquire),
        0,
        "healthy foreground cache installation must not flush the log sink"
    );
    assert!(
        logger.foreground_records.lock().unwrap().is_empty(),
        "healthy foreground cache installation must stay quiet at default Info"
    );
    assert!(
        logger.worker_start.load(Ordering::Acquire),
        "the non-Present worker start signal must remain visible at default Info"
    );
    let cached = owner.get(&renderer, 1.).unwrap().unwrap();
    assert_eq!(
        pixels(&renderer, cached.source()),
        [255, 0, 0, 255].repeat(32 * 24)
    );
}
