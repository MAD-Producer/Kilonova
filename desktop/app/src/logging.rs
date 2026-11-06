//! File process logging with console stderr for errors; stdout belongs to CLI/MCP.
#[cfg(test)]
mod media_commit_tests;
#[cfg(test)]
mod media_error_tests;
#[cfg(test)]
mod mp4_sample_clock_tests;
#[cfg(test)]
mod mp4_sample_layout_tests;
#[cfg(test)]
mod package_publication_tests;
#[cfg(test)]
mod frame_blend_tests;
#[cfg(test)]
mod composition_bundle_tests;
#[cfg(test)]
mod command_error_tests;
#[cfg(test)]
mod gpu_command_tests;
#[cfg(all(test, debug_assertions, feature = "gpu-tests"))]
mod pointwise_compile_tests;
#[cfg(all(test, debug_assertions, feature = "gpu-tests"))]
mod execution_ir_tests;
#[cfg(test)]
mod media_task_bounds_tests;

#[cfg(test)]
mod frozen_reader_tests;
include!("logging/sink.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;

    #[test]
    fn file_sink_filters_debug_and_flushes_buffered_info_on_warning() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sink.log");
        let logger = FileLogger {
            writer: Mutex::new(BufWriter::new(File::create(&path).unwrap())),
            level: log::LevelFilter::Info,
        };
        logger.log(
            &log::Record::builder()
                .level(log::Level::Debug)
                .args(format_args!("hidden"))
                .build(),
        );
        logger.log(
            &log::Record::builder()
                .level(log::Level::Info)
                .args(format_args!("start #7 test_wait"))
                .build(),
        );
        assert!(fs::read_to_string(&path).unwrap().is_empty());
        logger.log(
            &log::Record::builder()
                .level(log::Level::Warn)
                .args(format_args!("timeout #7"))
                .build(),
        );
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("start #7 test_wait"));
        assert!(text.contains("timeout #7"));
        assert!(!text.contains("hidden"));
    }

    #[test]
    fn debug_preserves_motion_details_and_dependency_signals_without_frame_descriptor_flooding() {
        let root = tempfile::tempdir().unwrap();
        for level in [log::LevelFilter::Debug, log::LevelFilter::Trace] {
            let path = root.path().join(format!("{level}.log"));
            let logger = FileLogger {
                writer:Mutex::new(BufWriter::new(File::create(&path).unwrap())),
                level,
            };
            logger.log(&log::Record::builder().level(log::Level::Debug)
                .target("motion_studio::engine").args(format_args!("dispatch #7 state")).build());
            logger.log(&log::Record::builder().level(log::Level::Debug)
                .target("wgpu_core::command").args(format_args!("dependency frame descriptor")).build());
            logger.log(&log::Record::builder().level(log::Level::Info)
                .target("wgpu_core::instance").args(format_args!("adapter context")).build());
            logger.log(&log::Record::builder().level(log::Level::Warn)
                .target("wgpu_hal::vulkan").args(format_args!("dependency warning")).build());
            let text=fs::read_to_string(path).unwrap();
            assert!(text.contains("dispatch #7 state"));
            assert!(text.contains("adapter context") && text.contains("dependency warning"));
            assert_eq!(text.contains("dependency frame descriptor"), level == log::LevelFilter::Trace);
        }
    }

    #[test]
    fn errors_reach_file_and_stderr_without_leaking_log_records_to_stdout() {
        if let Some(root) = std::env::var_os("MOTION_LOG_CONSOLE_TEST_CHILD") {
            let _guard = init(PathBuf::from(root)).unwrap();
            log::error!(target:"console_test", "dual-error-marker");
            log::warn!(target:"console_test", "file-only-warning-marker");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "logging::tests::errors_reach_file_and_stderr_without_leaking_log_records_to_stdout"])
            .env("MOTION_LOG_CONSOLE_TEST_CHILD",root.path())
            .env_remove("MOTION_LOG")
            .output().unwrap();
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("ERROR console_test: dual-error-marker"));
        assert!(!stderr.contains("file-only-warning-marker"));
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stdout.contains("dual-error-marker") && !stdout.contains("file-only-warning-marker"));
        let file = fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(file.contains("dual-error-marker") && file.contains("file-only-warning-marker"));
    }

    #[test]
    fn model_failures_reach_default_file_and_error_console_without_stdout_logs() {
        const CHILD: &str = "MOTION_MODEL_FILE_TEST_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let _guard = init(PathBuf::from(root)).unwrap();
            let invalid = motion_core::Track::constant(f32::INFINITY);
            let healthy = motion_core::Track::constant(3.0_f32);
            for _ in 0..120 {
                assert!(invalid.sample(5.0).unwrap().is_infinite());
                assert_eq!(healthy.sample(5.0).unwrap(), 3.0);
            }
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "logging::tests::model_failures_reach_default_file_and_error_console_without_stdout_logs"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let file = fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        let lines: Vec<_> = file.lines().filter(|line| line.contains("site=track.sample")).collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains(" WARN ") && lines[0].contains("lifetime_count=1 "));
        assert!(lines[1].contains(" ERROR ") && lines[1].contains("lifetime_count=60 "));
        assert!(lines.iter().all(|line| line.contains("frame=5") && line.contains("value_type=f32")));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(stderr.lines().filter(|line| line.contains("site=track.sample")).count(), 1);
        assert!(stderr.contains("lifetime_count=60 "));
        assert!(!stderr.contains("lifetime_count=1 "));
        assert!(!String::from_utf8(output.stdout).unwrap().contains("site=track.sample"));
    }

    #[test]
    fn error_is_flushed_to_disk_before_an_undrained_console_pipe_can_block() {
        const CHILD: &str = "MOTION_BLOCKED_CONSOLE_TEST_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let _logger = init(PathBuf::from(root)).unwrap();
            let fill = "x".repeat(2 * 1024 * 1024);
            log::error!("blocked-console-begin {fill} durable-error-end");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "logging::tests::error_is_flushed_to_disk_before_an_undrained_console_pipe_can_block"])
            .env(CHILD,root.path()).env_remove("MOTION_LOG")
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped())
            .spawn().unwrap();
        let deadline = std::time::Instant::now()+Duration::from_secs(5);
        let mut persisted = false;
        while std::time::Instant::now()<deadline {
            if fs::read_to_string(root.path().join("motion-studio.log"))
                .is_ok_and(|text| text.contains("durable-error-end")) {
                persisted = true;
                break;
            }
            if child.try_wait().unwrap().is_some() { break; }
            std::thread::sleep(Duration::from_millis(10));
        }
        // This test alone owns the deliberately blocked subprocess. Always reap
        // it before asserting so a failing negative case cannot leave it behind.
        let _ = child.kill();
        child.wait().unwrap();
        assert!(persisted,"stderr blocked before the error reached the file");
    }

    #[test]
    fn formatter_panic_does_not_disable_subsequent_file_logging() {
        struct BrokenDisplay;
        impl std::fmt::Display for BrokenDisplay {
            fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                panic!("injected formatter panic");
            }
        }
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sink.log");
        let logger = FileLogger {
            writer: Mutex::new(BufWriter::new(File::create(&path).unwrap())),
            level: log::LevelFilter::Info,
        };
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            logger.log(&log::Record::builder().level(log::Level::Info)
                .args(format_args!("broken record {BrokenDisplay}")).build());
        }));
        assert!(caught.is_err());
        assert!(logger.writer.is_poisoned());
        logger.log(&log::Record::builder().level(log::Level::Warn)
            .args(format_args!("logging survived formatter panic")).build());
        assert!(!logger.writer.is_poisoned());
        let text = fs::read_to_string(path).unwrap();
        assert!(text.lines().any(|line| line.contains(" WARN ") && line.ends_with("logging survived formatter panic")));
    }

    #[test]
    fn panic_hook_captures_caught_panics_and_never_waits_for_its_own_sink() {
        // A subprocess isolates the global logger/hook from parallel engine tests.
        if let Some(root) = std::env::var_os("MOTION_LOG_TEST_CHILD") {
            let root = PathBuf::from(root);
            let _guard = init(root.clone()).unwrap();
            let held = std::env::var_os("MOTION_LOG_TEST_HELD").is_some();
            let lock = held.then(|| LOGGER.get().unwrap().writer.lock().unwrap());
            let result = std::panic::catch_unwind(|| panic!("observable caught panic"));
            drop(lock);
            assert!(result.is_err());
            log::warn!("unwind completed");
            return;
        }
        for held in [false, true] {
            let root = tempfile::tempdir().unwrap();
            fs::write(root.path().join("motion-studio.log"), "previous run").unwrap();
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command.args(["--exact", "logging::tests::panic_hook_captures_caught_panics_and_never_waits_for_its_own_sink"])
                .env("MOTION_LOG_TEST_CHILD", root.path()).env_remove("MOTION_LOG");
            if held {
                command.env("MOTION_LOG_TEST_HELD", "1");
            } else {
                command.env_remove("MOTION_LOG_TEST_HELD");
            }
            // Redirect to files, so a failed hook cannot block on a full pipe.
            let stdout = root.path().join("stdout");
            let stderr = root.path().join("stderr");
            let mut child = command
                .stdout(File::create(&stdout).unwrap())
                .stderr(File::create(&stderr).unwrap())
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("panic hook deadlocked with held={held}");
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            assert!(status.success(), "{}", fs::read_to_string(&stderr).unwrap());
            let text = fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
            assert!(text.contains("unwind completed"));
            assert_eq!(
                fs::read_to_string(root.path().join("motion-studio.log.1")).unwrap(),
                "previous run"
            );
            if held {
                assert!(fs::read_to_string(stderr)
                    .unwrap()
                    .contains("observable caught panic"));
            } else {
                assert!(text.contains("observable caught panic"));
                assert!(text.contains("panic_hook_captures_caught_panics"));
            }
        }
    }
}

#[cfg(test)]
mod vector_upload_tests;
#[cfg(test)]
mod gpu_callback_tests;
