use super::*;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::channel,
    },
    time::{Duration, Instant},
};

fn status(error: Option<String>) -> Arc<Mutex<ExportStatus>> {
    Arc::new(Mutex::new(ExportStatus {
        state: if error.is_some() { "failed" } else { "running" },
        progress: 0.,
        revision: 77,
        result: None,
        error,
    }))
}

#[test]
fn shutdown_cancels_then_waits_for_owned_source_release() {
    struct Source(std::fs::File, Arc<AtomicBool>);
    impl Drop for Source {
        fn drop(&mut self) {
            self.1.store(true, Ordering::Release);
        }
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.bin");
    std::fs::write(&path, b"owned export source").unwrap();
    let freed = Arc::new(AtomicBool::new(false));
    let source = Source(std::fs::File::open(path).unwrap(), freed.clone());
    let control = ExportControl::default();
    let cancel = control.clone();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let worker = std::thread::spawn(move || {
        assert_eq!(source.0.metadata().unwrap().len(), 19);
        entered.send(()).unwrap();
        released.recv().unwrap();
        assert_eq!(cancel.check().unwrap_err(), "movie export cancelled");
        drop(source);
    });
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    let task = MovieExport {
        control: control.clone(),
        status: status(None),
        worker: Some(worker),
        revision: 77,
        output: root.path().join("out.mp4"),
        began: Instant::now(),
    };
    let (exited, exit) = channel();
    let owner = std::thread::spawn(move || {
        drop(task);
        exited.send(()).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while control.check().is_ok() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(matches!(
        exit.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert!(!freed.load(Ordering::Acquire));
    release.send(()).unwrap();
    exit.recv_timeout(Duration::from_secs(5)).unwrap();
    owner.join().unwrap();
    assert!(freed.load(Ordering::Acquire));
}

#[test]
fn closure_panic_reaches_default_sinks_and_preserves_an_existing_primary_error() {
    const CHILD: &str = "MOTION_MOVIE_JOIN_LOG_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        for primary in [None, Some("original codec failure".to_string())] {
            let state = status(primary.clone());
            let control = ExportControl::default();
            let (entered, entry) = channel();
            let (release, released) = channel();
            let worker = std::thread::spawn(move || {
                entered.send(()).unwrap();
                released.recv().unwrap();
                panic!("controlled movie closure panic");
            });
            entry.recv_timeout(Duration::from_secs(5)).unwrap();
            let task = MovieExport {
                control: control.clone(),
                status: state.clone(),
                worker: Some(worker),
                revision: 77,
                output: root.join("out.mp4"),
                began: Instant::now(),
            };
            let owner = std::thread::spawn(move || drop(task));
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
                if control.check().is_err()
                    && text.contains("movie export shutdown start: revision=77")
                {
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            release.send(()).unwrap();
            owner.join().unwrap();
            let value = state.lock().unwrap();
            assert_eq!(value.state, "failed");
            if let Some(primary) = primary {
                assert_eq!(value.error.as_deref(), Some(primary.as_str()));
            } else {
                assert!(value
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("controlled movie closure panic"));
            }
        }
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","movie_export::tests::closure_panic_reaches_default_sinks_and_preserves_an_existing_primary_error","--nocapture","--test-threads=1"]).env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    assert_eq!(
        text.matches("movie export shutdown failed: revision=77")
            .count(),
        2
    );
    assert!(
        text.contains("cause=movie export worker closure failed: controlled movie closure panic")
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .matches("ERROR motion_studio::movie_export: movie export shutdown failed: revision=77")
            .count(),
        2
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("movie export shutdown"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("movie export shutdown start"));
}

#[cfg(feature = "ffmpeg")]
#[test]
fn real_movie_completion_and_cancellation_survive_owner_cleanup() {
    const CHILD: &str = "MOTION_MOVIE_OWNER_REAL_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        exercise_real_movie(&root);
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "movie_export::tests::real_movie_completion_and_cancellation_survive_owner_cleanup",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    assert!(text.contains("movie export cancelled: revision="));
    assert_eq!(text.matches("movie export failed: revision=").count(), 1);
    let failure = text
        .lines()
        .find(|line| line.contains("movie export failed: revision="))
        .unwrap();
    assert!(
        failure.contains("blocked") && failure.contains("reason="),
        "{failure}"
    );
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("ERROR motion_studio::movie_export: movie export failed: revision="));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("movie export cancelled: revision="));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("movie export start:"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("movie export failed:"));
}

#[cfg(feature = "ffmpeg")]
fn exercise_real_movie(root: &std::path::Path) {
    let project = root.join("project");
    motion_core::storage::save(&project, &motion_core::Project::new(32, 32, 30, 2).unwrap())
        .unwrap();
    let engine =
        crate::engine::Engine::start(project, motion_desktop_media::platform(), 8 << 30).unwrap();
    for cancelled in [false, true] {
        let control = ExportControl::default();
        let snapshot = engine
            .freeze_movie(control.clone())
            .unwrap()
            .recv_timeout(Duration::from_secs(15))
            .unwrap()
            .unwrap();
        if cancelled {
            assert!(control.cancel());
        }
        let output = root.join(if cancelled {
            "cancelled.mp4"
        } else {
            "saved.mp4"
        });
        let task = MovieExport::start_with_control(snapshot, output.clone(), control).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !task.finished() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let before = task.status();
        let shared = task.status.clone();
        drop(task);
        let after = shared.lock().unwrap();
        assert_eq!(after.state, before.state);
        assert_eq!(after.error, before.error);
        if cancelled {
            assert_eq!(after.state, "cancelled");
            assert_eq!(after.error.as_deref(), Some("movie export cancelled"));
            assert!(!output.exists());
        } else {
            assert_eq!(after.state, "succeeded");
            assert_eq!(after.result.as_ref().unwrap().frames, 2);
            assert_eq!(
                motion_desktop_media::verify_video_runtime(&output).unwrap()["sourceFrames"],
                2
            );
        }
    }
    let control = ExportControl::default();
    let snapshot = engine
        .freeze_movie(control.clone())
        .unwrap()
        .recv_timeout(Duration::from_secs(15))
        .unwrap()
        .unwrap();
    let blocked = root.join("blocked");
    std::fs::write(&blocked, b"file instead of directory").unwrap();
    let task = MovieExport::start_with_control(snapshot, blocked.join("out.mp4"), control).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !task.finished() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let before = task.status();
    let shared = task.status.clone();
    drop(task);
    let after = shared.lock().unwrap();
    assert_eq!(after.state, "failed");
    assert_eq!(after.error, before.error);
    assert!(!after.error.as_ref().unwrap().is_empty());
    assert_eq!(
        std::fs::read(blocked).unwrap(),
        b"file instead of directory"
    );
}
