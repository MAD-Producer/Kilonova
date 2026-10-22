use super::*;
use motion_render::image_resources::Error as ImageError;

#[test]
fn native_image_failures_keep_default_output_channels_and_recover() {
    const CHILD: &str = "MOTION_IMAGE_ERROR_LOG_CHILD";
    const NAME: &str = "engine::failure::tests::image_errors::native_image_failures_keep_default_output_channels_and_recover";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project = root.join("project");
        std::fs::create_dir_all(project.join("assets")).unwrap();
        let path = project.join("assets/image.png");
        let missing = motion_host::ops::images::image_info(path.to_str().unwrap()).unwrap_err();
        assert!(
            matches!(&missing, motion_host::Error::ImageResource(ImageError::Io(cause))
            if cause.kind() == std::io::ErrorKind::NotFound)
        );
        let invalid =
            motion_host::ops::images::prepare_image(project.clone(), "../outside.png".into())
                .unwrap_err();
        assert!(matches!(&invalid, motion_host::Error::Model(_)));
        image::save_buffer(&path, &[20, 40, 60, 255], 1, 1, image::ColorType::Rgba8).unwrap();
        let healthy = std::fs::read(&path).unwrap();
        let mut corrupt = healthy.clone();
        let idat = corrupt.windows(4).position(|b| b == b"IDAT").unwrap();
        let len = u32::from_be_bytes(corrupt[idat - 4..idat].try_into().unwrap()) as usize;
        corrupt[idat + 4 + len] ^= 1;
        std::fs::write(&path, corrupt).unwrap();
        let png =
            motion_host::ops::images::prepare_image(project.clone(), "assets/image.png".into())
                .unwrap_err();
        assert!(matches!(
            &png,
            motion_host::Error::ImageResource(ImageError::Png(_))
        ));
        let mut causes = Vec::new();
        for (request, error) in [(621, missing), (622, invalid), (623, png)] {
            let original = error.to_string();
            let (reply, receive) = channel::<Result<Value, String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_image_decode",
                started: Instant::now(),
            }));
            respond(reply, Err(error));
            CURRENT_COMMAND.set(None);
            assert_eq!(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err(),
                original
            );
            causes.push(original);
        }
        std::fs::write(&path, healthy).unwrap();
        assert_eq!(
            motion_host::ops::images::image_info(path.to_str().unwrap()).unwrap()["width"],
            1
        );
        let ready =
            motion_host::ops::images::prepare_image(project, "assets/image.png".into()).unwrap();
        assert_eq!(ready["validated"], true);
        assert_eq!(ready["proxyWidth"], 1);
        std::fs::write(
            root.join("primary-causes.json"),
            serde_json::to_vec(&causes).unwrap(),
        )
        .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let causes: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.path().join("primary-causes.json")).unwrap())
            .unwrap();
    assert_eq!(causes.len(), 3);
    assert!(file.contains("logging initialized level=INFO"));
    for (request, cause) in (621..624).zip(causes) {
        let marker = format!("error #{request} test_image_decode ");
        for text in [&file, &stderr] {
            let records: Vec<_> = text.lines().filter(|line| line.contains(&marker)).collect();
            assert_eq!(records.len(), 1, "{text}");
            assert!(records[0].contains(&cause));
        }
        assert!(!stdout.contains(&cause));
    }
    for text in [&file, &stderr] {
        assert_eq!(
            text.lines().filter(|line| line.contains("ERROR")).count(),
            3,
            "{text}"
        );
        assert!(!text.contains("image decode superseded"));
    }
    for marker in ["logging initialized", "test_image_decode", "INFO", "ERROR"] {
        assert!(!stdout.contains(marker), "{stdout}");
    }
}
