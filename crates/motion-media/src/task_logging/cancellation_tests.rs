use super::*;
use crate::{
    AudioJobs, DecodeAudio, ImportOptions, Limits, PackageJobs, VideoImportOptions, VideoJobs,
};
use std::{
    path::Path,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    panic: bool,
}
impl Gate {
    fn block(&self) {
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(15))
            .unwrap();
    }
    fn fail<T>(&self) -> crate::Result<T> {
        self.block();
        if self.panic {
            panic!("controlled late native panic");
        }
        Err("controlled late provider failure".into())
    }
}
struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
fn make_gate(panic: bool) -> (Arc<Gate>, mpsc::Receiver<()>, Release) {
    let (entered, waiting) = mpsc::channel();
    let (release, receive) = mpsc::channel();
    (
        Arc::new(Gate {
            entered,
            release: Mutex::new(receive),
            panic,
        }),
        waiting,
        Release(release),
    )
}
struct Capture {
    events: Mutex<Vec<(log::Level, String)>>,
    package_gate: Mutex<Option<Arc<Gate>>>,
}
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        self.events
            .lock()
            .unwrap()
            .push((record.level(), message.clone()));
        if message.starts_with("media start request=cancel-observation-package") {
            let gate = self.package_gate.lock().unwrap().clone().unwrap();
            gate.block();
        }
    }
    fn flush(&self) {}
}
enum Jobs {
    Audio(AudioJobs),
    Video(VideoJobs),
    Package(PackageJobs),
}
impl Jobs {
    fn cancel(&self, id: &str) {
        match self {
            Self::Audio(j) => {
                j.cancel(id).unwrap();
            }
            Self::Video(j) => {
                j.cancel(id).unwrap();
            }
            Self::Package(j) => {
                j.cancel(id).unwrap();
            }
        }
    }
    fn status(&self, id: &str) -> (String, Option<String>) {
        match self {
            Self::Audio(j) => {
                let s = j.status(id).unwrap();
                (s.state, s.error)
            }
            Self::Video(j) => {
                let s = j.status(id).unwrap();
                (s.state, s.error)
            }
            Self::Package(j) => {
                let s = j.status(id).unwrap();
                (s.state, s.error)
            }
        }
    }
}
fn video_asset() -> motion_model::VideoAsset {
    serde_json::from_value(serde_json::json!({
        "id":1,"path":"assets/test.mp4","bytes":4,"mime":"video/avc","track":0,
        "width":2,"height":2,"rotation":0,"display_width":2,"display_height":2,
        "video_start_us":0,"video_end_us":66666,"duration_us":66666,
        "frame_count":2,"variable_frame_rate":false,"nominal_frame_rate":30.0,
        "color_standard":1,"color_range":1,"audio_asset":null
    }))
    .unwrap()
}
fn open(
    gate: &Gate,
    cooperative: bool,
) -> crate::Result<(Box<dyn std::io::Read + Send>, Option<u64>)> {
    if cooperative {
        gate.block();
        Ok((Box::new(std::io::Cursor::new(Vec::<u8>::new())), Some(0)))
    } else {
        gate.fail()
    }
}
fn start(root: &Path, operation: &str, id: &str, gate: Arc<Gate>, cooperative: bool) -> Jobs {
    match operation {
        "import_audio" => {
            let decode: DecodeAudio = Arc::new(|_, _, _, _, _| Err("unused decoder".into()));
            let jobs = AudioJobs::with_decoder(root.into(), Limits::default(), decode).unwrap();
            jobs.start_with_source(
                move || open(&gate, cooperative),
                ImportOptions {
                    request_id: id.into(),
                    at_frame: 0,
                    track: None,
                    name: "test".into(),
                },
                false,
            )
            .unwrap();
            Jobs::Audio(jobs)
        }
        "prepare_audio" => {
            let decode: DecodeAudio = Arc::new(move |_, _, _, _, progress| {
                if cooperative {
                    gate.block();
                    progress(0.0)?;
                    panic!("cancelled progress must reject work");
                }
                gate.fail()
            });
            let jobs = AudioJobs::with_decoder(root.into(), Limits::default(), decode).unwrap();
            jobs.prepare_cache(
                id,
                motion_model::AudioAsset {
                    id: 1,
                    path: "assets/test.wav".into(),
                    bytes: 4,
                    mime: "audio/wav".into(),
                    track: 0,
                    sample_rate: 48000,
                    channels: 1,
                    sample_frames: 48000,
                    duration_us: 1_000_000,
                },
            )
            .unwrap();
            Jobs::Audio(jobs)
        }
        "import_video" => {
            let jobs = VideoJobs::new(root.into()).unwrap();
            jobs.start(
                move || open(&gate, cooperative),
                VideoImportOptions {
                    request_id: id.into(),
                    at_frame: 0,
                    name: "test".into(),
                    track: None,
                    audio_track: None,
                    with_audio: false,
                },
                false,
                Arc::new(|_, _, _, _| Err("unused probe".into())),
            )
            .unwrap();
            Jobs::Video(jobs)
        }
        "prepare_video" => {
            let jobs = VideoJobs::new(root.into()).unwrap();
            jobs.prepare_cache(
                id,
                video_asset(),
                Arc::new(move |_, _, _, check| {
                    if cooperative {
                        gate.block();
                        check()?;
                        panic!("cancelled probe must reject work");
                    }
                    gate.fail()
                }),
            )
            .unwrap();
            Jobs::Video(jobs)
        }
        _ => unreachable!(),
    }
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "missing cancelled worker observation"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn assert_error(capture: &Capture, id: &str, operation: &str, reason: &str) {
    let marker = format!("media error request={id} operation={operation} phase=cancelled ");
    wait_until(|| {
        capture
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|(level, text)| *level == log::Level::Error && text.contains(&marker))
    });
    let events = capture.events.lock().unwrap();
    let errors = events
        .iter()
        .filter(|(level, text)| *level == log::Level::Error && text.contains(id))
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].1.contains(&marker));
    assert!(errors[0].1.contains("ms: "));
    assert!(errors[0].1.contains(reason), "{:?}", errors[0]);
    assert!(events.iter().any(|(level, text)| *level == log::Level::Info
        && text.contains(&format!("media start request={id} operation={operation}"))));
}

#[test]
fn cancelled_workers_preserve_returned_errors_and_panics_without_changing_status() {
    const CHILD: &str = "MOTION_MEDIA_CANCEL_OBSERVATION_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","task_logging::cancellation_tests::cancelled_workers_preserve_returned_errors_and_panics_without_changing_status"])
            .env(CHILD,"1").output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("media error"));
        return;
    }
    let capture = Box::leak(Box::new(Capture {
        events: Mutex::new(Vec::new()),
        package_gate: Mutex::new(None),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("assets")).unwrap();
    std::fs::write(root.path().join("assets/test.wav"), b"1234").unwrap();
    std::fs::write(root.path().join("assets/test.mp4"), b"1234").unwrap();
    for operation in [
        "import_audio",
        "prepare_audio",
        "import_video",
        "prepare_video",
    ] {
        for panic in [false, true] {
            let id = format!("cancel-observation-{operation}-{panic}");
            let (gate, entered, release) = make_gate(panic);
            let jobs = start(root.path(), operation, &id, gate, false);
            entered.recv_timeout(Duration::from_secs(5)).unwrap();
            jobs.cancel(&id);
            release.0.send(()).unwrap();
            assert_error(
                capture,
                &id,
                operation,
                if panic {
                    "worker panic: controlled late native panic"
                } else {
                    "controlled late provider failure"
                },
            );
            assert_eq!(
                jobs.status(&id),
                ("cancelled".into(), None),
                "late failure must not overwrite cancellation"
            );
        }
    }
    // Block before actual IO using the sink, without a production test hook.
    // Cancellation remains pending until the real failed preparation is dropped.
    let (gate, entered, release) = make_gate(false);
    *capture.package_gate.lock().unwrap() = Some(gate);
    let blocked = root.path().join("blocked");
    std::fs::write(&blocked, b"original output parent").unwrap();
    let expected_io_error = std::fs::create_dir_all(&blocked).unwrap_err().to_string();
    let project = motion_core::Project::new(64, 64, 30, 30).unwrap();
    let snapshot = motion_core::storage::PackageSnapshot::new(root.path(), &project).unwrap();
    let packages = PackageJobs::default();
    packages
        .start_export(
            "cancel-observation-package",
            snapshot,
            blocked.join("out.msproj"),
            0,
        )
        .unwrap();
    let jobs = Jobs::Package(packages);
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    jobs.cancel("cancel-observation-package");
    release.0.send(()).unwrap();
    assert_error(
        capture,
        "cancel-observation-package",
        "export_project",
        &expected_io_error,
    );
    assert_eq!(
        jobs.status("cancel-observation-package"),
        ("cancelled".into(), None)
    );
    assert_eq!(std::fs::read(blocked).unwrap(), b"original output parent");
    assert!(!root.path().join("blocked/out.msproj").exists());

    // Debug is enabled only to wait for the existing completion event of a
    // cooperative cancellation. All real-failure checks above ran at Info.
    log::set_max_level(log::LevelFilter::Debug);
    for operation in [
        "import_audio",
        "prepare_audio",
        "import_video",
        "prepare_video",
        "export_project",
    ] {
        let id = if operation == "export_project" {
            "cancel-observation-package-normal".into()
        } else {
            format!("cancel-observation-{operation}-normal")
        };
        let (gate, entered, release) = make_gate(false);
        let jobs = if operation == "export_project" {
            *capture.package_gate.lock().unwrap() = Some(gate);
            let snapshot =
                motion_core::storage::PackageSnapshot::new(root.path(), &project).unwrap();
            let packages = PackageJobs::default();
            packages
                .start_export(&id, snapshot, root.path().join("normal.msproj"), 0)
                .unwrap();
            Jobs::Package(packages)
        } else {
            start(root.path(), operation, &id, gate, true)
        };
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        jobs.cancel(&id);
        release.0.send(()).unwrap();
        let marker =
            format!("media worker done request={id} operation={operation} phase=cancelled ");
        wait_until(|| {
            capture
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|(level, text)| *level == log::Level::Debug && text.contains(&marker))
        });
        assert!(
            !capture
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|(level, text)| *level == log::Level::Error && text.contains(&id)),
            "normal cancellation must not fabricate a failure: {operation}"
        );
        assert_eq!(jobs.status(&id), ("cancelled".into(), None));
    }
    assert!(!root.path().join("normal.msproj").exists());
}

#[test]
fn expected_cancellation_and_success_have_no_failure_but_panics_keep_bounded_detail() {
    for reason in [
        "audio task cancelled",
        "video import cancelled",
        "video cache cancelled",
        "package export cancelled",
    ] {
        let result: std::thread::Result<crate::Result<()>> = Ok(Err(reason.into()));
        assert!(cancelled_error(&result, &[reason]).is_none());
        assert_eq!(cancelled_error(&result, &[]).as_deref(), Some(reason));
    }
    let success: std::thread::Result<crate::Result<()>> = Ok(Ok(()));
    assert!(cancelled_error(&success, &[]).is_none());
    let panic: std::thread::Result<crate::Result<()>> = Err(Box::new("故".repeat(4096)));
    let text = cancelled_error(&panic, &[]).unwrap();
    assert!(text.starts_with("worker panic: "));
    assert_eq!(
        text.strip_prefix("worker panic: ").unwrap().chars().count(),
        2048
    );
    let unknown: std::thread::Result<crate::Result<()>> = Err(Box::new(23u32));
    assert_eq!(
        cancelled_error(&unknown, &[]).as_deref(),
        Some("worker panic: unknown native error")
    );
}
