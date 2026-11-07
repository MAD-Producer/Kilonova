//! Exercise the real shell entry points and the default file/stderr logger.
use crate::*;
use motion_ui::style::{Cause, Error as StyleError, Phase};

fn style_error(error: &engine::Error) -> &StyleError {
    let mut host = error.host().expect("native Host wrapper");
    while let motion_host::Error::Context { cause, .. } = host {
        host = cause;
    }
    let motion_host::Error::NativeSource { cause } = host else {
        panic!("style provider was flattened: {host:?}");
    };
    cause.downcast_ref().expect("original StyleError")
}

#[test]
fn actual_style_failures_keep_native_causes_atomic_settings_and_default_channels() {
    const TEST: &str = "shell::styles::native_error_tests::actual_style_failures_keep_native_causes_atomic_settings_and_default_channels";
    const CHILD: &str = "MOTION_STYLE_ERROR_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let folder = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, folder.path())
            .env_remove("MOTION_LOG")
            .stdout(std::fs::File::create(folder.path().join("stdout.log")).unwrap())
            .stderr(std::fs::File::create(folder.path().join("stderr.log")).unwrap())
            .spawn()
            .unwrap();
        let began = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if began.elapsed() > Duration::from_secs(180) {
                child.kill().unwrap();
                panic!("style error child exceeded watchdog: {:?}", child.wait());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let stdout = std::fs::read_to_string(folder.path().join("stdout.log")).unwrap();
        let stderr = std::fs::read_to_string(folder.path().join("stderr.log")).unwrap();
        if let Some(destination) = std::env::var_os("MOTION_STYLE_ERROR_ARTIFACTS") {
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
        let preferences: Vec<_> = file
            .lines()
            .filter(|line| line.contains("preferences failed operation=apply"))
            .collect();
        assert_eq!(preferences.len(), 3, "{file}");
        let records: Vec<_> = file
            .lines()
            .filter(|line| line.contains("site=styles.reload "))
            .collect();
        assert_eq!(records.len(), 4, "{file}");
        let degradation: Vec<_> = records
            .iter()
            .copied()
            .filter(|line| line.contains("ERROR"))
            .collect();
        assert_eq!(degradation.len(), 2, "{file}");
        let warnings: Vec<_> = records
            .iter()
            .copied()
            .filter(|line| line.contains("WARN"))
            .collect();
        assert_eq!(warnings.len(), 2, "{file}");
        for line in warnings {
            assert!(line.contains("count=1 "), "{line}");
            let message = line.split_once("motion_studio::").unwrap().1;
            assert!(!stderr.contains(message) && !stdout.contains(message));
        }
        for line in preferences.iter().chain(degradation.iter()) {
            assert!(
                line.contains("ERROR") && line.contains("elapsed_ms="),
                "{line}"
            );
            let message = line.split_once("motion_studio::").unwrap().1;
            assert!(stderr.contains(message), "{stderr}");
            assert!(!stdout.contains(message), "{stdout}");
        }
        assert!(preferences[0].contains("phase=Open") && preferences[0].contains("missing.json"));
        assert!(
            preferences[1].contains("phase=DecodeOverride") && preferences[1].contains("bad.json")
        );
        assert!(preferences[2].contains("phase=Compile"));
        for line in degradation {
            assert!(line.contains("count=60 ") && line.contains("fallback=previous_sheet"));
        }
        assert!(!stderr.contains("preferences start") && !stdout.contains("preferences start"));
        assert!(
            file.find("preferences start operation=apply").unwrap()
                < file.find("preferences failed operation=apply").unwrap()
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let mut shell = Shell::new(root.join("project"), None, Some(Locale::En)).unwrap();
    shell.layout_path = root.join("layout.json");
    let path = root.join("style.json");
    let healthy = br##"{"tokens":{"colors":{"focus":"#60c5a0"}}}"##;
    std::fs::write(&path, healthy).unwrap();
    shell.load_style_source(path.clone()).unwrap();
    let previous = shell.panels.styles.clone().unwrap();
    let settings = shell.workspace.preferences();
    let persisted = std::fs::read(&shell.layout_path).unwrap();
    let project = shell.engine.state().unwrap()["project"].clone();
    let bad = root.join("bad.json");
    for (candidate, bytes, phase) in [
        (root.join("missing.json"), None, Phase::Open),
        (bad.clone(), Some(b"{".as_slice()), Phase::DecodeOverride),
        (
            bad.clone(),
            Some(br#"{"styles":{"button":{"radius":-1}}}"#.as_slice()),
            Phase::Compile,
        ),
    ] {
        if let Some(bytes) = bytes {
            std::fs::write(&candidate, bytes).unwrap();
        }
        let mut rejected = settings.clone();
        rejected.style_path = Some(candidate.clone());
        let error = shell.apply_preferences(rejected, true).unwrap_err();
        let native = style_error(&error);
        assert_eq!(native.path.as_ref(), Some(&candidate));
        assert_eq!(native.phase, phase);
        match (&native.cause, phase) {
            (Cause::Io(cause), Phase::Open) => {
                assert_eq!(cause.kind(), std::io::ErrorKind::NotFound)
            }
            (Cause::Json(cause), Phase::DecodeOverride) => assert!(cause.is_eof()),
            (Cause::Invalid(_), Phase::Compile) => (),
            _ => panic!("wrong provider cause: {native:?}"),
        }
        let cloned = error.clone();
        assert!(std::ptr::eq(style_error(&cloned), native));
        assert_eq!(shell.workspace.preferences(), settings);
        assert_eq!(std::fs::read(&shell.layout_path).unwrap(), persisted);
        assert!(Arc::ptr_eq(
            shell.panels.styles.as_ref().unwrap(),
            &previous
        ));
        assert_eq!(shell.engine.state().unwrap()["project"], project);
    }
    // Two consecutive failure episodes must each report once at 60, with a
    // successful actual reload between them resetting the existing counter.
    for _ in 0..2 {
        let previous = shell.panels.styles.clone().unwrap();
        std::fs::write(&path, b"{").unwrap();
        for _ in 0..61 {
            let error = shell.reload_styles().unwrap_err();
            assert_eq!(style_error(&error).phase, Phase::DecodeOverride);
            assert!(Arc::ptr_eq(
                shell.panels.styles.as_ref().unwrap(),
                &previous
            ));
        }
        std::fs::write(&path, healthy).unwrap();
        shell.reload_styles().unwrap();
        assert!(shell.panels.message.is_none());
    }
    shell.apply_preferences(settings.clone(), true).unwrap();
    assert_eq!(shell.workspace.preferences(), settings);
    assert_eq!(shell.engine.state().unwrap()["project"], project);
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "native_io_json_validation": true, "failed_settings_and_project_unchanged": true,
            "failed_sheet_identity_unchanged": true, "successful_recovery": true,
            "threshold_reset_episodes": 2
        }))
        .unwrap(),
    )
    .unwrap();
    crate::logging::flush();
}
