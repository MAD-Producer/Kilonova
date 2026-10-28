use super::*;
use motion_core::{LibraryAction, ProjectItem};

fn folder(shell: &mut Shell) {
    shell.state = shell
        .engine
        .edit(
            serde_json::to_string(&Command::ProjectLibrary {
                action: LibraryAction::CreateFolder {
                    id: 7,
                    parent: None,
                    name: "素材 Sources".into(),
                },
            })
            .unwrap(),
        )
        .unwrap();
    shell.panels.project.browser.view.root = shell.state["root"].as_str().unwrap().into();
    shell.panels.project.browser.view.folder = Some(7);
}
fn settle(shell: &mut Shell) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some((_, receive)) = &shell.editing {
            match receive.try_recv() {
                Ok(result) => {
                    let state = result?;
                    shell.editing = None;
                    shell.accept_edit_state(state);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => return Err("edit disconnected".into()),
            }
        }
        shell.poll_project_import()?;
        if shell.library_import.is_none()
            && shell.project_image.is_none()
            && shell.editing.is_none()
        {
            return Ok(());
        }
        assert!(Instant::now() < deadline, "project import did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn wave() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/tone-stereo-48000.wav")
}
#[test]
fn project_drop_registers_in_captured_folder_and_explicit_placement_has_separate_history() {
    for locale in [Locale::Zh, Locale::En] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("selected.png");
        image::RgbaImage::from_pixel(8, 6, image::Rgba([30, 80, 140, 255]))
            .save(&path)
            .unwrap();
        let project = root.path().join("project");
        let mut shell = Shell::new(project.clone(), None, Some(locale)).unwrap();
        folder(&mut shell);
        let bounds = motion_ui::Rect::new(0., 0., 400., 300.);
        assert!(!shell.drop_project_media(path.clone(), Ok([410., 40.]), bounds));
        assert!(shell.actions.is_empty());
        assert!(shell.drop_project_media(path.clone(), Ok([40., 40.]), bounds));
        shell.panels.project.browser.view.folder = None;
        let action = shell.actions.pop_front().unwrap();
        shell.action(action, None).unwrap();
        settle(&mut shell).unwrap();
        assert!(shell.state["project"]["layers"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(shell.panels.selected.is_none());
        let asset = shell.state["project"]["assets"][0]["id"].as_u64().unwrap();
        let p: Project = serde_json::from_value(shell.state["project"].clone()).unwrap();
        assert_eq!(
            p.library.folder_of(&ProjectItem::Image { id: asset }),
            Some(7)
        );
        shell.frame = 24.;
        let source = crate::media_assets::Source::read(
            &shell.state,
            motion_core::media_assets::AssetRef {
                kind: motion_core::media_assets::AssetKind::Image,
                id: asset,
            },
        )
        .unwrap();
        shell
            .action(
                panels::Action::MediaAsset {
                    source,
                    intent: crate::media_assets::Intent::Place,
                },
                None,
            )
            .unwrap();
        settle(&mut shell).unwrap();
        assert_eq!(
            shell.state["project"]["layers"][0]["timeline"]["in_frame"],
            24
        );
        assert!(shell.panels.selected.is_some());
        shell.state = shell.engine.history(0).unwrap();
        assert!(shell.state["project"]["layers"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            shell.state["project"]["assets"].as_array().unwrap().len(),
            1
        );
        shell.state = shell.engine.history(0).unwrap();
        assert!(shell.state["project"]["assets"]
            .as_array()
            .unwrap()
            .is_empty());
        shell.state = shell.engine.history(1).unwrap();
        shell.state = shell.engine.save().unwrap();
        drop(shell);
        let reopened = Shell::new(project, None, Some(locale)).unwrap();
        assert_eq!(
            reopened.state["project"]["library"]["placements"][0]["folder"],
            7
        );
        assert_eq!(
            reopened.state["project"]["assets"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn project_media_prepare_and_cancel_queue_without_waiting_for_a_blocked_engine() {
    let root = tempfile::tempdir().unwrap();
    let mut shell = Shell::new(root.path().join("project"), None, Some(Locale::En)).unwrap();
    folder(&mut shell);
    let before = shell.state.clone();
    let (entered, waiting) = std::sync::mpsc::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let blocked = shell
        .engine
        .request(|reply| engine::Command::TestWait {
            entered,
            release: gate,
            reply,
        })
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    shell
        .start_project_import(
            crate::layer_clipboard::KeyframeSource::of(&before),
            Some(7),
            Some(wave()),
        )
        .unwrap();
    let request = shell.library_import.as_ref().unwrap().request.clone();
    shell.poll_project_import().unwrap();
    assert!(shell.library_import.is_some());
    assert!(shell.action(panels::Action::New, None).is_err());
    shell.action(panels::Action::CancelImport, None).unwrap();
    assert!(shell.library_import.is_none());
    assert!(blocked.try_recv().is_err());
    release.send(()).unwrap();
    blocked
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let missing = shell
        .engine
        .media(serde_json::json!({"op":"media_status","request_id":request}).to_string());
    assert!(missing.is_err(), "cancelled task was not released");
    assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
    shell
        .start_project_import(
            crate::layer_clipboard::KeyframeSource::of(&shell.state),
            Some(7),
            Some(wave()),
        )
        .unwrap();
    settle(&mut shell).unwrap();
    assert_eq!(
        shell.state["project"]["audio_assets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(shell.state["project"]["layers"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        shell.state["project"]["library"]["placements"][0]["folder"],
        7
    );
}

#[test]
fn project_import_stale_source_and_failed_media_preserve_document_and_allow_recovery() {
    let root = tempfile::tempdir().unwrap();
    let mut shell = Shell::new(root.path().join("project"), None, Some(Locale::Zh)).unwrap();
    let source = crate::layer_clipboard::KeyframeSource::of(&shell.state);
    folder(&mut shell);
    let before = shell.state["project"].clone();
    shell
        .start_project_import(source, Some(7), Some(root.path().join("missing.png")))
        .unwrap();
    assert!(shell.editing.is_none() && shell.library_import.is_none());
    let path = root.path().join("broken.wav");
    std::fs::write(&path, b"broken audio").unwrap();
    shell
        .start_project_import(
            crate::layer_clipboard::KeyframeSource::of(&shell.state),
            Some(7),
            Some(path),
        )
        .unwrap();
    let cause = settle(&mut shell).unwrap_err();
    assert!(!cause.contains("cancelled"), "{cause}");
    assert!(shell.library_import.is_none());
    assert_eq!(shell.engine.state().unwrap()["project"], before);
    shell
        .start_project_import(
            crate::layer_clipboard::KeyframeSource::of(&shell.state),
            Some(7),
            Some(wave()),
        )
        .unwrap();
    shell.state = shell
        .engine
        .edit(
            serde_json::to_string(&Command::ProjectLibrary {
                action: LibraryAction::RenameFolder {
                    id: 7,
                    name: "changed".into(),
                },
            })
            .unwrap(),
        )
        .unwrap();
    settle(&mut shell).unwrap();
    let project: Project = serde_json::from_value(shell.state["project"].clone()).unwrap();
    assert!(project.audio_assets.is_empty());
    shell
        .start_project_import(
            crate::layer_clipboard::KeyframeSource::of(&shell.state),
            Some(7),
            Some(wave()),
        )
        .unwrap();
    settle(&mut shell).unwrap();
    assert_eq!(
        shell.state["project"]["audio_assets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
