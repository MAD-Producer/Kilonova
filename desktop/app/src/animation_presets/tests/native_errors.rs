use super::*;
use crate::engine::Error;
use std::error::Error as StdError;

fn operation(error: &Error) -> &super::super::error::Operation {
    let motion_host::Error::NativeSource { cause } = error.host().expect("native host cause") else {
        panic!("preset cause must retain its integration wrapper");
    };
    cause.downcast_ref().expect("native preset operation")
}

#[derive(Debug)]
struct Marker {
    identity: Arc<()>,
    message: String,
}
impl std::fmt::Display for Marker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl StdError for Marker {}

#[test]
fn bounded_diagnostic_preserves_native_identity_and_cannot_forge_cancellation() {
    let identity = Arc::new(());
    let original = std::io::Error::other(Marker {
        identity: identity.clone(),
        message: "x".repeat(900),
    });
    let error = io_error("read preset", Path::new("preset.msanim"), original);
    let clone = error.clone();
    let original_operation = operation(&error) as *const super::super::error::Operation;
    assert!(error.to_string().len() < 600);
    let native = operation(&error);
    assert!(native.source().unwrap().is::<motion_host::Error>());
    let motion_host::Error::Io(io) = &native.cause else { panic!("lost IO identity") };
    let marker = io.get_ref().unwrap().downcast_ref::<Marker>().unwrap();
    assert!(Arc::ptr_eq(&identity, &marker.identity));
    assert_eq!(marker.message.len(), 900);
    assert_eq!(Arc::strong_count(&identity), 2);
    let forged = Error::from(error.to_string());
    assert_eq!(forged.to_string(), error.to_string());
    assert!(forged.source().is_none() && forged.host().is_none());
    drop(error);
    assert_eq!(Arc::strong_count(&identity), 2);
    assert_eq!(operation(&clone) as *const _, original_operation);
    drop(clone);
    assert_eq!(Arc::strong_count(&identity), 1);

    let reason = "animation preset operation cancelled";
    let checkpoint = io_error("read preset", Path::new("preset.msanim"),
        motion_core::Error::Invalid(reason.into()));
    let independent_io = io_error("read preset", Path::new("preset.msanim"),
        std::io::Error::other(reason));
    let panic = super::super::error::panic_error(reason);
    assert!(super::super::error::cancelled(&checkpoint));
    assert!(!super::super::error::cancelled(&independent_io));
    assert!(!super::super::error::cancelled(&panic));
    assert!(!super::super::error::cancelled(&Error::from(checkpoint.to_string())));
}

#[test]
fn actual_catalog_worker_keeps_os_cause_default_channels_and_recovers() {
    const CHILD: &str = "MOTION_PRESET_NATIVE_CAUSE_CHILD";
    let Ok(root) = std::env::var(CHILD) else {
        let folder = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "animation_presets::tests::native_errors::actual_catalog_worker_keeps_os_cause_default_channels_and_recovers", "--nocapture"])
            .env(CHILD, folder.path()).env_remove("MOTION_LOG")
            .output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stdout}\n{stderr}");
        assert!(!stdout.contains("animation preset failed"));
        assert!(stderr.contains("animation preset failed") && stderr.contains("operation=scan"));
        if let Some(destination) = std::env::var_os("MOTION_PRESET_NATIVE_CAUSE_EVIDENCE") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in ["logs/motion-studio.log", "native-cause.json"] {
                std::fs::copy(folder.path().join(name), destination.join(Path::new(name).file_name().unwrap())).unwrap();
            }
            std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
        }
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let mut shell = fixture(&root.join("project"), Locale::En);
    let before = shell.engine.state().unwrap();
    let path = root.join("catalog");
    std::fs::write(&path, b"not a directory").unwrap();
    let reference = std::fs::read_dir(&path).unwrap_err();
    shell.animation_presets.scan(path.clone()).unwrap();
    let start = Instant::now();
    let error = loop {
        assert!(start.elapsed() < Duration::from_secs(30));
        if let Some(result) = shell.animation_presets.poll(&shell.engine, true) {
            break match result { Err(error) => error, Ok(_) => panic!("invalid catalog succeeded") };
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let motion_host::Error::Io(native) = &operation(&error).cause else { panic!("lost native directory IO") };
    assert_eq!(native.kind(), reference.kind());
    assert_eq!(native.raw_os_error(), reference.raw_os_error());
    assert!(native.raw_os_error().is_some());
    let retained = error.clone();
    assert!(std::ptr::eq(operation(&error), operation(&retained)));
    assert!(!shell.animation_presets.busy());
    let after = shell.engine.state().unwrap();
    assert_eq!(after["project"], before["project"]);
    assert_eq!(after["revision"], before["revision"]);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    shell.animation_presets.scan(path).unwrap();
    assert!(matches!(wait(&mut shell.animation_presets, &shell.engine, true).unwrap(), Completion::Catalog { entries, failures: 0 } if entries.is_empty()));
    let recovered = shell.engine.state().unwrap();
    assert_eq!(recovered["project"], before["project"]);
    assert_eq!(recovered["revision"], before["revision"]);
    crate::logging::flush();
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let start = file.find("animation preset start").unwrap();
    let failure = file.find("animation preset failed").unwrap();
    assert!(start < failure);
    assert!(file.contains(" ERROR ") && file.contains("operation=scan phase=io"));
    assert!(file.contains("animation preset controller failed"));
    let record = serde_json::json!({"kind":format!("{:?}",native.kind()),"os_error":native.raw_os_error(),"same_native_identity":true,"project_and_revision_preserved":true,"recovered_catalog_empty":true});
    std::fs::write(root.join("native-cause.json"), serde_json::to_vec_pretty(&record).unwrap()).unwrap();
}
