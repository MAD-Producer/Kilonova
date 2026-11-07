//! A real shaping failure crosses the actual worker/reply/logger boundary.
use super::*;
use motion_ui::text::{shaping::ShapeError, FontError};

fn native(error: &crate::engine::Error) -> &FontError {
    let motion_host::Error::NativeSource { cause } = error.host().expect("Host wrapper") else {
        panic!("font failure did not retain its native wrapper: {error:?}")
    };
    cause.downcast_ref().expect("FontError source")
}

#[test]
fn actual_font_failure_keeps_worker_identity_context_default_channels_and_recovery() {
    const CHILD: &str = "MOTION_FONT_WORKER_CHILD";
    const TEST:&str="plugin_editor::window::host::font_error_tests::actual_font_failure_keeps_worker_identity_context_default_channels_and_recovery";
    let Some(root) = std::env::var_os(CHILD) else {
        let folder = tempfile::tempdir().unwrap();
        let mut process = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, folder.path())
            .env_remove("MOTION_LOG")
            .stdout(std::fs::File::create(folder.path().join("stdout.log")).unwrap())
            .stderr(std::fs::File::create(folder.path().join("stderr.log")).unwrap())
            .spawn()
            .unwrap();
        let began = Instant::now();
        let status = loop {
            if let Some(status) = process.try_wait().unwrap() {
                break status;
            }
            if began.elapsed() > Duration::from_secs(60) {
                process.kill().unwrap();
                panic!("font worker test exceeded watchdog: {:?}", process.wait());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let stdout = std::fs::read_to_string(folder.path().join("stdout.log")).unwrap();
        let stderr = std::fs::read_to_string(folder.path().join("stderr.log")).unwrap();
        if let Some(destination) = std::env::var_os("MOTION_FONT_ERROR_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in [
                "stdout.log",
                "stderr.log",
                "motion-studio.log",
                "report.json",
            ] {
                let source = folder.path().join(name);
                if source.is_file() {
                    std::fs::copy(source, destination.join(name)).unwrap();
                }
            }
        }
        assert!(status.success(), "{status}\n{stdout}\n{stderr}");
        let file = std::fs::read_to_string(folder.path().join("motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        let errors: Vec<_> = file
            .lines()
            .filter(|line| line.contains("plugin.host error request=41 phase=font_validation"))
            .collect();
        assert_eq!(errors.len(), 1, "{file}");
        let error = errors[0];
        assert!(error.contains("elapsed_ms=") && error.contains("face_index=7 bytes=4"));
        let message = error.split_once("motion_studio::").unwrap().1;
        assert_eq!(stderr.matches(message).count(), 1, "{stderr}");
        assert!(!stdout.contains(message));
        let start = "plugin.host start request=41 phase=font_validation in_flight=true";
        assert!(file.find(start).unwrap() < file.find(message).unwrap());
        assert!(!stderr.contains(start) && !stdout.contains(start));
        assert!(file.contains("plugin.host start request=42 phase=font_cancel in_flight=true"));
        assert!(!file.contains("plugin.host error request=42"));
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let worker = Worker::new().unwrap();
    worker
        .send
        .send(Queued {
            flight: Flight {
                request: 41,
                phase: "font_validation",
                began: Instant::now(),
            },
            job: Job::FontInput {
                bytes: Arc::from([0u8; 4]),
                face_index: 7,
            },
        })
        .unwrap_or_else(|_| panic!("worker must accept font probe"));
    let result = worker.reply.recv_timeout(Duration::from_secs(5)).unwrap();
    let error = result.err().expect("actual invalid font must fail");
    let FontError::Shaping(ShapeError::InvalidFont { face_index, bytes }) = native(&error) else {
        panic!("{error:?}")
    };
    assert_eq!((*face_index, *bytes), (7, 4));
    let cloned = error.clone();
    assert!(std::ptr::eq(native(&error), native(&cloned)));
    assert!(worker.flight.lock().unwrap().is_none());
    assert!(error.envelope()["error"]
        .as_str()
        .unwrap()
        .contains("face_index=7 bytes=4"));
    // A normal empty job after the failure is still processed by the same
    // worker. It is not a restart, a retry, or a replacement of the failed reply.
    let (entered, receive) = mpsc::sync_channel(1);
    let (release, ready) = mpsc::sync_channel(1);
    worker
        .send
        .send(Queued {
            flight: Flight {
                request: 42,
                phase: "font_cancel",
                began: Instant::now(),
            },
            job: Job::Block {
                entered,
                release: ready,
            },
        })
        .unwrap_or_else(|_| panic!("same worker must accept next job"));
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(worker.flight.lock().unwrap().as_ref().unwrap().request, 42);
    release.send(()).unwrap();
    assert!(matches!(
        worker.reply.recv_timeout(Duration::from_secs(5)).unwrap(),
        Ok(Done::Retired)
    ));
    assert!(worker.flight.lock().unwrap().is_none());
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "actual_invalid_font":true,"native_reply_identity":true,"in_flight_cleared":true,
            "same_worker_recovery":true,"next_in_flight_published_before_handler_completion":true
        }))
        .unwrap(),
    )
    .unwrap();
    crate::logging::flush();
}
