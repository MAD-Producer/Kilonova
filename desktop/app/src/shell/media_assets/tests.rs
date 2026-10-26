use super::*;
use motion_core::media_assets::AssetRef;

fn wait(shell: &mut Shell) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while shell.importing_image.is_some() || shell.importing_media.is_some() {
        shell.poll_import()?;
        assert!(Instant::now() < deadline, "asset operation did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures")
        .join(name)
}

#[test]
fn asset_relink_ui_queues_prepare_status_commit_and_cancel_behind_a_blocked_worker() {
    use std::sync::mpsc::channel;
    for phase in [
        RelinkPhase::Prepare,
        RelinkPhase::Status,
        RelinkPhase::Commit,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(temp.path().join("project"), None, Some(Locale::En)).unwrap();
        let path = temp.path().join("source.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([25, 50, 75, 255]))
            .save(&path)
            .unwrap();
        shell.import_file(path.clone()).unwrap();
        wait(&mut shell).unwrap();
        let before = shell.state.clone();
        let candidates = vec![
            serde_json::json!({"target":{"kind":"image","id":before["project"]["assets"][0]["id"]},"path":path}),
        ];
        let request = format!("blocked-relink-{phase:?}");
        if phase != RelinkPhase::Prepare {
            shell.engine.media(serde_json::json!({"op":"start_asset_relink","request_id":request,"candidates":candidates,"revision":before["revision"],"document_generation":before["document_generation"]}).to_string()).unwrap();
            loop {
                let value = shell
                    .engine
                    .media(
                        serde_json::json!({"op":"media_status","request_id":request}).to_string(),
                    )
                    .unwrap();
                if value["state"] == "ready" {
                    break;
                }
                assert_eq!(value["state"], "running", "{value}");
            }
        }
        let (entered, waiting) = channel();
        let (release, gate) = channel();
        let blocked = shell
            .engine
            .request(|reply| engine::Command::TestWait {
                entered,
                release: gate,
                reply,
            })
            .unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        if phase == RelinkPhase::Prepare {
            shell.start_asset_relink(candidates, &before).unwrap();
        } else {
            shell.importing_media =
                Some((request.clone(), Instant::now() - Duration::from_secs(1)));
            shell.relinking_assets = true;
            let op = if phase == RelinkPhase::Status {
                "media_status"
            } else {
                "finish_asset_relink"
            };
            shell
                .queue_asset_relink(phase, serde_json::json!({"op":op,"request_id":request}))
                .unwrap();
        }
        shell.poll_import().unwrap();
        shell.action(panels::Action::CancelImport, None).unwrap();
        shell.poll_import().unwrap();
        assert!(shell.relinking_assets);
        assert_eq!(shell.relink_reply.as_ref().unwrap().0, RelinkPhase::Cancel);
        // A synchronous UI wait would exhaust TestWait's controlled gate before
        // reaching this send. No wall-clock speed threshold or mocked success.
        release.send(()).unwrap();
        assert_eq!(
            blocked
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap()["ready"],
            true
        );
        wait(&mut shell).unwrap();
        assert!(shell.relink_reply.is_none());
        if phase == RelinkPhase::Commit {
            assert_eq!(
                shell.state["revision"].as_u64().unwrap(),
                before["revision"].as_u64().unwrap() + 1
            );
            assert!(
                shell.panels.message.as_ref().unwrap().contains("relinked"),
                "late cancellation must report the actual commit"
            );
        } else {
            assert_eq!(shell.state["project"], before["project"]);
            assert!(shell.panels.message.as_ref().unwrap().contains("cancelled"));
        }
        assert_eq!(
            shell.engine.state().unwrap()["project"],
            shell.state["project"]
        );
    }
}

#[test]
fn asset_relink_shell_preserves_selection_and_releases_success_failure_and_cancel_slots() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let mut shell = Shell::new(root.clone(), None, Some(Locale::Zh)).unwrap();
    let mut paths = Vec::new();
    for index in 0..2 {
        let path = temp.path().join(format!("image-{index}.png"));
        image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 20, index as u8, 255]))
            .save(&path)
            .unwrap();
        shell.import_file(path.clone()).unwrap();
        wait(&mut shell).unwrap();
        paths.push(path);
    }
    shell.panels.select_only(None);
    let before = shell.state.clone();
    for a in before["project"]["assets"].as_array().unwrap() {
        std::fs::remove_file(root.join(a["path"].as_str().unwrap())).unwrap();
    }
    let inventory = shell
        .engine
        .media(serde_json::json!({"op":"missing_media_assets"}).to_string())
        .unwrap();
    let inputs: Vec<_> = before["project"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&paths)
        .map(|(a, path)| serde_json::json!({"target":{"kind":"image","id":a["id"]},"path":path}))
        .collect();
    shell
        .start_asset_relink(inputs.clone(), &inventory)
        .unwrap();
    wait(&mut shell).unwrap();
    assert!(!shell.relinking_assets && shell.importing_media.is_none());
    assert_eq!(shell.panels.selected, None);
    assert_eq!(
        shell.state["project"]["layers"],
        before["project"]["layers"]
    );
    assert!(shell.panels.message.as_ref().unwrap().contains("已重链接"));
    let after = shell.state.clone();
    let mut bad = inputs.clone();
    bad[1]["path"] = serde_json::json!(temp.path().join("missing-candidate.png"));
    shell.start_asset_relink(bad, &after).unwrap();
    assert!(wait(&mut shell).is_err());
    assert!(!shell.relinking_assets && shell.importing_media.is_none());
    assert_eq!(shell.state["project"], after["project"]);
    shell.start_asset_relink(inputs, &after).unwrap();
    shell.action(panels::Action::CancelImport, None).unwrap();
    wait(&mut shell).unwrap();
    assert!(!shell.relinking_assets && shell.importing_media.is_none());
    assert_eq!(shell.state["project"], after["project"]);
    assert!(!std::fs::read_dir(&root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".asset-relink-")));
    shell.engine.save().unwrap();
}

#[test]
fn asset_replacement_image_reply_keeps_selection_and_failed_or_stale_operations_release_slots() {
    let root = tempfile::tempdir().unwrap();
    let mut shell = Shell::new(root.path().join("project"), None, Some(Locale::En)).unwrap();
    let path = root.path().join("source.png");
    image::RgbaImage::from_pixel(32, 32, image::Rgba([20, 40, 60, 255]))
        .save(&path)
        .unwrap();
    shell.import_file(path.clone()).unwrap();
    wait(&mut shell).unwrap();
    shell.panels.select_only(None);
    let id = shell.state["project"]["assets"][0]["id"].as_u64().unwrap();
    let source = Source::read(
        &shell.state,
        AssetRef {
            kind: AssetKind::Image,
            id,
        },
    )
    .unwrap();
    let before = shell.state.clone();
    shell
        .replace_asset_file(source.clone(), root.path().join("missing.png"))
        .unwrap();
    assert!(wait(&mut shell).unwrap_err().contains("missing.png"));
    assert!(shell.replacing_asset.is_none());
    assert!(shell.importing_image.is_none());
    assert_eq!(shell.state["project"], before["project"]);
    assert_eq!(shell.panels.selected, None);
    shell.replace_asset_file(source.clone(), path).unwrap();
    wait(&mut shell).unwrap();
    assert_eq!(shell.panels.selected, None);
    assert!(shell.replacing_asset.is_none());
    assert!(shell
        .panels
        .message
        .as_ref()
        .unwrap()
        .contains("Asset replaced"));
    let after = shell.state.clone();
    shell
        .replace_asset_file(source, root.path().join("does-not-exist.png"))
        .unwrap();
    assert!(shell.importing_image.is_none());
    assert_eq!(shell.state["project"], after["project"]);
    assert!(shell.panels.message.as_ref().unwrap().contains("cancelled"));
    shell.engine.save().unwrap();
}

#[test]
fn asset_replacement_audio_polling_keeps_identity_and_releases_real_cancellation_and_failure() {
    let root = tempfile::tempdir().unwrap();
    let mut shell = Shell::new(root.path().join("project"), None, Some(Locale::Zh)).unwrap();
    shell.import_file(fixture("tone-stereo-48000.wav")).unwrap();
    wait(&mut shell).unwrap();
    let id = shell.state["project"]["audio_assets"][0]["id"]
        .as_u64()
        .unwrap();
    let source = Source::read(
        &shell.state,
        AssetRef {
            kind: AssetKind::Audio,
            id,
        },
    )
    .unwrap();
    let before = shell.state.clone();
    let bad = root.path().join("bad.wav");
    std::fs::write(&bad, b"bad audio").unwrap();
    shell.replace_asset_file(source.clone(), bad).unwrap();
    assert!(wait(&mut shell).is_err());
    assert!(shell.replacing_asset.is_none());
    assert!(shell.importing_media.is_none());
    assert_eq!(shell.state["project"], before["project"]);
    shell
        .replace_asset_file(source.clone(), fixture("tone-stereo-48000.wav"))
        .unwrap();
    shell.action(panels::Action::CancelImport, None).unwrap();
    assert!(shell.replacing_asset.is_none());
    assert!(shell.importing_media.is_none());
    assert_eq!(shell.state["project"], before["project"]);
    shell
        .replace_asset_file(source, fixture("tone-stereo-48000.wav"))
        .unwrap();
    wait(&mut shell).unwrap();
    assert_eq!(shell.state["project"]["audio_assets"][0]["id"], id);
    assert_eq!(
        shell.state["project"]["layers"],
        before["project"]["layers"]
    );
    assert!(shell
        .panels
        .message
        .as_ref()
        .unwrap()
        .contains("素材已替换"));
    shell.engine.save().unwrap();
}

#[test]
fn file_location_failure_reports_native_cause_in_default_file_and_stderr_without_stdout() {
    const CHILD: &str = "MOTION_ASSET_FILE_LOCATION_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let blocked = root.join("blocked");
        std::fs::write(&blocked, b"file").unwrap();
        let path = blocked.join("missing/asset.png");
        let cause = reveal_file(&path, 42)
            .err()
            .expect("a file cannot contain a parent directory");
        assert!(
            cause.contains("inspect asset parent") || cause.contains("not a directory"),
            "{cause}"
        );
        let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(file.contains(&cause));
        assert!(file.contains("asset=42"));
        assert!(file.contains("phase=launch"));
        std::fs::write(root.join("cause.txt"), cause).unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","shell::media_assets::tests::file_location_failure_reports_native_cause_in_default_file_and_stderr_without_stdout","--nocapture"])
        .env_remove("MOTION_LOG").env(CHILD,root.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let cause = std::fs::read_to_string(root.path().join("cause.txt")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stderr.contains(&cause));
    assert!(!stderr.contains("location start"));
    assert!(!stdout.contains("location failed") && !stdout.contains("location start"));
}
