use super::*;

struct UnformattedSource;
impl std::fmt::Display for UnformattedSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("native source")
    }
}
impl std::fmt::Debug for UnformattedSource {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        panic!("disabled logging must not format GPU source Debug")
    }
}
impl std::error::Error for UnformattedSource {}

#[test]
fn disabled_logging_keeps_health_cause_and_counters_without_formatting_source() {
    const CHILD: &str = "MOTION_GPU_DISABLED_LOG_CHILD";
    const TEST: &str = "renderer::gpu_failure::limits_tests::disabled_logging_keeps_health_cause_and_counters_without_formatting_source";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, "1")
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
    log::set_max_level(log::LevelFilter::Off);
    let failure = Arc::new(Mutex::new(None));
    let observer = Observation::new(
        failure.clone(),
        wgpu::AdapterInfo::new(wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan),
    );
    for n in 0..120 {
        observer.uncaptured(wgpu::Error::Validation {
            source: Box::new(UnformattedSource),
            description: format!("controlled validation {n}"),
        });
    }
    assert_eq!(
        read(&failure).as_deref(),
        Some("GPU error: controlled validation 0")
    );
    assert_eq!(observer.uncaptured[1].load(Ordering::Relaxed), 120);
}

#[test]
fn bounded_text_keeps_utf8_and_both_operation_and_root_cause() {
    let original = format!(
        "operation=decode {} root=driver-reset",
        "敏\"\n".repeat(8192)
    );
    for limit in [0, 1, 2, 3, 17, 18, 19, 20, 511, 512, 2048] {
        let result = bounded(&original, limit);
        assert!(result.len() <= limit, "{} > {limit}", result.len());
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }
    let result = bounded(&original, CAUSE_BYTES);
    assert!(result.starts_with("operation=decode "));
    assert!(result.ends_with("root=driver-reset"));
    assert!(result.contains("...[truncated]..."));
    assert_eq!(bounded("short cause: 敏", CAUSE_BYTES), "short cause: 敏");
}

struct Capture {
    failure: Arc<Mutex<Option<String>>>,
    records: Mutex<Vec<(log::Level, String)>>,
}
impl log::Log for Capture {
    fn enabled(&self, record: &log::Metadata<'_>) -> bool {
        record.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            assert!(self.failure.try_lock().is_ok());
            self.records
                .lock()
                .unwrap()
                .push((record.level(), record.args().to_string()));
        }
    }
    fn flush(&self) {}
}

#[test]
fn oversized_callbacks_keep_latch_policy_thresholds_and_bounded_context() {
    const CHILD: &str = "MOTION_GPU_LIMITS_CHILD";
    const TEST: &str = "renderer::gpu_failure::limits_tests::oversized_callbacks_keep_latch_policy_thresholds_and_bounded_context";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
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
    let failure = Arc::new(Mutex::new(None));
    let capture = Box::leak(Box::new(Capture {
        failure: failure.clone(),
        records: Mutex::new(Vec::new()),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let huge = "敏\"\n\t".repeat(8192);
    let mut adapter = wgpu::AdapterInfo::new(wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan);
    adapter.name = format!("adapter-start {huge} adapter-end");
    adapter.driver = format!("driver-start {huge} driver-end");
    adapter.driver_info = huge.clone();
    let observer = Observation::new(failure.clone(), adapter);
    assert!(observer.adapter.name.len() <= CONTEXT_BYTES);
    assert!(observer.adapter.driver.len() <= CONTEXT_BYTES);
    for n in 0..120 {
        observer.uncaptured(wgpu::Error::Validation {
            source: Box::new(std::io::Error::other(format!(
                "native-start {huge} native-root=bad-usage"
            ))),
            description: format!("validation-{n} {huge} root=invalid-usage"),
        });
    }
    let first = read(&failure).unwrap();
    assert!(first.len() <= CAUSE_BYTES);
    assert!(first.starts_with("GPU error: validation-0 "));
    assert!(first.ends_with("root=invalid-usage"));
    observer.uncaptured(wgpu::Error::OutOfMemory {
        source: Box::new(std::io::Error::other("native OOM")),
    });
    assert_eq!(read(&failure).unwrap(), first);
    observer.device_lost(
        wgpu::DeviceLostReason::Destroyed,
        format!("explicit {huge} root=destroyed"),
    );
    assert!(read(&failure).unwrap().ends_with("root=destroyed"));
    assert_eq!(capture.records.lock().unwrap().len(), 3);
    for _ in 0..120 {
        observer.device_lost(
            wgpu::DeviceLostReason::Unknown,
            format!("driver {huge} root=device-reset"),
        );
        let retained = read(&failure).unwrap();
        assert!(retained.len() <= CAUSE_BYTES);
        assert!(retained.starts_with("GPU device lost: Unknown: driver "));
        assert!(retained.ends_with("root=device-reset"));
    }
    let records = capture.records.lock().unwrap();
    assert_eq!(records.len(), 5);
    assert_eq!(
        records.iter().filter(|r| r.0 == log::Level::Error).count(),
        3
    );
    assert_eq!(
        records.iter().filter(|r| r.0 == log::Level::Warn).count(),
        2
    );
    for (_, record) in records.iter() {
        assert!(record.len() < 16 * 1024);
        assert!(!record.contains(&huge));
        assert!(record.contains("renderer=") && record.contains("elapsed_ms="));
    }
    assert!(records
        .iter()
        .any(|r| r.1.contains("count=60 last_kind=validation")));
    assert!(records
        .iter()
        .any(|r| r.1.contains("count=60 cause=Unknown")));
    assert!(records
        .iter()
        .any(|r| r.1.contains("native-root=bad-usage")));
    assert!(!records.iter().any(|r| r.1.contains("root=destroyed")));
}
