use super::*;

#[test]
fn color_command_cause_keeps_native_io_after_a_long_path_without_replacing_error() {
    let root = tempfile::tempdir().unwrap();
    let io = std::fs::File::open(root.path().join("absent-ocio-manifest.json")).unwrap_err();
    let original_cause = io.to_string();
    let original_kind = io.kind();
    let path = "界".repeat(6000);
    let error = motion_host::Error::ColorPreparation(motion_host::color_preparation::Error::Provider {
        target: motion_host::color_preparation::Target::DefaultInput,
        phase: motion_effects::color_processor::ColorProcessorPhase::InputToWorking,
        source: Box::new(motion_host::Error::NativeColor(motion_effects::native_color::Error::Io {
            phase: "manifest", path: path.clone(), source: io,
        })),
    });
    let text = command_cause(&error);
    assert!(text.contains("provider failed") && text.contains("phase=manifest"), "{text}");
    assert!(text.contains(&original_cause), "{text}");
    assert!(text.chars().count() <= 2048);
    assert!(!text.contains(&path));
    // Negative control: the old outer-only formatting loses the native cause.
    assert!(!crate::fallback_observation::error_cause(&error).contains("phase=manifest"));
    let motion_host::Error::ColorPreparation(motion_host::color_preparation::Error::Provider { source, .. }) = error else { unreachable!() };
    let motion_host::Error::NativeColor(motion_effects::native_color::Error::Io { phase, path: retained, source }) = *source else { unreachable!() };
    assert_eq!(phase, "manifest");
    assert_eq!(retained, path);
    assert_eq!(source.kind(), original_kind);
    assert_eq!(source.to_string(), original_cause);
    let ordinary = motion_host::Error::from("ordinary host failure");
    assert_eq!(command_cause(&ordinary), crate::fallback_observation::error_cause(&ordinary));
}
use crate::engine::{respond, Command, Engine};
use serde_json::json;
use std::{path::PathBuf, sync::mpsc::channel};

#[derive(Debug)]
struct Cause(u64);
impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("owned IO cause")
    }
}
impl std::error::Error for Cause {}

#[test]
fn reply_preserves_native_cause_identity_until_last_owner_exits() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Error>();
    let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, Cause(43));
    let original = io.get_ref().unwrap().downcast_ref::<Cause>().unwrap() as *const Cause as usize;
    let (sender, receiver) = channel::<Result<(), Error>>();
    std::thread::spawn(move || respond(sender, Err(motion_host::Error::Io(io))))
        .join()
        .unwrap();
    let received = receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap_err();
    let Some(motion_host::Error::Io(io)) = received.host() else {
        panic!("lost IO variant: {received:?}")
    };
    assert_eq!(io.kind(), std::io::ErrorKind::PermissionDenied);
    let cause = io.get_ref().unwrap().downcast_ref::<Cause>().unwrap();
    assert_eq!(cause as *const Cause as usize, original);
    assert_eq!(cause.0, 43);
    let Error::Host(native) = &received else {
        panic!("lost host owner")
    };
    let weak = Arc::downgrade(native);
    let copy = received.clone();
    assert_eq!(received, copy);
    assert_ne!(received, Error::from(received.to_string()));
    drop(received);
    assert!(weak.upgrade().is_some());
    drop(copy);
    assert!(weak.upgrade().is_none());
}

#[test]
fn structured_details_require_native_identity_and_survive_disconnection_context() {
    let detail = motion_core::composition::CompositionError {
        code: "composition_missing".into(),
        composition: "missing-child".into(),
        message: "Composition does not exist".into(),
        details: json!({"object": 7}),
    };
    let native = Error::from(motion_host::Error::Composition(detail.clone()));
    let text = native.to_string();
    assert_eq!(native.envelope()["error_detail"], json!(detail));
    assert!(Error::from(text.clone()).envelope()["error_detail"].is_null());
    assert!(motion_host::ops::error_envelope(&text)["error_detail"].is_null());
    let disconnected = Error::ReplyDisconnected {
        operation: "edit".into(),
        cause: Box::new(native.clone()),
    };
    assert_eq!(disconnected.envelope()["error_detail"], json!(detail));
    let Some(motion_host::Error::Composition(cause)) = disconnected.host() else {
        panic!("lost composition identity")
    };
    assert!(std::ptr::eq(cause, native.composition_detail().unwrap()));
    assert_eq!(
        serde_json::to_value(&disconnected).unwrap(),
        json!(disconnected.to_string())
    );
}

#[test]
fn serial_worker_preserves_native_errors_and_default_output_channels() {
    const CHILD: &str = "MOTION_TYPED_WORKER_ERROR_CHILD";
    const TEST: &str =
        "engine::error::tests::serial_worker_preserves_native_errors_and_default_output_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project = root.join("project");
        let engine = Engine::start(project, motion_desktop_media::platform(), 8 << 30).unwrap();
        let before = engine.state().unwrap();
        let missing = root.join("missing-native-input.png");
        let expected = std::fs::metadata(&missing).unwrap_err();
        let pending = engine
            .request(|reply| Command::ImageInfo {
                path: missing.to_str().unwrap().into(),
                reply,
            })
            .unwrap();
        let error = pending
            .recv_timeout(Duration::from_secs(60))
            .unwrap()
            .unwrap_err();
        let Some(motion_host::Error::ImageResource(motion_render::image_resources::Error::Io(io))) =
            error.host()
        else {
            panic!("worker erased native image cause: {error:?}")
        };
        assert_eq!(io.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(io.raw_os_error(), expected.raw_os_error());
        let error = engine.edit("{".into()).unwrap_err();
        let Some(motion_host::Error::Model(motion_core::Error::Json(cause))) = error.host() else {
            panic!("worker erased native parse cause: {error:?}")
        };
        assert!(cause.is_eof());
        assert_eq!(cause.line(), 1);
        let error = engine.edit(json!({"op":"composition","action":{"kind":"reference","target":"comp-missing","at_frame":0}}).to_string()).unwrap_err();
        assert!(matches!(
            error.host(),
            Some(motion_host::Error::Model(motion_core::Error::Composition(
                _
            )))
        ));
        assert_eq!(
            error.composition_detail().unwrap().code,
            "composition_missing"
        );
        assert_eq!(
            error.envelope()["error_detail"]["code"],
            "composition_missing"
        );
        let after = engine.state().unwrap();
        for field in ["project", "revision", "history", "canUndo", "canRedo"] {
            assert_eq!(after[field], before[field], "changed {field}");
        }
        assert!(!engine.failed());
        // A departed receiver is normal; the next actual command still completes.
        drop(engine.request(Command::State).unwrap());
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
        drop(engine);
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    if let Some(evidence) = std::env::var_os("MOTION_ERROR_TRANSPORT_EVIDENCE") {
        let evidence = PathBuf::from(evidence);
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(evidence.join("worker.stdout"), &output.stdout).unwrap();
        std::fs::write(evidence.join("worker.stderr"), &output.stderr).unwrap();
        if output.status.success() {
            std::fs::copy(
                root.path().join("motion-studio.log"),
                evidence.join("worker.log"),
            )
            .unwrap();
        }
    }
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for text in [&file, &stderr] {
        let errors: Vec<_> = text
            .lines()
            .filter(|line| line.contains("motion_studio::engine:") && line.contains("error #"))
            .collect();
        assert_eq!(errors.len(), 3, "{text}");
        assert_eq!(
            errors
                .iter()
                .filter(|line| line.contains(" image_info "))
                .count(),
            1
        );
        assert_eq!(
            errors.iter().filter(|line| line.contains(" edit ")).count(),
            2
        );
    }
    for line in file.lines().filter(|line| line.contains("error #")) {
        let command = line
            .split_once("error #")
            .unwrap()
            .1
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(file.find(&format!("start #{command}")).unwrap() < file.find(line).unwrap());
    }
    assert!(!stdout.contains("error #"));
    assert!(!stdout.contains("start #"));
    assert!(!stdout.contains("missing-native-input.png"));
}

#[test]
fn timeout_remains_pending_and_publishes_typed_request_identity() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        root.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let (entered, waiting) = channel();
    let (release, gate) = channel();
    let error = engine
        .call_with_timeout(
            |reply| Command::TestWait {
                entered,
                release: gate,
                reply,
            },
            Duration::from_millis(4),
        )
        .unwrap_err();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    let Error::Timeout {
        request,
        operation,
        waited,
    } = error
    else {
        panic!("lost timeout classification: {error:?}")
    };
    assert!(request > 0);
    assert!(waited >= Duration::from_millis(4));
    let flight = *engine.in_flight.lock().unwrap();
    assert_eq!(flight.unwrap().request, request);
    assert_eq!(flight.unwrap().operation, operation);
    release.send(()).unwrap();
    assert!(engine.state().is_ok());
    assert!(!engine.failed());
    engine.request_shutdown().unwrap();
    assert!(matches!(engine.state(), Err(Error::ShuttingDown)));
}

#[test]
fn startup_lock_and_capability_requests_preserve_native_errors_and_default_channels() {
    const CHILD: &str = "MOTION_TYPED_STARTUP_ERROR_CHILD";
    const TEST: &str = "engine::error::tests::startup_lock_and_capability_requests_preserve_native_errors_and_default_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project = root.join("project");
        let engine =
            Engine::start(project.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        let before = engine.state().unwrap();
        let lock = match Engine::start(project.clone(), motion_desktop_media::platform(), 8 << 30) {
            Ok(_) => panic!("concurrent editor acquired the same project"),
            Err(error) => error,
        };
        let Some(motion_host::Error::Context { context, cause }) = lock.host() else {
            panic!("startup erased native lock identity: {lock:?}");
        };
        assert_eq!(context, "project is already open or cannot be locked");
        assert!(matches!(
            cause.as_ref(),
            motion_host::Error::FileLock(std::fs::TryLockError::WouldBlock)
        ));
        assert!(lock
            .to_string()
            .starts_with("project is already open or cannot be locked: "));
        let malformed = engine.media_capabilities("{}".into()).unwrap_err();
        let Some(motion_host::Error::Context { context, cause }) = malformed.host() else {
            panic!("capability request erased native JSON identity: {malformed:?}");
        };
        assert_eq!(context, "media request");
        let motion_host::Error::Json(json) = cause.as_ref() else {
            panic!("capability request has the wrong cause: {cause:?}");
        };
        assert!(json.is_data());
        assert!(json.to_string().contains("op"));
        let truncated = engine.media_capabilities("{".into()).unwrap_err();
        assert!(matches!(truncated.host(), Some(motion_host::Error::Json(json)) if json.is_eof()));
        assert!(engine
            .media_capabilities(r#"{"op":"media_capabilities"}"#.into())
            .unwrap()
            .is_object());
        let after = engine.state().unwrap();
        for field in ["project", "revision", "history", "canUndo", "canRedo"] {
            assert_eq!(after[field], before[field], "changed {field}");
        }
        assert!(!engine.failed());
        drop(engine);
        let reopened = Engine::start(project, motion_desktop_media::platform(), 8 << 30).unwrap();
        assert_eq!(reopened.state().unwrap()["project"], before["project"]);
        drop(reopened);
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    if let Some(evidence) = std::env::var_os("MOTION_ERROR_TRANSPORT_EVIDENCE") {
        let evidence = PathBuf::from(evidence).join("startup-capabilities");
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(evidence.join("worker.stdout"), &output.stdout).unwrap();
        std::fs::write(evidence.join("worker.stderr"), &output.stderr).unwrap();
        std::fs::write(evidence.join("worker.log"), &file).unwrap();
    }
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for text in [&file, &stderr] {
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("worker startup lock failed:"))
                .count(),
            1,
            "{text}"
        );
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("error #") && line.contains(" media_capabilities "))
                .count(),
            2,
            "{text}"
        );
    }
    assert!(
        file.find("worker startup phase=project_lock").unwrap()
            < file.find("worker startup lock failed:").unwrap()
    );
    for line in file.lines().filter(|line| line.contains("error #")) {
        let command = line
            .split_once("error #")
            .unwrap()
            .1
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(file.find(&format!("start #{command}")).unwrap() < file.find(line).unwrap());
    }
    assert!(!stdout.contains("worker startup lock failed"));
    assert!(!stdout.contains("error #"));
    assert!(!stdout.contains("start #"));
}
