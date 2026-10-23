use super::*;
use std::process::{Command, Stdio};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_gpu_callback_is_bounded_and_reaches_default_sinks_without_health_polling() {
    const CHILD: &str = "MOTION_GPU_CALLBACK_LOG_CHILD";
    const TEST: &str = "logging::gpu_callback_tests::native_gpu_callback_is_bounded_and_reaches_default_sinks_without_health_polling";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        let renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let adapter = renderer.adapter.get_info();
        fs::write(root.join("adapter.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "name":adapter.name,"driver":adapter.driver,"driverInfo":adapter.driver_info,
            "backend":format!("{:?}",adapter.backend),"vendor":adapter.vendor,"device":adapter.device
        })).unwrap()).unwrap();
        for _ in 0..120 {
            renderer.check_health().unwrap();
        }
        let label = format!(
            "native-invalid-buffer-start {} native-invalid-buffer-end",
            "界".repeat(8192)
        );
        let _invalid = renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&label),
            size: 4,
            usage: wgpu::BufferUsages::empty(),
            mapped_at_creation: false,
        });
        // Read the real file sink before gpu_error/check_health: callback evidence
        // must not depend on a later frame or host health polling.
        let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(file.contains("GPU uncaptured error"), "{file}");
        assert!(!file.contains(&label));
        assert!(file.contains("...[truncated]..."));
        let first = renderer.gpu_error().unwrap();
        assert!(first.len() <= 2048, "{}", first.len());
        assert!(
            first.contains("native-invalid-buffer-start")
                && first.contains("native-invalid-buffer-end")
        );
        assert!(first.to_lowercase().contains("usage"), "{first}");
        assert!(renderer
            .check_health()
            .unwrap_err()
            .to_string()
            .contains(&first));
        let _second = renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("second-invalid-buffer"),
            size: 4,
            usage: wgpu::BufferUsages::empty(),
            mapped_at_creation: false,
        });
        assert_eq!(renderer.gpu_error().unwrap(), first);
        renderer.device.destroy();
        let _ = renderer.device.poll(wgpu::PollType::wait_indefinitely());
        let after = renderer.gpu_error().unwrap();
        assert!(after.starts_with("GPU device lost: Destroyed:"), "{after}");
        flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout = root.path().join("stdout.txt");
    let stderr = root.path().join("stderr.txt");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .stdout(Stdio::from(File::create(&stdout).unwrap()))
        .stderr(Stdio::from(File::create(&stderr).unwrap()))
        .spawn()
        .unwrap();
    let began = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(45) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("native GPU callback child did not finish");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = fs::read_to_string(stdout).unwrap();
    let console = fs::read_to_string(stderr).unwrap();
    assert!(status.success(), "{output}\n{console}");
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    for (text, expected) in [(&file, 1), (&console, 1), (&output, 0)] {
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("GPU uncaptured error"))
                .count(),
            expected,
            "{text}"
        );
        assert!(
            !text.contains("GPU device lost"),
            "explicit destroy must stay quiet: {text}"
        );
    }
    assert!(file.len() < 16 * 1024, "{}", file.len());
    assert!(console.len() < 16 * 1024, "{}", console.len());
    assert!(!output.contains("native-invalid-buffer-start"));
    assert!(!file.contains("second-invalid-buffer"));
}
