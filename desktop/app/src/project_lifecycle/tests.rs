use super::*;
use crate::{i18n::Locale, panels};
use motion_core::{Command, Project};
use std::{
    fs,
    time::{Duration, Instant},
};

fn shell(folder: &std::path::Path) -> Shell {
    Shell::new(folder.join("current"), None, Some(Locale::En)).unwrap()
}
fn drain(shell: &mut Shell) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while shell.project_pending.is_some() {
        assert!(
            Instant::now() < deadline,
            "project transition did not complete"
        );
        shell.poll_project_change();
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn destination(folder: &std::path::Path) -> PathBuf {
    let root = folder.join("other");
    fs::create_dir_all(&root).unwrap();
    let mut project = Project::new(640, 360, 24, 100).unwrap();
    project.name = "Other".into();
    motion_core::storage::save(&root, &project).unwrap();
    root
}
fn saved_then_edit(shell: &mut Shell) -> (PathBuf, Vec<u8>, Value) {
    shell.action(Action::Save, None).unwrap();
    let root = PathBuf::from(shell.state["root"].as_str().unwrap());
    let bytes = fs::read(root.join("project.json")).unwrap();
    shell.add_solid().unwrap();
    let state = shell.engine.state().unwrap();
    (root, bytes, state)
}
fn new_target() -> Target {
    Target::New(serde_json::to_string(&Project::new(800, 600, 30, 100).unwrap()).unwrap())
}

#[test]
fn dirty_tracks_saved_content_undo_redo_seek_and_external_shared_save() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = shell(folder.path());
    assert!(!shell.engine.project_ticket().unwrap().dirty);
    saved_then_edit(&mut shell);
    assert!(shell.engine.project_ticket().unwrap().dirty);
    shell.engine.seek(20.).unwrap();
    assert!(shell.engine.project_ticket().unwrap().dirty);
    shell.engine.history(0).unwrap();
    assert!(!shell.engine.project_ticket().unwrap().dirty);
    shell.engine.history(1).unwrap();
    assert!(shell.engine.project_ticket().unwrap().dirty);
    // MCP and GUI use the same Save command, independent of Shell.state.
    shell.engine.save().unwrap();
    assert!(!shell.engine.project_ticket().unwrap().dirty);
    shell.engine.seek(40.).unwrap();
    assert!(!shell.engine.project_ticket().unwrap().dirty);
}

#[test]
fn cancel_new_keeps_disk_memory_selection_and_history() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = shell(folder.path());
    let (root, bytes, before) = saved_then_edit(&mut shell);
    let selected = shell.panels.selected;
    shell.action(Action::New, None).unwrap();
    assert!(shell.project_prompt.is_some());
    shell.choose_project_change(CloseChoice::Cancel).unwrap();
    assert!(!shell.project_modal());
    let after = shell.engine.state().unwrap();
    assert_eq!(before["project"], after["project"]);
    assert_eq!(before["revision"], after["revision"]);
    assert_eq!(selected, shell.panels.selected);
    assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
    assert!(after["canUndo"].as_bool().unwrap());
}

#[test]
fn new_and_open_save_or_discard_follow_explicit_choice() {
    for open in [false, true] {
        for save in [false, true] {
            let folder = tempfile::tempdir().unwrap();
            let mut shell = shell(folder.path());
            let (root, bytes, before) = saved_then_edit(&mut shell);
            let other = destination(folder.path());
            let target = if open {
                Target::Open(other.clone())
            } else {
                new_target()
            };
            shell.request_project_change(target).unwrap();
            assert!(shell.project_prompt.is_some());
            shell
                .choose_project_change(if save {
                    CloseChoice::Save
                } else {
                    CloseChoice::Discard
                })
                .unwrap();
            drain(&mut shell);
            assert!(!shell.project_modal());
            assert_ne!(shell.state["root"], before["root"]);
            assert!(!shell.engine.project_ticket().unwrap().dirty);
            let persisted = motion_core::storage::load_document(&root).unwrap();
            assert_eq!(persisted.layers.len(), usize::from(save));
            if !save {
                assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
            }
            if open {
                assert_eq!(shell.state["project"]["name"], "Other");
            }
        }
    }
}

#[test]
fn failed_save_blocks_new_and_leaves_retry_discard_cancel_available() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = shell(folder.path());
    let (root, bytes, _) = saved_then_edit(&mut shell);
    let asset = motion_core::AudioAsset {
        id: 99,
        path: "assets/missing.wav".into(),
        mime: "audio/wav".into(),
        bytes: 64,
        track: 0,
        sample_rate: 48000,
        channels: 1,
        sample_frames: 48000,
        duration_us: 1_000_000,
    };
    let source = root.join(&asset.path);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, [0u8; 64]).unwrap();
    shell
        .engine
        .edit(serde_json::to_string(&Command::RegisterAudioAsset { asset }).unwrap())
        .unwrap();
    // Registration validates the source; simulate it disappearing afterward.
    fs::remove_file(source).unwrap();
    let before = shell.engine.state().unwrap();
    shell.request_project_change(new_target()).unwrap();
    shell.choose_project_change(CloseChoice::Save).unwrap();
    drain(&mut shell);
    assert!(shell.project_prompt.as_ref().unwrap().error.is_some());
    let after = shell.engine.state().unwrap();
    assert_eq!(before["project"], after["project"]);
    assert_eq!(before["revision"], after["revision"]);
    assert_eq!(before["root"], after["root"]);
    assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
    shell.choose_project_change(CloseChoice::Cancel).unwrap();
    assert!(!shell.project_modal());
    shell.request_project_change(new_target()).unwrap();
    shell.choose_project_change(CloseChoice::Discard).unwrap();
    drain(&mut shell);
    assert_ne!(shell.state["root"], before["root"]);
    assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
}

#[test]
fn save_io_error_keeps_current_and_a_retry_saves_before_switching() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = shell(folder.path());
    let (root, bytes, before) = saved_then_edit(&mut shell);
    fs::create_dir(root.join("project.json.tmp")).unwrap();
    shell.request_project_change(new_target()).unwrap();
    shell.choose_project_change(CloseChoice::Save).unwrap();
    drain(&mut shell);
    assert!(shell.project_prompt.as_ref().unwrap().error.is_some());
    assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
    assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
    fs::remove_dir(root.join("project.json.tmp")).unwrap();
    shell.choose_project_change(CloseChoice::Save).unwrap();
    drain(&mut shell);
    assert!(!shell.project_modal());
    assert_eq!(
        motion_core::storage::load_document(&root)
            .unwrap()
            .layers
            .len(),
        1
    );
}

#[test]
fn concurrent_edit_invalidates_confirmation_and_requires_another_choice() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = shell(folder.path());
    let (root, bytes, _) = saved_then_edit(&mut shell);
    shell.request_project_change(new_target()).unwrap();
    let captured = shell.project_prompt.as_ref().unwrap().ticket.revision;
    shell.add_solid().unwrap(); // Simulate MCP editing while the modal is open.
    let before = shell.engine.state().unwrap();
    shell.choose_project_change(CloseChoice::Discard).unwrap();
    drain(&mut shell);
    let prompt = shell.project_prompt.as_ref().unwrap();
    assert!(prompt.error.is_some());
    assert_ne!(prompt.ticket.revision, captured);
    assert_eq!(before["project"], shell.engine.state().unwrap()["project"]);
    assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
    shell.choose_project_change(CloseChoice::Cancel).unwrap();
}

#[test]
fn invalid_open_target_preserves_current_without_saving_it() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = shell(folder.path());
    let (root, bytes, before) = saved_then_edit(&mut shell);
    let broken = folder.path().join("broken");
    fs::create_dir(&broken).unwrap();
    fs::write(broken.join("project.json"), b"invalid JSON").unwrap();
    shell.action(Action::OpenPath(broken), None).unwrap();
    shell.choose_project_change(CloseChoice::Save).unwrap();
    drain(&mut shell);
    assert!(shell.project_prompt.as_ref().unwrap().error.is_some());
    let after = shell.engine.state().unwrap();
    assert_eq!(before["root"], after["root"]);
    assert_eq!(before["project"], after["project"]);
    assert_eq!(before["revision"], after["revision"]);
    assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
}

#[test]
fn imported_package_honors_save_and_discard() {
    for save in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = shell(folder.path());
        let (root, bytes, _) = saved_then_edit(&mut shell);
        let other = destination(folder.path());
        let project = motion_core::storage::load(&other).unwrap();
        let package = folder.path().join("other.msproj");
        motion_core::storage::export_package(&other, &project, &package).unwrap();
        shell.action(Action::OpenPath(package), None).unwrap();
        shell
            .choose_project_change(if save {
                CloseChoice::Save
            } else {
                CloseChoice::Discard
            })
            .unwrap();
        drain(&mut shell);
        assert!(!shell.project_modal());
        assert_eq!(shell.state["project"]["name"], "Other");
        assert_eq!(
            motion_core::storage::load_document(&root)
                .unwrap()
                .layers
                .len(),
            usize::from(save)
        );
        if !save {
            assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
        }
    }
}

#[test]
fn project_close_cancel_save_discard_and_failed_save_preserve_expected_content() {
    for choice in [CloseChoice::Cancel, CloseChoice::Save, CloseChoice::Discard] {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = shell(folder.path());
        let (root, bytes, before) = saved_then_edit(&mut shell);
        shell.begin_close();
        assert!(!shell.closing);
        assert!(shell.project_prompt.is_some());
        shell.choose_project_change(choice).unwrap();
        drain(&mut shell);
        assert_eq!(shell.closing, choice != CloseChoice::Cancel);
        assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
        if choice == CloseChoice::Save {
            assert_eq!(
                motion_core::storage::load_document(&root)
                    .unwrap()
                    .layers
                    .len(),
                1
            );
        } else {
            assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
        }
    }
    let folder = tempfile::tempdir().unwrap();
    let mut shell = shell(folder.path());
    let (root, bytes, _) = saved_then_edit(&mut shell);
    fs::create_dir(root.join("project.json.tmp")).unwrap();
    shell.begin_close();
    shell.choose_project_change(CloseChoice::Save).unwrap();
    drain(&mut shell);
    assert!(!shell.closing);
    assert!(shell.project_prompt.as_ref().unwrap().error.is_some());
    assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
}

#[test]
fn node_close_then_project_cancel_keeps_both_documents() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = shell(folder.path());
    let (_, _, before) = saved_then_edit(&mut shell);
    shell
        .panels
        .node_editor
        .add_node(motion_graph::NodeOp::Float { value: 2. }, [0., 0.])
        .unwrap();
    let graph = shell.panels.node_editor.document().clone();
    shell.request_close();
    assert!(shell.node_close_prompt.is_some());
    let snapshot = shell.node_close_prompt.take().unwrap();
    assert!(shell.node_close_current(&snapshot));
    shell.begin_close(); // Existing node Discard choice, project still asks separately.
    assert!(shell.project_prompt.is_some());
    shell.choose_project_change(CloseChoice::Cancel).unwrap();
    assert!(!shell.closing);
    assert_eq!(&graph, shell.panels.node_editor.document());
    assert!(shell.panels.node_editor.dirty());
    assert_eq!(before["project"], shell.engine.state().unwrap()["project"]);
}

#[test]
fn clean_open_needs_no_prompt_and_same_root_does_not_write_on_discard() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = shell(folder.path());
    shell.action(Action::Save, None).unwrap();
    let other = destination(folder.path());
    shell.action(Action::OpenPath(other), None).unwrap();
    assert!(shell.project_prompt.is_none());
    drain(&mut shell);
    assert_eq!(shell.state["project"]["name"], "Other");
    let (root, bytes, before) = saved_then_edit(&mut shell);
    shell.action(Action::OpenPath(root.clone()), None).unwrap();
    shell.choose_project_change(CloseChoice::Discard).unwrap();
    drain(&mut shell);
    assert_eq!(
        shell.engine.state().unwrap()["project"]["layers"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(before["root"], shell.engine.state().unwrap()["root"]);
    assert_eq!(bytes, fs::read(root.join("project.json")).unwrap());
}

#[test]
fn modal_escape_routes_cancel_and_blocks_underlying_edit_actions() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = shell(folder.path());
    let (_, _, before) = saved_then_edit(&mut shell);
    shell.request_project_change(new_target()).unwrap();
    shell.action(panels::Action::AddSolid, None).unwrap();
    shell.input.keys.push((
        motion_ui::input::Key::Escape,
        motion_ui::input::Modifiers::default(),
    ));
    let (_, actions) = shell.draw_commands([1280., 800.]);
    assert!(actions.is_empty());
    assert!(matches!(
        shell.actions.pop_front(),
        Some(Action::ProjectChoice(CloseChoice::Cancel))
    ));
    shell.choose_project_change(CloseChoice::Cancel).unwrap();
    assert_eq!(before["project"], shell.engine.state().unwrap()["project"]);
}

#[test]
fn missing_recent_projects_show_an_acknowledgement_on_home_and_preserve_the_current_project() {
    use motion_ui::input::{Event, Key, Modifiers};
    for moved in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = shell(folder.path());
        shell.home = Some(crate::home::Home::default());
        shell.action(Action::Save, None).unwrap();
        let before = shell.engine.state().unwrap();
        let current = PathBuf::from(before["root"].as_str().unwrap());
        let bytes = fs::read(current.join("project.json")).unwrap();
        let missing = destination(folder.path());
        if moved {
            fs::rename(&missing, folder.path().join("moved")).unwrap();
        } else {
            fs::remove_file(missing.join("project.json")).unwrap();
            fs::remove_dir(&missing).unwrap();
        }
        shell
            .action(Action::OpenPath(missing.clone()), None)
            .unwrap();
        drain(&mut shell);
        let prompt = shell
            .project_prompt
            .as_ref()
            .expect("open failure must be visible");
        assert!(matches!(&prompt.target, Target::Open(path) if path == &missing));
        assert!(!prompt.ticket.dirty);
        let error = prompt.error.as_ref().unwrap();
        assert!(
            error.contains("os error") && error.contains(&missing.display().to_string()),
            "{error}"
        );
        assert!(shell.project_modal() && shell.home.is_some());
        shell.action(Action::AddSolid, None).unwrap();
        shell.translate(Event::KeyPressed {
            key: Key::N,
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
        });
        assert!(shell.draw_commands([900., 600.]).1.is_empty());
        assert!(
            shell.actions.is_empty(),
            "background shortcut leaked through the error notice"
        );
        for key in ["root", "project", "revision", "canUndo", "canRedo"] {
            assert_eq!(before[key], shell.engine.state().unwrap()[key]);
        }
        assert_eq!(bytes, fs::read(current.join("project.json")).unwrap());
        shell.translate(Event::KeyPressed {
            key: if moved { Key::Escape } else { Key::Enter },
            modifiers: Modifiers::default(),
        });
        assert!(shell.draw_commands([900., 600.]).1.is_empty());
        assert!(matches!(
            shell.actions.pop_front(),
            Some(Action::ProjectChoice(CloseChoice::Cancel))
        ));
        shell.choose_project_change(CloseChoice::Cancel).unwrap();
        assert!(!shell.project_modal() && shell.home.is_some());
        let restored = destination(folder.path());
        shell.action(Action::OpenPath(restored), None).unwrap();
        drain(&mut shell);
        assert_eq!(shell.state["project"]["name"], "Other");
        assert!(shell.home.is_none());
    }
}

#[test]
fn invalid_clean_project_and_missing_package_use_error_notices_without_implicit_retry() {
    for package in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = shell(folder.path());
        shell.action(Action::Save, None).unwrap();
        let before = shell.engine.state().unwrap();
        let path = if package {
            folder.path().join("missing.msproj")
        } else {
            let root = destination(folder.path());
            fs::write(root.join("project.json"), b"invalid JSON").unwrap();
            root
        };
        shell.action(Action::OpenPath(path), None).unwrap();
        drain(&mut shell);
        assert!(shell.project_prompt.as_ref().unwrap().error.is_some());
        assert!(shell.project_modal());
        // A queued choice from an earlier prompt must only acknowledge this
        // notice, never resubmit a failed open or save the clean current project.
        shell.choose_project_change(CloseChoice::Save).unwrap();
        assert!(!shell.project_modal() && shell.project_pending.is_none());
        assert_eq!(before["project"], shell.engine.state().unwrap()["project"]);
        assert_eq!(before["root"], shell.state["root"]);
    }
}

#[test]
fn missing_project_notice_preserves_the_default_file_and_stderr_error_channels() {
    const CHILD: &str = "MOTION_MISSING_PROJECT_NOTICE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut shell = shell(&root);
        shell.home = Some(crate::home::Home::default());
        shell.action(Action::Save, None).unwrap();
        let missing = root.join("消失的工程");
        let expected = format!(
            "open project {}: {}",
            missing.display(),
            missing.canonicalize().unwrap_err()
        );
        shell.action(Action::OpenPath(missing), None).unwrap();
        drain(&mut shell);
        assert!(shell
            .project_prompt
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains(&expected));
        assert!(shell.project_modal());
        for _ in 0..3 {
            assert!(shell.draw_commands([900., 600.]).1.is_empty());
            assert!(shell.actions.is_empty());
        }
        fs::write(root.join("expected.txt"), &expected).unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "project_lifecycle::tests::missing_project_notice_preserves_the_default_file_and_stderr_error_channels"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = fs::read_to_string(root.path().join("expected.txt")).unwrap();
    let file = fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("ERROR") && line.contains(" change_project "))
        .collect();
    assert_eq!(errors.len(), 1, "{file}");
    assert!(errors[0].contains(&expected), "{}", errors[0]);
    assert!(stderr.contains(&expected));
    assert!(!stdout.contains(&expected) && !stdout.contains("logging initialized"));
    let request = errors[0]
        .split("error #")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    assert!(
        file.find(&format!("start #{request} change_project"))
            .unwrap()
            < file.find(errors[0]).unwrap()
    );
}
