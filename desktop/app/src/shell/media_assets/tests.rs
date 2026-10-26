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
