//! The actual reporter must deliver native IO causes before presentation formats them.
use super::*;
use crate::engine::Error;
use std::{error::Error as _, path::Path, process::Command};

fn io(error: &Error) -> &std::io::Error {
    let Some(motion_host::Error::Io(cause)) = error.host() else {
        panic!("diagnostics reply lost its native IO cause: {error:?}");
    };
    assert!(cause.raw_os_error().is_some());
    let copied = error.clone();
    assert!(std::ptr::eq(error.host().unwrap(), copied.host().unwrap()));
    assert!(std::ptr::eq(
        error
            .source()
            .unwrap()
            .downcast_ref::<motion_host::Error>()
            .unwrap(),
        copied
            .source()
            .unwrap()
            .downcast_ref::<motion_host::Error>()
            .unwrap(),
    ));
    let text_only = Error::from(error.to_string());
    assert_eq!(text_only.to_string(), error.to_string());
    assert!(text_only.host().is_none());
    cause
}

fn wait_for(path: &Path, marker: &str) {
    let began = Instant::now();
    while !std::fs::read_to_string(path).unwrap().contains(marker) {
        assert!(began.elapsed() < Duration::from_secs(5), "missing {marker}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn actual_reporter_keeps_native_io_replies_default_channels_and_recovers() {
    const TEST: &str = "diagnostics::native_error_tests::actual_reporter_keeps_native_io_replies_default_channels_and_recovers";
    const CHILD: &str = "MOTION_DIAGNOSTICS_NATIVE_ERROR_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .stdout(std::fs::File::create(root.path().join("stdout.log")).unwrap())
            .stderr(std::fs::File::create(root.path().join("stderr.log")).unwrap())
            .spawn()
            .unwrap();
        let began = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if began.elapsed() > Duration::from_secs(60) {
                child.kill().unwrap();
                break child.wait().unwrap();
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if let Some(destination) = std::env::var_os("MOTION_DIAGNOSTICS_NATIVE_ERROR_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in [
                "stdout.log",
                "stderr.log",
                "motion-studio.log",
                "result.json",
                "healthy.json",
            ] {
                let path = root.path().join(name);
                if path.exists() {
                    std::fs::copy(path, destination.join(name)).unwrap();
                }
            }
        }
        let stdout = std::fs::read_to_string(root.path().join("stdout.log")).unwrap();
        let stderr = std::fs::read_to_string(root.path().join("stderr.log")).unwrap();
        assert!(status.success(), "{status}:\n{stdout}\n{stderr}");
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        let errors: Vec<_> = file
            .lines()
            .filter(|line| line.contains("diagnostics failed #"))
            .collect();
        assert_eq!(errors.len(), 2, "{file}");
        assert_eq!(stderr.lines().count(), 2, "{stderr}");
        for (index, line) in errors.iter().enumerate() {
            assert!(line.contains("ERROR [motion-diagnostics]"));
            let record = line.split("motion_studio::diagnostics: ").nth(1).unwrap();
            assert!(stderr.contains(record), "{stderr}");
            assert!(!stdout.contains(record));
            let start = format!("diagnostics start #{} export ", index + 1);
            assert!(file.find(&start).unwrap() < file.find(record).unwrap());
            assert!(record.contains("ms path=") && record.contains(" reason="));
        }
        assert!(!stderr.contains("diagnostics start"));
        assert!(!stdout.contains("diagnostics start"));
        assert!(!file.contains("diagnostics done"));
        return;
    };

    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let diagnostics = Diagnostics::new(None, None);
    diagnostics.record(
        91,
        "worker",
        "native-error-study",
        Duration::from_millis(21),
        None,
        None,
        Default::default(),
    );
    let before = diagnostics.report()["recent"].clone();
    let occupied = root.join("occupied");
    std::fs::write(&occupied, b"keep original file").unwrap();
    let directory = root.join("directory.json");
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("keep"), b"keep original directory").unwrap();

    // Stop the real reporter before serialization. Always unlock before checking
    // the observation so a failed assertion cannot strand the reporter.
    let held = diagnostics.state.lock().unwrap();
    let first = diagnostics.export(occupied.join("report.json")).unwrap();
    let began = Instant::now();
    let observed = loop {
        if std::fs::read_to_string(root.join("motion-studio.log"))
            .unwrap()
            .contains("diagnostics start #1 export ")
        {
            break true;
        }
        if began.elapsed() > Duration::from_secs(5) {
            break false;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    drop(held);
    assert!(
        observed,
        "start was not published before the blocked report snapshot"
    );
    let first = first
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap_err();
    assert!(matches!(
        io(&first).kind(),
        std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::NotADirectory
    ));
    let second = diagnostics
        .export(directory.clone())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap_err();
    assert!(matches!(
        io(&second).kind(),
        std::io::ErrorKind::PermissionDenied
            | std::io::ErrorKind::IsADirectory
            | std::io::ErrorKind::AlreadyExists
    ));
    let healthy = root.join("healthy.json");
    diagnostics
        .export(healthy.clone())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    diagnostics
        .checkpoint()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(diagnostics.status()["localWriteErrors"], 2);
    let saved: Value = serde_json::from_slice(&std::fs::read(healthy).unwrap()).unwrap();
    assert_eq!(saved["recent"], before);
    assert_eq!(saved["reporting"]["localWriteErrors"], 2);
    assert_eq!(std::fs::read(&occupied).unwrap(), b"keep original file");
    assert_eq!(
        std::fs::read(directory.join("keep")).unwrap(),
        b"keep original directory"
    );
    assert!(!std::fs::read_dir(&root).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".motion-diagnostics-")));
    std::fs::write(root.join("result.json"), serde_json::to_vec_pretty(&json!({
        "first": {"kind":format!("{:?}", io(&first).kind()), "osCode":io(&first).raw_os_error()},
        "second": {"kind":format!("{:?}", io(&second).kind()), "osCode":io(&second).raw_os_error()},
        "sameNativeCauseAfterClone":true, "textCannotForgeType":true,
        "actualReporterBlockedAfterStart":true, "recentRecordsUnchanged":true,
        "originalPathsAndCleanupPreserved":true, "healthyExportAndCheckpointRecovered":true,
    })).unwrap()).unwrap();
    drop(diagnostics);
    wait_for(
        &root.join("motion-studio.log"),
        "diagnostics reporter stopped",
    );
}
