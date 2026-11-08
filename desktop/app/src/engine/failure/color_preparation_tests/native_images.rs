//! Real missing input and normal cancellation through the default desktop sink.
use super::*;
use std::sync::atomic::AtomicBool;

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn authored_image_failure_reaches_default_channels_and_cancellation_is_normal() {
    const CHILD: &str = "MOTION_OCIO_IMAGE_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::native_authored::images::authored_image_failure_reaches_default_channels_and_cancellation_is_normal";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut p = project(&root);
        p.layers[0].three_d = false;
        p.layers[0].content = motion_core::Content::Image { asset: 1 };
        p.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/missing.png".into(),
            width: 64,
            height: 64,
        });
        let before = serde_json::to_value(&p).unwrap();
        let runtime = PathBuf::from(std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap());
        let prepared =
            prep::prepare_native_authored_project(&p, &root, &runtime, &Observation::new(8910))
                .unwrap()
                .unwrap();
        let mut renderer =
            pollster::block_on(motion_render::Renderer::headless_with_working_precision(
                ChannelPrecision::Float32,
                Default::default(),
            ))
            .unwrap();
        let observation = Observation::new(8911);
        let error = prepared
            .prepare_images(
                &mut renderer,
                &p,
                &root,
                &AtomicBool::new(false),
                &observation,
            )
            .err()
            .unwrap();
        let prep::Error::SourceImage {
            asset: 1,
            phase: "resolve_source",
            source,
        } = error
        else {
            panic!("original source failure")
        };
        let motion_host::Error::ImageResource(motion_render::image_resources::Error::Io(source)) =
            *source
        else {
            panic!("original IO error")
        };
        assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        assert!(!observation.running());
        let cancelled = Observation::new(8912);
        let error = prepared
            .prepare_images(&mut renderer, &p, &root, &AtomicBool::new(true), &cancelled)
            .err()
            .unwrap();
        assert!(matches!(
            error,
            prep::Error::SourceImage {
                phase: "cancel",
                ..
            }
        ));
        assert!(!cancelled.running());
        assert_eq!(serde_json::to_value(&p).unwrap(), before);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout_path = root.path().join("child.stdout");
    let stderr_path = root.path().join("child.stderr");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .stdout(fs::File::create(&stdout_path).unwrap())
        .stderr(fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(60) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("image log child exceeded 60s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("authored-image-failure");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("motion-studio.log"), &file).unwrap();
        fs::write(directory.join("child.stdout"), &stdout).unwrap();
        fs::write(directory.join("child.stderr"), &stderr).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let valid = |file: &str, stderr: &str, stdout: &str| {
        let errors: Vec<_> = file
            .lines()
            .filter(|line| line.contains("ERROR") && line.contains("request=8911"))
            .collect();
        errors.len() == 2
            && file.contains("logging initialized level=INFO")
            && [
                ("prepare_authored_images", ""),
                ("prepare_source_color_image", " asset=1"),
            ]
            .iter()
            .all(|(operation, context)| {
                let start = format!("request=8911 operation={operation}{context}");
                file.find(&format!("{start} phase=start"))
                    .zip(file.rfind(&format!("{start} phase=")))
                    .is_some_and(|(first, last)| first < last)
            })
            && errors.iter().all(|error| {
                ["asset=1", "phase=resolve_source", "os error", "elapsed_ms="]
                    .iter()
                    .all(|text| error.contains(text) && stderr.contains(text))
            })
            && stderr
                .lines()
                .filter(|line| line.contains("request=8911"))
                .count()
                == 2
            && file.contains("assets/missing.png")
            && file.contains("request=8912 operation=prepare_authored_images phase=start")
            && !file
                .lines()
                .any(|line| line.contains("ERROR") && line.contains("request=8912"))
            && !stderr.contains("phase=start")
            && !stderr.contains("request=8912")
            && !stdout.contains("color preparation request=")
    };
    assert!(valid(&file, &stderr, &stdout), "{file}\n{stderr}\n{stdout}");
    assert!(!valid(&file.replace("ERROR", "DEBUG"), &stderr, &stdout));
    assert!(!valid(&file, "", &stdout));
    assert!(!valid(
        &file.replace("asset=1", "asset=2"),
        &stderr,
        &stdout
    ));
    assert!(!valid(&file, &stderr, &format!("{stdout}{stderr}")));
}
