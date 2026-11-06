#[path = "../../logging/sink.rs"]
mod logging;
#[path = "../panic_context.rs"]
mod panic_context;
mod engine {
    pub fn current_command() -> Option<(u64, &'static str, std::time::Duration)> {
        None
    }
}

#[test]
fn fatal_child_panic_retains_cause_without_changing_unwind_or_stdout() {
    const CHILD: &str = "MOTION_DEPTH_WORKER_PANIC_CHILD";
    const TEST: &str = "fatal_child_panic_retains_cause_without_changing_unwind_or_stdout";
    const SENTINEL: &str = "native-depth-original-panic-sentinel";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = logging::init(root.join("logs")).unwrap();
        let path = root.join("panic.json");
        if std::env::var_os("MOTION_DEPTH_PANIC_BLOCK_RECEIPT").is_some() {
            std::fs::create_dir(&path).unwrap();
        }
        panic_context::install(path);
        if std::env::var_os("MOTION_DEPTH_PANIC_PRELUDE").is_some() {
            assert!(std::panic::catch_unwind(|| panic!("earlier-caught-panic")).is_err());
        }
        let fail =
            || std::panic::panic_any(format!("{SENTINEL}\n{}", "controlled-detail-".repeat(4096)));
        if std::env::var_os("MOTION_DEPTH_PANIC_CAUGHT").is_some() {
            assert!(std::panic::catch_unwind(fail).is_err());
            return;
        }
        fail();
    }
    for (blocked, caught, prelude) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (true, true, false),
        (false, false, true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .env_remove("MOTION_DEPTH_PANIC_BLOCK_RECEIPT")
            .env_remove("MOTION_DEPTH_PANIC_CAUGHT")
            .env_remove("MOTION_DEPTH_PANIC_PRELUDE");
        if blocked {
            command.env("MOTION_DEPTH_PANIC_BLOCK_RECEIPT", "1");
        }
        if caught {
            command.env("MOTION_DEPTH_PANIC_CAUGHT", "1");
        }
        if prelude {
            command.env("MOTION_DEPTH_PANIC_PRELUDE", "1");
        }
        let output = command.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(if caught { 0 } else { 101 }),
            "Preserve fatal/caught behavior: {:?}",
            output.status
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        let tail = stderr
            .chars()
            .rev()
            .take(512)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        assert!(
            tail.contains(SENTINEL),
            "Bounded child capture lost the primary cause: {tail}"
        );
        assert!(
            tail.contains("at=") && tail.contains("panic.rs:"),
            "Bounded child capture lost the source location: {tail}"
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stdout.contains("ERROR motion_depth_wgsl") && !stdout.contains("panic command="));
        let file = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
        assert!(
            file.contains("logging initialized level=INFO")
                && file.contains(SENTINEL)
                && file.contains(" ERROR ")
        );
        if blocked {
            assert!(stderr.contains("panic receipt unavailable"));
            assert!(root.path().join("panic.json").is_dir());
        } else {
            let bytes = std::fs::read(root.path().join("panic.json")).unwrap();
            assert!(bytes.len() < 16384);
            let receipt: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(receipt["status"], "native_panic_original_cause_retained");
            assert!(receipt["location"].as_str().unwrap().contains("panic.rs:"));
            let cause = receipt["cause"].as_str().unwrap();
            assert!(
                cause.contains(if prelude {
                    "earlier-caught-panic"
                } else {
                    SENTINEL
                }) && cause.chars().count() <= 1536
                    && !cause.contains('\n')
            );
        }
    }
}
