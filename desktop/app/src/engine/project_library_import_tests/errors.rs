use super::*;

#[test]
fn project_library_import_failures_preserve_publication_and_reach_default_file_stderr_without_stdout(
) {
    const CHILD: &str = "MOTION_LIBRARY_IMPORT_ERROR_CHILD";
    const NAME:&str="engine::project_library_import_tests::errors::project_library_import_failures_preserve_publication_and_reach_default_file_stderr_without_stdout";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let e = engine(&root.join("project"));
        let original = e.state().unwrap();
        let before = folder(&e, 7);
        let source = picture(&root);
        let mut reasons = Vec::new();
        reasons.push(
            apply(
                &e,
                &before,
                Request::Image {
                    file: root.join("missing-import-source.png"),
                    folder: Some(7),
                },
            )
            .unwrap_err(),
        );
        reasons.push(
            apply(
                &e,
                &before,
                Request::Image {
                    file: source.clone(),
                    folder: Some(999),
                },
            )
            .unwrap_err(),
        );
        assert_eq!(e.state().unwrap()["project"], before["project"]);
        assert!(std::fs::read_dir(root.join("project/assets"))
            .unwrap()
            .next()
            .is_none());
        start(
            &e,
            "bad-library-folder",
            &fixture("tone-stereo-48000.wav"),
            "audio",
        );
        reasons.push(
            apply(
                &e,
                &before,
                Request::Media {
                    request: "bad-library-folder".into(),
                    folder: Some(999),
                },
            )
            .unwrap_err(),
        );
        assert_eq!(e.state().unwrap()["project"], before["project"]);
        assert!(std::fs::read_dir(root.join("project/assets"))
            .unwrap()
            .next()
            .is_none());
        e.media(json!({"op":"release_media_task","request_id":"bad-library-folder"}).to_string())
            .unwrap();
        reasons.push(
            apply(
                &e,
                &before,
                Request::Place {
                    target: AssetRef {
                        kind: AssetKind::Image,
                        id: 404,
                    },
                    at_frame: 0,
                },
            )
            .unwrap_err(),
        );
        assert_eq!(e.state().unwrap()["project"], before["project"]);
        assert_eq!(
            apply(
                &e,
                &original,
                Request::Image {
                    file: root.join("stale-must-not-open.png"),
                    folder: None,
                }
            )
            .unwrap()["assetOperation"]["cancelled"],
            true
        );
        let registered = apply(
            &e,
            &before,
            Request::Image {
                file: source,
                folder: Some(7),
            },
        )
        .unwrap();
        let target = AssetRef {
            kind: AssetKind::Image,
            id: registered["assetRegistration"]["asset"].as_u64().unwrap(),
        };
        let placed = apply(
            &e,
            &registered,
            Request::Place {
                target,
                at_frame: 0,
            },
        )
        .unwrap();
        assert_eq!(placed["project"]["layers"].as_array().unwrap().len(), 1);
        std::fs::write(
            root.join("causes.json"),
            serde_json::to_vec(&reasons).unwrap(),
        )
        .unwrap();
        e.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LIBRARY_IMPORT_ERROR_EVIDENCE")
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let causes: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.join("causes.json")).unwrap()).unwrap();
    assert_eq!(causes.len(), 4);
    for cause in &causes {
        assert!(
            file.lines()
                .any(|line| line.contains(" ERROR ") && line.contains(cause)),
            "{cause}\n{file}"
        );
        assert!(stderr.contains(cause), "{cause}\n{stderr}");
        assert!(!stdout.contains(cause), "{stdout}");
    }
    for operation in [
        "project_register_image",
        "project_register_media",
        "project_place_asset",
    ] {
        assert!(
            file.lines().any(|line| line.contains(" INFO ")
                && line.contains("start #")
                && line.ends_with(operation)),
            "{file}"
        );
        assert!(!stdout.contains(operation));
    }
    assert!(!stderr.contains("start #") && !stdout.contains("start #"));
    assert!(!stderr.contains("stale-must-not-open"));
}
