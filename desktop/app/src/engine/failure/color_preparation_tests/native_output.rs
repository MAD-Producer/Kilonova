//! Real native output preparation failure through the default desktop sink.
use super::*;

#[test]
#[ignore = "requires the pinned native OCIO runtime"]
fn opaque_output_retains_original_runtime_failure_and_default_error_channels() {
    const CHILD: &str = "MOTION_OCIO_OPAQUE_OUTPUT_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::native_authored::output::opaque_output_retains_original_runtime_failure_and_default_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut p = project(&root);
        p.layers[0].three_d = false;
        let before = serde_json::to_value(&p).unwrap();
        let runtime = PathBuf::from(std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap());
        let prepared =
            prep::prepare_native_project_colors(&p, &root, &runtime, &Observation::new(8383))
                .unwrap()
                .unwrap();
        let original_key = prepared.output().key();
        let original_count = prepared.program_count();
        let original_payload = prepared.payload_bytes();
        let observation = Observation::new(8384);
        let missing = root.join("missing-opaque-output-runtime.json");
        let error = prep::prepare_native_opaque_output(&prepared, &missing, &observation)
            .err()
            .unwrap();
        assert!(matches!(
            error,
            prep::Error::Provider {
                target: prep::Target::Output,
                phase: motion_effects::color_processor::ColorProcessorPhase::WorkingToOutput,
                ..
            }
        ));
        let mut causes = Vec::new();
        let mut cause: Option<&dyn std::error::Error> = Some(&error);
        while let Some(value) = cause {
            causes.push(value.to_string());
            cause = value.source();
        }
        assert!(causes
            .iter()
            .any(|cause| cause.contains("missing-opaque-output-runtime.json")));
        assert!(!observation.running());
        assert_eq!(prepared.output().key(), original_key);
        assert_eq!(prepared.program_count(), original_count);
        assert_eq!(prepared.payload_bytes(), original_payload);
        assert_eq!(serde_json::to_value(&p).unwrap(), before);
        // Explicit new request, not an automatic retry of a failed operation.
        let recovery =
            prep::prepare_native_opaque_output(&prepared, &runtime, &Observation::new(8385))
                .unwrap();
        assert!(recovery.is_opaque_output());
        assert!(!prepared.output().is_opaque_output());
        fs::write(
            root.join("causes.json"),
            serde_json::to_vec(&causes).unwrap(),
        )
        .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
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
        .stdout(fs::File::create(root.path().join("child.stdout")).unwrap())
        .stderr(fs::File::create(root.path().join("child.stderr")).unwrap())
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
            panic!("output log child exceeded 60s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(root.path().join("child.stdout")).unwrap();
    let stderr = fs::read_to_string(root.path().join("child.stderr")).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("opaque-output");
        fs::create_dir_all(&directory).unwrap();
        for (name, value) in [
            ("motion-studio.log", &file),
            ("child.stdout", &stdout),
            ("child.stderr", &stderr),
        ] {
            fs::write(directory.join(name), value).unwrap();
        }
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let causes: Vec<String> =
        serde_json::from_slice(&fs::read(root.path().join("causes.json")).unwrap()).unwrap();
    let start = "request=8384 operation=prepare_opaque_color_output phase=start";
    let valid = |file: &str, stderr: &str, stdout: &str| {
        let errors: Vec<_> = file
            .lines()
            .filter(|line| line.contains("ERROR") && line.contains("request=8384"))
            .collect();
        errors.len() == 1
            && file.contains("logging initialized level=INFO")
            && file
                .find(start)
                .zip(file.find(errors[0]))
                .is_some_and(|(start, error)| start < error)
            && [
                "operation=prepare_opaque_color_output",
                "target=Output",
                "WorkingToOutput",
                "elapsed_ms=",
                "missing-opaque-output-runtime.json",
            ]
            .iter()
            .all(|v| errors[0].contains(v) && stderr.contains(v))
            && causes
                .iter()
                .all(|cause| errors[0].contains(cause) && stderr.contains(cause))
            && !stdout.contains("color preparation request=")
            && !stderr.contains(start)
            && stderr
                .lines()
                .filter(|line| line.contains("request=8384"))
                .count()
                == 1
    };
    assert!(valid(&file, &stderr, &stdout), "{file}\n{stderr}\n{stdout}");
    assert!(!valid(&file.replace("ERROR", "DEBUG"), &stderr, &stdout));
    assert!(!valid(&file, "", &stdout));
    assert!(!valid(
        &file.replace("missing-opaque-output-runtime.json", "hidden cause"),
        &stderr,
        &stdout
    ));
    assert!(!valid(&file, &stderr, &format!("{stdout}{stderr}")));
}
