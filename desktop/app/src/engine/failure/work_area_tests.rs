//! Invalid document ranges travel through the actual serial worker and logger.
use super::*;

#[test]
fn invalid_work_area_reaches_reply_file_and_stderr_without_mutation_at_default_level() {
    const CHILD: &str = "MOTION_WORK_AREA_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project_root = root.join("project");
        motion_core::storage::save(
            &project_root,
            &motion_core::Project::new(64, 64, 30, 120).unwrap(),
        )
        .unwrap();
        let engine = Engine::start(
            project_root.clone(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine.state().unwrap();
        assert_eq!(before["capabilities"]["work_area"]["interval"], "half_open");
        let saved = std::fs::read(project_root.join("project.json")).unwrap();
        for (in_frame, out_frame) in [(30, 30), (50, 40), (0, 121)] {
            let command = serde_json::json!([
                {"op":"set_work_area","in_frame":10,"out_frame":100},
                {"op":"set_work_area","in_frame":in_frame,"out_frame":out_frame}
            ])
            .to_string();
            let error = engine
                .edit_async(command)
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap_err();
            assert!(error.contains("invalid work area"), "{error}");
            assert!(
                error.contains(&format!("in_frame={in_frame} out_frame={out_frame}")),
                "{error}"
            );
            let after = engine.state().unwrap();
            assert_eq!(after["project"], before["project"]);
            assert_eq!(after["revision"], before["revision"]);
            assert_eq!(
                std::fs::read(project_root.join("project.json")).unwrap(),
                saved
            );
        }
        let valid = engine
            .edit(r#"{"op":"set_work_area","in_frame":15,"out_frame":45}"#.into())
            .unwrap();
        assert_eq!(
            valid["project"]["work_area"],
            serde_json::json!({"in_frame":15,"out_frame":45})
        );
        let error = engine.edit_async_at_revision(
            r#"{"op":"set_work_area","in_frame":20,"out_frame":80}"#.into(),before["revision"].as_u64().unwrap())
            .unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err();
        assert!(error.contains("project revision changed"));
        assert_eq!(engine.state().unwrap()["project"],valid["project"]);
        drop(engine);
        let project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        let error = crate::audio_output::AudioOutput::start_in_range(
            project,
            &project_root,
            f64::NAN,
            motion_core::WorkArea::full(120),
            true,
        )
        .err()
        .unwrap();
        assert_eq!(error, "audio start outside composition");
        let mut shell = crate::Shell::new(project_root, None, Some(crate::i18n::Locale::En)).unwrap();
        let blocked = root.join("blocked-preferences");
        std::fs::create_dir_all(&blocked).unwrap();
        shell.layout_path = blocked;
        assert!(shell.action(crate::panels::Action::PreviewPlayback(Default::default()),None).is_err());
        assert!(shell.action(crate::panels::Action::MovieExportScope(motion_core::FrameScope::WorkArea),None).is_err());
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_WORK_AREA_ERROR_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("observability"));
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::failure::work_area_tests::invalid_work_area_reaches_reply_file_and_stderr_without_mutation_at_default_level"])
        .env(CHILD, &root).env_remove("MOTION_LOG").output().unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    assert!(file.contains("project revision changed") && stderr.contains("project revision changed") && !stdout.contains("project revision changed"));
    for operation in ["preview_playback", "movie_export_scope"] {
        let start = format!("preview preferences start operation={operation}");
        let error = format!("preview preferences failed operation={operation}");
        assert!(file.find(&start).unwrap() < file.find(&error).unwrap());
        assert!(stderr.contains(&error) && !stdout.contains(&error));
        assert!(file.lines().any(|line| line.contains(&error) && line.contains("phase=save") && line.contains("elapsed_ms=") && line.contains("cause=")));
    }
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("invalid work area"))
        .collect();
    assert_eq!(errors.len(), 3, "{file}");
    for error in errors {
        assert!(
            error.contains("ERROR")
                && error.contains("error #")
                && error.contains(" edit ")
                && error.contains("ms:"),
            "{error}"
        );
        let request = error
            .split("error #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let start = format!("start #{request} edit");
        assert!(file.find(&start).unwrap() < file.find(error).unwrap());
        assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    }
    assert_eq!(stderr.matches("invalid work area").count(), 3);
    assert!(!stderr.contains("start #") && !stderr.contains("logging initialized"));
    let audio_error = file
        .lines()
        .find(|line| line.contains("audio preview start failed"))
        .unwrap();
    assert!(
        audio_error.contains("ERROR")
            && audio_error.contains("phase=start")
            && audio_error.contains("elapsed_ms=")
            && audio_error.contains("fps=30 in_frame=0 out_frame=120 looping=true")
            && audio_error.contains("audio start outside composition")
    );
    assert!(
        file.find("audio preview start phase=start").unwrap() < file.find(audio_error).unwrap()
    );
    assert_eq!(stderr.matches("audio start outside composition").count(), 1);
    assert!(
        !stdout.contains("audio preview") && !stdout.contains("audio start outside composition")
    );
    for marker in ["invalid work area", "start #", "logging initialized"] {
        assert!(!stdout.contains(marker), "stdout logging leak: {stdout}");
    }
}
