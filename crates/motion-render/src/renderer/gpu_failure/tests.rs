use super::*;
use std::{
    fs::{self, File},
    io::Write,
};

struct Capture {
    file: Mutex<File>,
    failure: Arc<Mutex<Option<String>>>,
}
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            // Callback logging may be slow. Never hold the health-state lock.
            assert!(self.failure.try_lock().is_ok());
            let mut file = self.file.lock().unwrap();
            writeln!(file, "{} {}", record.level(), record.args()).unwrap();
            file.flush().unwrap();
        }
    }
    fn flush(&self) {}
}

#[test]
fn callback_causes_are_preserved_without_hiding_new_failure_kinds_or_logging_under_lock() {
    const CHILD: &str = "MOTION_GPU_CAUSE_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        let failure = Arc::new(Mutex::new(None));
        let capture = Box::leak(Box::new(Capture {
            file: Mutex::new(File::create(&path).unwrap()),
            failure: failure.clone(),
        }));
        log::set_logger(capture).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let observer = Observation::new(
            failure.clone(),
            wgpu::AdapterInfo {
                name: "controlled adapter".into(),
                vendor: 11,
                device: 22,
                device_type: wgpu::DeviceType::DiscreteGpu,
                driver: "controlled driver".into(),
                driver_info: String::new(),
                backend: wgpu::Backend::Vulkan,
                ..wgpu::AdapterInfo::new(wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan)
            },
        );
        for i in 0..120 {
            observer.uncaptured(wgpu::Error::Validation {
                source: Box::new(std::io::Error::other("controlled validation source")),
                description: format!("controlled validation {i}"),
            });
        }
        assert_eq!(
            failure.lock().unwrap().as_deref(),
            Some("GPU error: controlled validation 0")
        );
        observer.uncaptured(wgpu::Error::OutOfMemory {
            source: Box::new(std::io::Error::other("controlled OOM cause")),
        });
        observer.uncaptured(wgpu::Error::Internal {
            source: Box::new(std::io::Error::other("controlled internal source")),
            description: "controlled internal description".into(),
        });
        assert_eq!(
            failure.lock().unwrap().as_deref(),
            Some("GPU error: controlled validation 0")
        );
        observer.device_lost(
            wgpu::DeviceLostReason::Destroyed,
            "controlled explicit destroy".into(),
        );
        assert_eq!(
            failure.lock().unwrap().as_deref(),
            Some("GPU device lost: Destroyed: controlled explicit destroy")
        );
        let before = fs::read_to_string(&path).unwrap();
        assert!(!before.contains("controlled explicit destroy"), "{before}");
        observer.device_lost(
            wgpu::DeviceLostReason::Unknown,
            "controlled driver reset callback".into(),
        );
        assert_eq!(
            failure.lock().unwrap().as_deref(),
            Some("GPU device lost: Unknown: controlled driver reset callback")
        );
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(
            text.lines()
                .filter(|line| line.starts_with("ERROR"))
                .count(),
            4,
            "{text}"
        );
        assert_eq!(
            text.lines().filter(|line| line.starts_with("WARN")).count(),
            1,
            "{text}"
        );
        assert!(
            text.contains("count=60 last_kind=validation retained_first_cause=true"),
            "{text}"
        );
        for kind in ["validation", "out_of_memory", "internal"] {
            assert!(text.contains(&format!("count=1 kind={kind}")), "{text}");
        }
        assert!(text.contains("kind=validation retained_first_cause=false"));
        assert!(text.contains("kind=out_of_memory retained_first_cause=true"));
        assert!(
            text.contains("cause=Unknown") && text.contains("controlled driver reset callback")
        );
        assert!(text.contains("controlled OOM cause"));
        assert!(text.contains("adapter=\"controlled adapter\" backend=Vulkan vendor=11 device=22 driver=\"controlled driver\""));
        assert!(text
            .lines()
            .all(|line| line.contains("renderer=") && line.contains("elapsed_ms=")));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "renderer::gpu_failure::tests::callback_causes_are_preserved_without_hiding_new_failure_kinds_or_logging_under_lock"])
        .env(CHILD, root.path().join("events.log")).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("GPU uncaptured error"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("GPU uncaptured error"));
}
