//! Actual authored consumer refusal through the normal default desktop sink.
use super::*;
use motion_core::color_management::SignalRange;
use motion_effects::{color_config::*, image_contract::ChannelPrecision, AlphaMode};
#[path = "native_catalogue.rs"]
mod catalogue;
#[path = "native_images.rs"]
mod images;
#[path = "native_session.rs"]
mod session;
#[path = "native_surface.rs"]
mod surface;
#[path = "native_png.rs"]
mod png;
#[cfg(target_os = "windows")]
#[path = "native_window.rs"]
mod window;

fn project(root: &std::path::Path) -> motion_core::Project {
    #[derive(serde::Deserialize)]
    struct Config {
        manifest: ColorConfigManifest,
        config: String,
        selection: ColorSelection,
    }
    #[derive(serde::Deserialize)]
    struct Reference {
        configs: Vec<Config>,
    }
    let reference: Reference = serde_json::from_slice(include_bytes!(
        "../../../../../../crates/motion-effects/tests/color_processor/reference.json"
    ))
    .unwrap();
    let config = &reference.configs[0];
    let mut settings = fixture::settings();
    settings.manifest = config.manifest.clone();
    settings.selection = config.selection.clone();
    let signal = |space: &SpaceReference| {
        let declared =
            &settings.manifest.spaces[settings.manifest.declared_color_space(space).unwrap()];
        fixture::signal(&declared.encoding, declared.reference)
    };
    settings.default_input = signal(&settings.selection.input);
    settings.authored_input.space = settings.selection.input.clone();
    settings.authored_input.signal = settings.default_input.clone();
    settings.working = signal(&settings.selection.working);
    settings.working.alpha = AlphaMode::Premultiplied;
    settings.working.range = SignalRange::Full {};
    settings.display = settings.default_input.clone();
    settings.display.reference = ReferenceSpace::Display;
    settings.output = settings.default_input.clone();
    let mut p = motion_core::Project::new(64, 64, 30, 120).unwrap();
    p.working_precision = ChannelPrecision::Float32;
    p.color_management = fixture::managed(settings);
    let mut layer = motion_core::Layer::solid(
        1,
        "unsupported authored consumer",
        [64.; 2],
        [32., 32., 0.],
        [1.; 4],
    );
    layer.three_d = true;
    p.layers.push(layer);
    p.validate().unwrap();
    let resource = &p.color_management.resources().unwrap()[0];
    let path = root.join(&resource.path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, &config.config).unwrap();
    p
}

#[test]
#[ignore = "requires explicit checksum-verified native OCIO prefix"]
fn unsupported_authored_consumer_reaches_default_channels_without_partial_publication() {
    const CHILD: &str = "MOTION_OCIO_AUTHORED_LOG_CHILD";
    const TEST: &str="engine::failure::color_preparation_tests::native_authored::unsupported_authored_consumer_reaches_default_channels_without_partial_publication";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let p = project(&root);
        let before = serde_json::to_value(&p).unwrap();
        let runtime = PathBuf::from(
            std::env::var_os("MOTION_TEST_OCIO_RUNTIME").expect("explicit native runtime"),
        );
        let observation = Observation::new(8801);
        let error = prep::prepare_native_authored_project(&p, &root, &runtime, &observation)
            .err()
            .unwrap();
        let prep::Error::Provider { source, .. } = error else {
            panic!("original provider failure")
        };
        assert!(matches!(
            *source,
            motion_host::Error::Render(motion_render::RenderError::AuthoredScene(
                motion_render::AuthoredSceneError::Admission {
                    phase: "admission",
                    object: Some(1),
                    ..
                }
            ))
        ));
        assert!(!observation.running());
        assert_eq!(serde_json::to_value(&p).unwrap(), before);
        let unmanaged = motion_core::Project::new(64, 64, 30, 120).unwrap();
        assert!(prep::prepare_native_authored_project(
            &unmanaged,
            &root,
            std::path::Path::new("absent-runtime"),
            &observation
        )
        .unwrap()
        .is_none());
        assert!(!observation.running());
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
        if began.elapsed() > Duration::from_secs(20) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("authored log child exceeded 20s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(&stdout_path).unwrap();
    let stderr = fs::read_to_string(&stderr_path).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("authored-refusal");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("motion-studio.log"), &file).unwrap();
        fs::write(directory.join("child.stdout"), &stdout).unwrap();
        fs::write(directory.join("child.stderr"), &stderr).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let start = "color preparation request=8801 operation=prepare_project_colors phase=start";
    let valid = |file: &str, stderr: &str, stdout: &str| {
        let errors: Vec<_> = file
            .lines()
            .filter(|l| l.contains("ERROR") && l.contains("request=8801"))
            .collect();
        errors.len() == 1
            && file.contains("logging initialized level=INFO")
            && file
                .find(start)
                .zip(file.find(errors[0]))
                .is_some_and(|(s, e)| s < e)
            && [
                "target=DefaultInput",
                "provider failed",
                "phase=admission",
                "object=Some(1)",
                "not integrated",
                "elapsed_ms=",
            ]
            .iter()
            .all(|v| errors[0].contains(v) && stderr.contains(v) && !stdout.contains(v))
            && stderr
                .lines()
                .filter(|l| l.contains("request=8801"))
                .count()
                == 1
            && !stderr.contains(start)
            && !stdout.contains("color preparation request=")
    };
    assert!(valid(&file, &stderr, &stdout), "{file}\n{stderr}\n{stdout}");
    assert!(!valid(&file.replace("ERROR", "DEBUG"), &stderr, &stdout));
    assert!(!valid(&file, "", &stdout));
    assert!(!valid(
        &file.replace("object=Some(1)", "object=None"),
        &stderr,
        &stdout
    ));
    assert!(!valid(&file, &stderr, &format!("{stdout}{stderr}")));
}
