//! Worker cache failures through the actual production logger at default level.
use super::*;
use motion_render::{DriverPipelineCacheError, DriverPipelineCachePhase, Renderer};
use std::{
    error::Error,
    process::{Command, Stdio},
};

const NAME: &str = "logging::driver_cache_tests::cache_worker_failure_reaches_default_file_and_stderr_without_stdout";
const CHILD: &str = "MOTION_DRIVER_CACHE_LOG_CHILD";
const TARGET: &str = "motion_render::driver_pipeline_cache::tasks";

#[test]
fn cache_worker_failure_reaches_default_file_and_stderr_without_stdout() {
    if let Some(root) = std::env::var_os(CHILD) {
        assert!(std::env::var_os("MOTION_LOG").is_none());
        let root = PathBuf::from(root);
        let _guard = init(root.join("logs")).unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let path = root.join("directory-not-a-cache");
        fs::create_dir(&path).unwrap();
        let supported = renderer.driver_pipeline_cache_info().supported;
        let (ticket, result) = if supported {
            // SAFETY: test-owned directory, not imported bytes; actual open/read
            // must fail before any data can reach the native cache constructor.
            let job = unsafe { renderer.driver_pipeline_cache_restore_job(&path) };
            let ticket = job.ticket();
            (
                ticket,
                std::thread::spawn(move || job.run()).join().unwrap(),
            )
        } else {
            // Even on a backend without cache support the owner-thread contract
            // remains a real API error, not a fabricated success/failure result.
            let job = renderer.driver_pipeline_cache_save_job(&path);
            let ticket = job.ticket();
            (ticket, job.run())
        };
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("invalid cache operation succeeded"),
        };
        if supported {
            assert!(matches!(
                &error,
                DriverPipelineCacheError::Io {
                    operation: "open" | "read",
                    ..
                }
            ));
            assert!(error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .is_some());
        } else {
            assert!(matches!(error, DriverPipelineCacheError::OwnerThread));
        }
        for _ in 0..65 {
            assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Failed);
        }
        let cause = error.to_string();
        let next = renderer.driver_pipeline_cache_save_job(&root.join("recovered-cache"));
        let healthy = next.ticket();
        let completion = std::thread::spawn(move || next.run())
            .join()
            .unwrap()
            .unwrap();
        assert!(matches!(
            completion,
            motion_render::DriverPipelineCacheCompletion::Saved { .. }
                | motion_render::DriverPipelineCacheCompletion::Unsupported
        ));
        assert!(matches!(
            healthy.status().phase,
            DriverPipelineCachePhase::Saved | DriverPipelineCachePhase::Ready
        ));
        renderer.check_health().unwrap();
        flush();
        fs::write(root.join("expected.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "cause": cause, "request": ticket.status().request,
            "operation": if supported { "Restore" } else { "Save" }, "supported": supported,
            "adapter": { "name": renderer.adapter_info.name, "backend": format!("{:?}", renderer.adapter_info.backend) }
        })).unwrap()).unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let artifact = std::env::var_os("MOTION_DRIVER_CACHE_LOG_EVIDENCE").map(PathBuf::from);
    let root = artifact.as_deref().unwrap_or(temporary.path());
    fs::create_dir_all(root).unwrap();
    let stdout_path = root.join("stdout.txt");
    let stderr_path = root.join("stderr.txt");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, root)
        .env_remove("MOTION_LOG")
        .stdout(Stdio::from(File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(File::create(&stderr_path).unwrap()))
        .spawn()
        .unwrap();
    let began = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(90) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("driver cache default logger acceptance child hung");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = fs::read_to_string(stdout_path).unwrap();
    let console = fs::read_to_string(stderr_path).unwrap();
    assert!(status.success(), "{output}\n{console}");
    let expected: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("expected.json")).unwrap()).unwrap();
    let cause = expected["cause"].as_str().unwrap();
    let request = expected["request"].as_u64().unwrap();
    let operation = expected["operation"].as_str().unwrap();
    let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let start = file
        .lines()
        .position(|line| {
            line.contains(TARGET)
                && line.contains(&format!(
                    "start request={request} operation={operation} path="
                ))
                && line.contains("INFO")
        })
        .unwrap();
    let failure = file
        .lines()
        .position(|line| line.contains(TARGET) && line.contains("driver pipeline cache failed"))
        .unwrap();
    assert!(
        start < failure,
        "start record must precede the actual operation's failure"
    );
    for text in [&file, &console] {
        let errors: Vec<_> = text
            .lines()
            .filter(|line| line.contains(TARGET) && line.contains("driver pipeline cache failed"))
            .collect();
        assert_eq!(errors.len(), 1, "{text}");
        assert!(errors[0].contains(&format!("request={request} operation={operation} path=")));
        assert!(errors[0].contains("elapsed_ms=") && errors[0].contains(cause));
    }
    assert!(!console
        .lines()
        .any(|line| line.contains(TARGET) && line.contains("cache start")));
    assert!(
        !output.contains(TARGET) && !output.contains(cause),
        "stdout carried logger records"
    );
}
