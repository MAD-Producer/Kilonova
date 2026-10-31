use super::*;
use std::sync::{Mutex, OnceLock};

struct Capture(Mutex<Vec<(log::Level, String)>>);
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            self.0
                .lock()
                .unwrap()
                .push((record.level(), record.args().to_string()));
        }
    }
    fn flush(&self) {}
}
static CAPTURE: OnceLock<Capture> = OnceLock::new();

#[test]
fn repeated_actual_open_failures_accumulate_until_success_or_configuration_change() {
    let temp = tempfile::tempdir().unwrap();
    let blocked = temp.path().join("not-directory");
    fs::write(&blocked, b"preserve original").unwrap();
    let config = Config::new(blocked.clone(), 1 << 20).unwrap();
    let mut owner = Owner::default();
    let control = ExportControl::default();
    for id in 1..=65 {
        owner
            .configure(Some(config.clone()), 1 << 16, id, &control)
            .unwrap();
    }
    assert_eq!(owner.stats.io_failures, 65);
    assert_eq!(owner.consecutive[0], 65);
    assert!(!owner.stats.ready);
    assert_eq!(fs::read(&blocked).unwrap(), b"preserve original");
    let healthy = Config::new(temp.path().join("healthy"), 1 << 20).unwrap();
    owner
        .configure(Some(healthy), 1 << 16, 66, &control)
        .unwrap();
    assert!(owner.stats.ready);
    assert_eq!(owner.consecutive, [0; 3]);
    assert_eq!(
        owner.stats.io_failures, 65,
        "lifetime totals remain diagnostic"
    );
    owner.configure(None, 1 << 16, 67, &control).unwrap();
    assert!(!owner.stats.enabled && !owner.stats.ready);
    assert_eq!(owner.stats.quota_bytes, 0);
}

#[test]
fn disk_faults_are_bounded_escalate_once_and_reset_on_real_miss_or_success() {
    const CHILD: &str = "MOTION_DISK_CACHE_OBSERVATION_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "movie::prerender::disk::tests::observations::disk_faults_are_bounded_escalate_once_and_reset_on_real_miss_or_success",
                "--exact",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed; 0 ignored"));
        return;
    }
    let logger = CAPTURE.get_or_init(|| Capture(Mutex::new(Vec::new())));
    log::set_logger(logger).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let temp = tempfile::tempdir().unwrap();
    let config = Config::new(temp.path().join("cache"), 1 << 20).unwrap();
    let control = ExportControl::default();
    let mut owner = Owner::default();
    owner
        .configure(Some(config), 1 << 16, 82, &control)
        .unwrap();
    let image = image(ChannelPrecision::Float32);
    let key = key(&image, context(), 1., 5);
    let long = "界".repeat(2000);
    for _ in 0..300 {
        owner.fault(1, 82, Some(1.), &long);
    }
    {
        let records = logger.0.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, log::Level::Warn);
        assert_eq!(records[1].0, log::Level::Error);
        assert!(records[1]
            .1
            .contains("ticket=82 phase=read frame=Some(1.0) consecutive=60"));
        assert_eq!(records[1].1.matches('界').count(), 512);
    }
    assert_eq!(owner.consecutive[1], u8::MAX);
    assert_eq!(owner.stats.io_failures, 300);
    assert_eq!(
        owner.stats.last_failure.as_ref().unwrap().chars().count(),
        512
    );
    // A real missing path resets read degradation instead of becoming a fault.
    assert!(owner.load(&key, 83, &control).unwrap().is_none());
    assert_eq!(owner.consecutive[1], 0);
    owner.fault(1, 84, Some(1.), "second episode original cause");
    assert_eq!(logger.0.lock().unwrap().len(), 3);
    owner.store(&key, &image, 85, &control).unwrap();
    assert!(owner.load(&key, 86, &control).unwrap().is_some());
    assert_eq!(owner.consecutive[1], 0);
    for _ in 0..60 {
        owner.fault(2, 87, Some(1.), "write failure");
    }
    assert_eq!(logger.0.lock().unwrap().len(), 5);
    owner.store(&key, &image, 88, &control).unwrap();
    assert_eq!(owner.consecutive[2], 0);
    let before = owner.stats.io_failures;
    let cancelled = ExportControl::default();
    cancelled.cancel();
    assert_eq!(
        owner.load(&key, 89, &cancelled).unwrap_err(),
        "movie export cancelled"
    );
    assert_eq!(owner.stats.io_failures, before);
    assert_eq!(logger.0.lock().unwrap().len(), 5);
    assert!(owner.stats.hits >= 1 && owner.stats.misses >= 1 && owner.stats.writes >= 2);
}
