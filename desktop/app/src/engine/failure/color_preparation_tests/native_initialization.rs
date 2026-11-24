//! Real C++ bad_alloc through the product initializer and desktop logger.
use super::*;
#[path = "../../../../../../crates/motion-effects/tests/support/native_color_failure.rs"]
mod initialization_fixture;

#[test]
#[ignore = "requires explicit qualification-only native initialization-failure prefix"]
fn native_initialization_exception_reaches_default_channels_without_changing_the_project() {
    const CHILD: &str = "MOTION_OCIO_INIT_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::native_initialization::native_initialization_exception_reaches_default_channels_without_changing_the_project";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        #[derive(serde::Deserialize)]
        struct Reference {
            configs: Vec<Config>,
        }
        #[derive(serde::Deserialize)]
        struct Config {
            manifest: motion_effects::color_config::ColorConfigManifest,
            config: String,
            selection: motion_effects::color_config::ColorSelection,
        }
        let reference: Reference = serde_json::from_slice(include_bytes!(
            "../../../../../../crates/motion-effects/tests/color_processor/reference.json"
        ))
        .unwrap();
        let config = &reference.configs[0];
        let mut project = fixture::project();
        let motion_core::color_management::ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut project.color_management)
        else {
            panic!("managed fixture");
        };
        // Real closed ACES configuration: execution must reach the actual native
        // loader/initializer, rather than fail earlier at a synthetic resource.
        settings.manifest = config.manifest.clone();
        settings.selection = config.selection.clone();
        settings.authored_input.space = config.selection.input.clone();
        settings.default_input.encoding = config
            .manifest
            .declared_color_space(&config.selection.input)
            .map(|name| config.manifest.spaces[name].encoding.clone())
            .unwrap();
        settings.default_input.reference = config.manifest.spaces[config
            .manifest
            .declared_color_space(&config.selection.input)
            .unwrap()]
        .reference;
        settings.authored_input.signal = settings.default_input.clone();
        settings.working.encoding = config.manifest.spaces[config
            .manifest
            .declared_color_space(&config.selection.working)
            .unwrap()]
        .encoding
        .clone();
        settings.working.reference = config.manifest.spaces[config
            .manifest
            .declared_color_space(&config.selection.working)
            .unwrap()]
        .reference;
        // The explicit display/output are the same encoded signal in this fixture.
        settings.display.encoding = settings.default_input.encoding.clone();
        settings.output = settings.default_input.clone();
        project.validate().unwrap();
        let resource = &project.color_management.resources().unwrap()[0];
        let path = root.join(&resource.path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        assert_eq!(
            resource.identity,
            motion_effects::color_config::ContentIdentity::of(config.config.as_bytes())
        );
        fs::write(path, &config.config).unwrap();
        let before = serde_json::to_value(&project).unwrap();
        let observation = Observation::new(8301);
        let runtime = initialization_fixture::create(&root.join("fault-runtime"));
        let error = prep::prepare_native_project_colors(&project, &root, &runtime, &observation)
            .err()
            .unwrap();
        let prep::Error::Provider {
            target: prep::Target::DefaultInput,
            source,
            ..
        } = error
        else {
            panic!("original provider target");
        };
        let motion_host::Error::NativeColor(motion_effects::native_color::Error::Native {
            phase: "initialize",
            category: 3,
            cause,
        }) = *source
        else {
            panic!("actual native initialization bad_alloc");
        };
        assert!(!cause.is_empty());
        fs::write(root.join("original-cause.txt"), cause).unwrap();
        assert!(!observation.running());
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
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
            panic!("native initialization log child exceeded 20s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let cause = fs::read_to_string(root.path().join("original-cause.txt")).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("native-initialization");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("motion-studio.log"), &file).unwrap();
        fs::write(directory.join("child.stdout"), &stdout).unwrap();
        fs::write(directory.join("child.stderr"), &stderr).unwrap();
        fs::write(directory.join("original-cause.txt"), &cause).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let start = "color preparation request=8301 operation=prepare_project_colors phase=start";
    let verifies = |file: &str, stderr: &str, stdout: &str| {
        let errors: Vec<_> = file
            .lines()
            .filter(|line| line.contains("ERROR") && line.contains("request=8301"))
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
                "phase=initialize",
                "category=3",
                "elapsed_ms=",
                &cause,
            ]
            .iter()
            .all(|text| errors[0].contains(text) && stderr.contains(text) && !stdout.contains(text))
            && stderr
                .lines()
                .filter(|line| line.contains("request=8301"))
                .count()
                == 1
            && !stderr.contains(start)
            && !stdout.contains("color preparation request=")
    };
    assert!(
        verifies(&file, &stderr, &stdout),
        "{file}\n{stderr}\n{stdout}"
    );
    assert!(!verifies(&file.replace("ERROR", "DEBUG"), &stderr, &stdout));
    assert!(!verifies(&file, "", &stdout));
    assert!(!verifies(
        &file.replace(&cause, "lost-native-cause"),
        &stderr,
        &stdout
    ));
    assert!(!verifies(&file, &stderr, &format!("{stdout}{stderr}")));
}
