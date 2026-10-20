//! File process logging with console stderr for errors; stdout belongs to CLI/MCP.
#[cfg(test)]
mod media_commit_tests;
#[cfg(test)]
mod frame_blend_tests;

use std::{
    backtrace::Backtrace,
    cell::Cell,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct FileLogger {
    writer: Mutex<BufWriter<File>>,
    level: log::LevelFilter,
}
static LOGGER: OnceLock<FileLogger> = OnceLock::new();
thread_local! {
    static IN_PANIC_HOOK: Cell<bool> = const { Cell::new(false) };
}

impl FileLogger {
    fn lock_writer(&self) -> std::sync::MutexGuard<'_, BufWriter<File>> {
        match self.writer.lock() {
            Ok(writer) => writer,
            Err(poisoned) => {
                let mut writer = poisoned.into_inner();
                // A formatter can panic after writing part of a record.
                // Separate the next record and keep this file sink usable.
                let _ = writer.write_all(b"\n");
                self.writer.clear_poison();
                writer
            }
        }
    }
    fn write(&self, record: &log::Record<'_>, writer: &mut impl Write) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let thread = std::thread::current();
        let _ = writeln!(
            writer,
            "{}.{:03} {} [{}] {}: {}",
            timestamp.as_secs(),
            timestamp.subsec_millis(),
            record.level(),
            thread.name().unwrap_or("unnamed"),
            record.target(),
            record.args()
        );
        if record.level() <= log::Level::Warn {
            let _ = writer.flush();
        }
    }
}
impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= self.level
            // Wgpu emits per-frame descriptors at Debug. Keep ordinary debug
            // sessions focused on Motion; Trace explicitly includes dependencies.
            && (self.level != log::LevelFilter::Debug
                || metadata.level() <= log::Level::Info
                || metadata.target().starts_with("motion_"))
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if IN_PANIC_HOOK.get() {
            // The previous panic hook already preserves console stderr output.
            // The panic may have originated while this very thread held writer.
            match self.writer.try_lock() {
                Ok(mut writer) => self.write(record, &mut *writer),
                Err(_) => {
                    let _ = writeln!(io::stderr(), "{}: {}", record.level(), record.args());
                }
            }
        } else {
            let mut writer = self.lock_writer();
            self.write(record, &mut *writer);
        }
        if record.level() == log::Level::Error && !IN_PANIC_HOOK.get() {
            // Flush the file and release its mutex before console IO: an MCP
            // client may pipe stderr without draining it. Keep the cause on disk.
            // stdout remains the command-list / JSON-RPC transport.
            let _ = writeln!(io::stderr(), "ERROR {}: {}", record.target(), record.args());
        }
    }
    fn flush(&self) {
        let _ = self.lock_writer().flush();
    }
}

/// Flush buffered info/debug records when main returns, including early CLI exits.
pub struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        flush();
    }
}
pub fn flush() {
    log::logger().flush();
}

pub fn init(dir: PathBuf) -> Result<Guard, Box<dyn std::error::Error>> {
    fs::create_dir_all(&dir)?;
    let path = dir.join("motion-studio.log");
    if path.exists() {
        let previous = dir.join("motion-studio.log.1");
        match fs::remove_file(&previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::rename(&path, previous)?;
    }
    let level = std::env::var("MOTION_LOG")
        .ok()
        .and_then(|level| level.parse().ok())
        .unwrap_or(log::LevelFilter::Info);
    let logger = FileLogger {
        writer: Mutex::new(BufWriter::new(
            OpenOptions::new().create(true).append(true).open(path)?,
        )),
        level,
    };
    LOGGER
        .set(logger)
        .map_err(|_| io::Error::other("logging already initialized"))?;
    let logger = LOGGER
        .get()
        .ok_or_else(|| io::Error::other("logger unavailable"))?;
    log::set_logger(logger).map_err(|_| io::Error::other("process logger already installed"))?;
    log::set_max_level(level);
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        IN_PANIC_HOOK.set(true);
        log::error!(
            "panic command={:?}: {info}\n{}",
            crate::engine::current_command(),
            Backtrace::force_capture()
        );
        IN_PANIC_HOOK.set(false);
        original(info);
    }));
    std::thread::Builder::new()
        .name("motion-log-flush".into())
        .spawn(|| loop {
            std::thread::sleep(Duration::from_secs(1));
            flush();
        })?;
    log::info!("logging initialized level={level}");
    Ok(Guard)
}

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
                assert!(invalid.sample(5.0).is_infinite());
                assert_eq!(healthy.sample(5.0), 3.0);
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
