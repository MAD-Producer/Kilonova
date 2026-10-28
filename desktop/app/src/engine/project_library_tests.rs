//! The catalogue command uses the existing worker/error channels. No UI or GPU
//! is needed to exercise real rejected edits and stale-document cancellation.
use super::{Command, Engine};
use crate::layer_clipboard::KeyframeSource;
use motion_core::{LibraryAction, Project};
use serde_json::{json, Value};
use std::path::PathBuf;

#[test]
fn project_view_close_saves_pending_search_and_reports_actual_preference_io_failure_without_user_text(
) {
    const CHILD: &str = "MOTION_PROJECT_VIEW_CLOSE_CHILD";
    const NAME:&str="engine::project_library_tests::project_view_close_saves_pending_search_and_reports_actual_preference_io_failure_without_user_text";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let view = crate::panels::project::ViewPreferences {
            root: "project-root".into(),
            folder: Some(7),
            query: "private-user-query".into(),
            search_all: true,
        };
        let mut success =
            crate::Shell::new(root.join("success"), None, Some(crate::i18n::Locale::Zh)).unwrap();
        success.panels.project.browser.view = view.clone();
        success.finish_close();
        assert_eq!(
            crate::workspace::Workspace::load(&success.layout_path).project_view,
            view
        );
        drop(success);
        let mut failure =
            crate::Shell::new(root.join("failure"), None, Some(crate::i18n::Locale::Zh)).unwrap();
        std::fs::write(root.join("blocked"), b"not a directory").unwrap();
        failure.layout_path = root.join("blocked/layout.json");
        let reason = failure.workspace.save(&failure.layout_path).unwrap_err();
        failure.panels.project.browser.view = view;
        failure.finish_close();
        assert!(failure.closing);
        std::fs::write(root.join("reason.txt"), reason).unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PROJECT_VIEW_CLOSE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let reason = std::fs::read_to_string(root.join("reason.txt")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let marker = "save project browser preferences phase=close";
    assert!(file
        .lines()
        .any(|line| line.contains(" ERROR ") && line.contains(marker) && line.contains(&reason)));
    assert!(stderr.contains(marker) && stderr.contains(&reason));
    assert!(!stdout.contains(marker) && !stdout.contains(&reason));
    assert_eq!(file.lines().filter(|line| line.contains(marker)).count(), 1);
    assert!(!file.contains("private-user-query") && !stderr.contains("private-user-query"));
}

#[test]
fn project_open_rejects_stale_targets_reports_actual_failures_and_preserves_library_and_context_frames(
) {
    const CHILD: &str = "MOTION_PROJECT_OPEN_ERROR_CHILD";
    const NAME:&str="engine::project_library_tests::project_open_rejects_stale_targets_reports_actual_failures_and_preserves_library_and_context_frames";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut p = Project::new(64, 64, 30, 120).unwrap();
        p.compositions.push(motion_core::Composition {
            id: "comp-detail".into(),
            name: "细节".into(),
            width: 64,
            height: 64,
            fps: 30,
            frames: 120,
            work_area: p.work_area.clone(),
            guides: vec![],
            background: p.background,
            camera: p.camera.clone(),
            layers: vec![],
            expressions: vec![],
        });
        motion_core::storage::save(&root.join("project"), &p).unwrap();
        let engine = Engine::start(
            root.join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let original = engine.state().unwrap();
        let organized = edit(
            &engine,
            &original,
            LibraryAction::CreateFolder {
                id: 7,
                parent: None,
                name: "目录".into(),
            },
        )
        .unwrap();
        let open = |source: &Value, composition: &str| {
            engine.call(|reply| Command::ProjectOpen {
                source: KeyframeSource::of(source),
                composition: composition.into(),
                reply,
            })
        };
        let stale = open(&original, "comp-detail").unwrap();
        assert_eq!(stale["editorAction"]["cancelled"], true);
        assert_eq!(stale["project"], organized["project"]);
        let reason = open(&organized, "missing-project-composition").unwrap_err();
        assert!(!reason.is_empty());
        assert_eq!(engine.state().unwrap()["project"], organized["project"]);
        let at_main = engine.seek(33.).unwrap();
        let detail = open(&at_main, "comp-detail").unwrap();
        assert_eq!(detail["composition"], "comp-detail");
        assert_eq!(detail["frame"], 0.);
        assert_eq!(
            detail["project"]["library"],
            organized["project"]["library"]
        );
        let at_detail = engine.seek(12.).unwrap();
        let main = open(&at_detail, "comp-main").unwrap();
        assert_eq!(main["frame"], 33.);
        let returned = open(&main, "comp-detail").unwrap();
        assert_eq!(returned["frame"], 12.);
        assert_eq!(returned["canUndo"], organized["canUndo"]);
        let mut shell = crate::Shell::new(
            root.join("another-project"),
            None,
            Some(crate::i18n::Locale::Zh),
        )
        .unwrap();
        shell.frame = 99.;
        shell.panels.select_only(Some(88));
        shell.accept_edit_state(returned.clone());
        assert_eq!(shell.frame, 12.);
        assert_eq!(shell.panels.selected, None);
        shell.accept_edit_state(stale);
        assert_eq!(
            shell.frame, 12.,
            "cancelled navigation must not move the shell playhead"
        );
        engine.save().unwrap();
        drop(engine);
        let reopened = Engine::start(
            root.join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        assert_eq!(
            reopened.state().unwrap()["project"]["library"],
            organized["project"]["library"]
        );
        std::fs::write(root.join("reason.txt"), reason).unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PROJECT_OPEN_ERROR_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let reason = std::fs::read_to_string(root.join("reason.txt")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        file.lines().any(|line| line.contains(" INFO ")
            && line.contains("start #")
            && line.ends_with(" project_open")),
        "{file}"
    );
    assert!(
        file.lines().any(|line| line.contains(" ERROR ")
            && line.contains("project_open")
            && line.contains(&reason)),
        "{file}"
    );
    assert!(stderr.contains(&reason));
    assert!(!stdout.contains(&reason));
    assert!(!stdout.contains("start #") && !stderr.contains("start #"));
    assert_eq!(
        file.lines()
            .filter(|line| line.contains(" ERROR ") && line.contains("project_open"))
            .count(),
        1
    );
    assert!(
        !file.contains("context_changed"),
        "normal stale navigation is not a fault"
    );
}

fn edit(engine: &Engine, state: &Value, action: LibraryAction) -> Result<Value, String> {
    engine.call(|reply| Command::EditorAction {
        source: KeyframeSource::of(state),
        commands: vec![motion_core::Command::ProjectLibrary { action }],
        reply,
    })
}

#[test]
fn project_library_failures_reach_default_channels_and_stale_actions_preserve_the_document() {
    const NAME: &str = "engine::project_library_tests::project_library_failures_reach_default_channels_and_stale_actions_preserve_the_document";
    const CHILD: &str = "MOTION_PROJECT_LIBRARY_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let directory = root.join("project");
        motion_core::storage::save(&directory, &Project::new(64, 64, 30, 120).unwrap()).unwrap();
        let engine =
            Engine::start(directory.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        let original = engine.state().unwrap();
        let organized = edit(
            &engine,
            &original,
            LibraryAction::CreateFolder {
                id: 7,
                parent: None,
                name: "素材".into(),
            },
        )
        .unwrap();
        for (action, reason) in [
            (
                LibraryAction::RenameFolder {
                    id: 99,
                    name: "missing".into(),
                },
                "project folder no longer exists",
            ),
            (
                LibraryAction::MoveFolder {
                    id: 7,
                    parent: Some(7),
                },
                "project folders cannot contain a cycle",
            ),
            (
                LibraryAction::MoveItems {
                    items: vec![motion_core::ProjectItem::Image { id: 99 }],
                    folder: Some(7),
                },
                "project item no longer exists",
            ),
        ] {
            let error = edit(&engine, &organized, action).unwrap_err();
            assert!(error.contains("project library operation="), "{error}");
            assert!(error.contains(reason), "{error}");
            let after = engine.state().unwrap();
            assert_eq!(after["project"], organized["project"]);
            assert_eq!(after["revision"], organized["revision"]);
        }
        let renamed = edit(
            &engine,
            &organized,
            LibraryAction::RenameFolder {
                id: 7,
                name: "Footage".into(),
            },
        )
        .unwrap();
        let stale = edit(&engine, &organized, LibraryAction::RemoveFolder { id: 7 }).unwrap();
        assert_eq!(
            stale["editorAction"],
            json!({"cancelled":true,"reason":"context_changed"})
        );
        assert_eq!(stale["project"], renamed["project"]);
        assert_eq!(stale["revision"], renamed["revision"]);
        assert_eq!(engine.history(0).unwrap()["project"], organized["project"]);
        engine.save().unwrap();
        assert_eq!(
            serde_json::to_value(motion_core::storage::load(&directory).unwrap()).unwrap(),
            organized["project"]
        );

        // Same revision, same composition and reused folder IDs in another document
        // must still cancel by the captured root/document generation.
        let other = root.join("other");
        motion_core::storage::save(&other, &Project::new(64, 64, 30, 120).unwrap()).unwrap();
        let ticket = engine.call(Command::ProjectTicket).unwrap();
        let changed = engine
            .call(|reply| Command::ChangeProject {
                target: crate::project_lifecycle::Target::Open(other),
                ticket,
                save: false,
                reply,
            })
            .unwrap();
        assert_eq!(changed["revision"], original["revision"]);
        let cancelled = edit(
            &engine,
            &original,
            LibraryAction::CreateFolder {
                id: 7,
                parent: None,
                name: "must not reach another document".into(),
            },
        )
        .unwrap();
        assert_eq!(cancelled["editorAction"]["cancelled"], true);
        assert_eq!(cancelled["project"], changed["project"]);
        let recovered = edit(
            &engine,
            &changed,
            LibraryAction::CreateFolder {
                id: 7,
                parent: None,
                name: "Recovered".into(),
            },
        )
        .unwrap();
        assert_eq!(
            recovered["project"]["library"]["folders"][0]["name"],
            "Recovered"
        );
        engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PROJECT_LIBRARY_ERROR_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for reason in [
        "project folder no longer exists",
        "project folders cannot contain a cycle",
        "project item no longer exists",
    ] {
        assert!(
            file.lines()
                .any(|line| line.contains(" ERROR ") && line.contains(reason)),
            "{file}"
        );
        assert!(stderr.contains(reason), "{stderr}");
        assert!(!stdout.contains(reason), "{stdout}");
    }
    assert_eq!(
        file.lines()
            .filter(|line| line.contains(" ERROR ") && line.contains("editor_action"))
            .count(),
        3
    );
    assert!(file.lines().any(|line| line.contains(" INFO ")
        && line.contains("start #")
        && line.ends_with(" editor_action")));
    assert!(!stderr.contains("start #") && !stdout.contains("start #"));
    assert!(!stderr.contains("context_changed") && !stdout.contains("context_changed"));
}
