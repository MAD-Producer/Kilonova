//! The catalogue command uses the existing worker/error channels. No UI or GPU
//! is needed to exercise real rejected edits and stale-document cancellation.
use super::{Command, Engine};
use crate::layer_clipboard::KeyframeSource;
use motion_core::{LibraryAction, Project};
use serde_json::{json, Value};
use std::path::PathBuf;

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
