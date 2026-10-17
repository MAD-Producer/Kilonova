use super::{last_error_observation_tests::Stub, metadata_observation::modified, *};
use std::io;

struct Capture(Mutex<Vec<(log::Level, String)>>);
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if text.starts_with("preview image mtime unavailable ") {
            assert!(
                !matches!(
                    sessions().try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ),
                "registry held during metadata log"
            );
            self.0.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}

#[test]
fn unavailable_mtime_keeps_optional_fallback_and_logs_after_session_unlock() {
    const TEST: &str = "session::metadata_observation_tests::unavailable_mtime_keeps_optional_fallback_and_logs_after_session_unlock";
    const CHILD: &str = "MOTION_MTIME_OBSERVATION_TEST";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["direct", "episodes", "primary", "unwind"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, mode)
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!String::from_utf8_lossy(&output.stdout)
                .contains("site=host.preview_input.modified"));
        }
        return;
    };
    let capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image");
    std::fs::write(&path, b"original fixture").unwrap();
    let time = std::fs::metadata(&path).unwrap().modified().unwrap();
    for _ in 0..120 {
        assert_eq!(
            modified(std::fs::metadata(&path).unwrap().modified(), 7, &path),
            Some(time)
        );
    }
    assert!(capture.0.lock().unwrap().is_empty());
    // Unsupported mtime is a controlled OS-result input to the production
    // helper; the healthy metadata calls above use the actual filesystem.
    let failed = || {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "mtime unsupported",
        ))
    };
    if mode == "direct" {
        let long_path = temp.path().join("sensitive-敏".repeat(2000));
        let cause = "private-cause-敏".repeat(2000);
        assert_eq!(
            modified(
                Err(io::Error::new(io::ErrorKind::Unsupported, cause.clone())),
                7,
                &long_path
            ),
            None
        );
        let records = capture.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, log::Level::Warn);
        assert!(records[0].1.chars().count() < 850);
        assert!(!records[0].1.contains(&cause));
        assert!(!records[0]
            .1
            .contains(&long_path.to_string_lossy().to_string()));
        assert!(records[0].1.contains("source_id=7 "));
        assert!(records[0].1.contains("fallback=length_only"));
        return;
    }
    let id = open(
        temp.path().to_path_buf(),
        Project::new(64, 64, 30, 120).unwrap(),
        Arc::new(Stub),
    )
    .unwrap();
    if mode == "unwind" {
        let result = std::panic::catch_unwind(|| {
            with_session(id, |_| -> Result<()> {
                assert_eq!(modified(failed(), 7, &path), None);
                assert!(capture.0.lock().unwrap().is_empty());
                panic!("controlled metadata unwind");
            })
        });
        assert!(result.is_err());
        assert!(sessions().is_poisoned());
        assert_eq!(capture.0.lock().unwrap().len(), 1);
        return;
    }
    if mode == "primary" {
        let result = with_session(id, |_| -> Result<()> {
            assert_eq!(modified(failed(), 7, &path), None);
            assert!(capture.0.lock().unwrap().is_empty());
            Err("primary failure retained".into())
        });
        assert_eq!(result.unwrap_err(), "primary failure retained");
        assert_eq!(capture.0.lock().unwrap().len(), 1);
        return;
    }
    for _ in 0..120 {
        let before = capture.0.lock().unwrap().len();
        with_session(id, |_| {
            assert_eq!(modified(failed(), 7, &path), None);
            assert_eq!(capture.0.lock().unwrap().len(), before);
            assert_eq!(
                modified(std::fs::metadata(&path).unwrap().modified(), 8, &path),
                Some(time)
            );
            Ok(())
        })
        .unwrap();
    }
    let records = capture.0.lock().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].0, log::Level::Warn);
    assert_eq!(records[1].0, log::Level::Error);
    assert!(records[0].1.contains("lifetime_count=1 "));
    assert!(records[1].1.contains("lifetime_count=60 "));
}
