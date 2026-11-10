//! A structurally valid but false manifest must retain the real native cause.
use super::*;

#[test]
#[ignore = "requires explicit checksum-verified native OCIO prefix"]
fn catalogue_mismatch_reaches_default_channels_and_does_not_publish_a_project() {
    const CHILD: &str = "MOTION_OCIO_CATALOGUE_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::native_authored::catalogue::catalogue_mismatch_reaches_default_channels_and_does_not_publish_a_project";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut p = project(&root);
        p.layers[0].three_d = false;
        let healthy = p.clone();
        let motion_core::color_management::ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut p.color_management)
        else {
            panic!("managed fixture");
        };
        settings
            .manifest
            .spaces
            .get_mut("Gamma 1.8 Encoded Rec.709")
            .unwrap()
            .encoding = "log".into();
        let resource = root.join(&healthy.color_management.resources().unwrap()[0].path);
        let bytes = fs::read(resource).unwrap();
        let config = VerifiedColorConfig::new(
            settings.manifest.clone(),
            Arc::from(bytes.as_slice()),
            Default::default(),
        )
        .unwrap();
        settings.selection.config_key = config.key();
        p.validate().unwrap();
        // Resource directories include the full manifest identity. Keep the
        // same actual config bytes at the rekeyed fixture path as well.
        let changed_path = root.join(&p.color_management.resources().unwrap()[0].path);
        fs::create_dir_all(changed_path.parent().unwrap()).unwrap();
        fs::write(changed_path, bytes).unwrap();
        let before = serde_json::to_value(&p).unwrap();
        let runtime = PathBuf::from(std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap());
        let observation = Observation::new(8921);
        let error = prep::prepare_native_authored_project(&p, &root, &runtime, &observation)
            .err()
            .unwrap();
        let prep::Error::Provider { source, .. } = error else {
            panic!("original provider failure");
        };
        assert!(
            matches!(*source, motion_host::Error::NativeColor(motion_effects::native_color::Error::Catalogue {
            field: "encoding", ref name, ref expected, ref actual,
        }) if name == "Gamma 1.8 Encoded Rec.709" && expected == "log" && actual == "sdr-video")
        );
        assert!(!observation.running());
        assert_eq!(serde_json::to_value(&p).unwrap(), before);
        let recovered = Observation::new(8922);
        assert!(
            prep::prepare_native_authored_project(&healthy, &root, &runtime, &recovered)
                .unwrap()
                .is_some()
        );
        assert!(!recovered.running());
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
            panic!("catalogue log child exceeded 20s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("catalogue-mismatch");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("motion-studio.log"), &file).unwrap();
        fs::write(directory.join("child.stdout"), &stdout).unwrap();
        fs::write(directory.join("child.stderr"), &stderr).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let start = "request=8921 operation=prepare_project_colors phase=start";
    let valid = |file: &str, stderr: &str, stdout: &str| {
        let errors: Vec<_> = file
            .lines()
            .filter(|l| l.contains("ERROR") && l.contains("request=8921"))
            .collect();
        errors.len() == 1
            && file.contains("logging initialized level=INFO")
            && file
                .find(start)
                .zip(file.find(errors[0]))
                .is_some_and(|(s, e)| s < e)
            && [
                "target=DefaultInput",
                "phase=catalogue",
                "field=encoding",
                "Gamma 1.8 Encoded Rec.709",
                "expected=\"log\"",
                "actual=\"sdr-video\"",
                "elapsed_ms=",
            ]
            .iter()
            .all(|v| errors[0].contains(v) && stderr.contains(v) && !stdout.contains(v))
            && stderr
                .lines()
                .filter(|l| l.contains("request=8921"))
                .count()
                == 1
            && !stderr.contains("phase=start")
            && !stderr.contains("request=8922")
            && !file
                .lines()
                .any(|l| l.contains("ERROR") && l.contains("request=8922"))
            && !stdout.contains("color preparation request=")
    };
    assert!(valid(&file, &stderr, &stdout), "{file}\n{stderr}\n{stdout}");
    assert!(!valid(&file.replace("ERROR", "DEBUG"), &stderr, &stdout));
    assert!(!valid(&file, "", &stdout));
    assert!(!valid(
        &file.replace("field=encoding", "field=roles"),
        &stderr,
        &stdout
    ));
    assert!(!valid(&file, &stderr, &format!("{stdout}{stderr}")));
}
