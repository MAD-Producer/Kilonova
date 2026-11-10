//! Wrong ACES provenance uses the existing default file/stderr error channels.
use super::*;

#[test]
#[ignore = "requires the pinned native OCIO runtime"]
fn builtin_origin_refusals_preserve_default_channels_and_the_complete_project() {
    const CHILD: &str = "MOTION_OCIO_ORIGIN_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::native_authored::origin::builtin_origin_refusals_preserve_default_channels_and_the_complete_project";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut healthy = project(&root);
        healthy.layers[0].three_d = false;
        let original =
            fs::read(root.join(&healthy.color_management.resources().unwrap()[0].path)).unwrap();
        let runtime = PathBuf::from(std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap());
        for index in 0..3 {
            let mut p = healthy.clone();
            let mut bytes = original.clone();
            let motion_core::color_management::ProjectColorManagement::Ocio { settings } =
                Arc::make_mut(&mut p.color_management)
            else {
                unreachable!()
            };
            let ConfigOrigin::Builtin { id, aces } = &mut settings.manifest.origin else {
                unreachable!()
            };
            let field = match index {
                0 => {
                    aces.major = 2;
                    aces.minor = 0;
                    settings.selection.aces = Some(*aces);
                    "aces_version"
                }
                1 => {
                    *id = "cg-config-latest".into();
                    "builtin_id"
                }
                _ => {
                    let text = String::from_utf8(bytes).unwrap().replace(
                        "luma: [0.2126, 0.7152, 0.0722]",
                        "luma: [0.2127, 0.7151, 0.0722]",
                    );
                    bytes = text.into_bytes();
                    assert_ne!(bytes, original);
                    settings.manifest.config = ContentIdentity::of(&bytes);
                    "builtin_id"
                }
            };
            settings.selection.config_key = VerifiedColorConfig::new(
                settings.manifest.clone(),
                Arc::from(bytes.as_slice()),
                Default::default(),
            )
            .unwrap()
            .key();
            p.validate().unwrap();
            let path = root.join(&p.color_management.resources().unwrap()[0].path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
            let before = serde_json::to_value(&p).unwrap();
            let observation = Observation::new(8931 + index);
            let error = prep::prepare_native_authored_project(&p, &root, &runtime, &observation)
                .err()
                .unwrap();
            let prep::Error::Provider { source, .. } = error else {
                panic!("original provider cause");
            };
            assert!(matches!(source.as_ref(), motion_host::Error::NativeColor(
                motion_effects::native_color::Error::Catalogue { field: actual, .. }) if *actual == field));
            assert!(!observation.running());
            assert_eq!(serde_json::to_value(&p).unwrap(), before);
        }
        let before = serde_json::to_value(&healthy).unwrap();
        let recovery = Observation::new(8934);
        assert!(
            prep::prepare_native_authored_project(&healthy, &root, &runtime, &recovery)
                .unwrap()
                .is_some()
        );
        assert!(!recovery.running());
        assert_eq!(serde_json::to_value(&healthy).unwrap(), before);
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
            panic!("origin log child exceeded 60s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(root.path().join("child.stdout")).unwrap();
    let stderr = fs::read_to_string(root.path().join("child.stderr")).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("builtin-origin");
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
    let valid = |file: &str, stderr: &str, stdout: &str| {
        file.contains("logging initialized level=INFO")
            && [
                (8931, "aces_version", "actual=\"1.3.0\""),
                (8932, "builtin_id", "cg-config-latest"),
                (8933, "builtin_id", "actual=\"unverified\""),
            ]
            .iter()
            .all(|(request, field, detail)| {
                let request = format!("request={request}");
                let start = format!("{request} operation=prepare_project_colors phase=start");
                let errors: Vec<_> = file
                    .lines()
                    .filter(|line| line.contains("ERROR") && line.contains(&request))
                    .collect();
                errors.len() == 1
                    && file
                        .find(&start)
                        .zip(file.find(errors[0]))
                        .is_some_and(|(s, e)| s < e)
                    && [
                        format!("field={field}"),
                        "target=DefaultInput".into(),
                        "phase=catalogue".into(),
                        "elapsed_ms=".into(),
                        detail.to_string(),
                    ]
                    .iter()
                    .all(|v| errors[0].contains(v) && stderr.contains(v))
                    && stderr
                        .lines()
                        .filter(|line| line.contains(&request))
                        .count()
                        == 1
                    && errors[0]
                        .split_once("color preparation ")
                        .is_some_and(|(_, body)| stderr.contains(body))
                    && !stderr.contains(&start)
            })
            && !file
                .lines()
                .any(|line| line.contains("ERROR") && line.contains("request=8934"))
            && !stdout.contains("color preparation request=")
    };
    assert!(valid(&file, &stderr, &stdout), "{file}\n{stderr}\n{stdout}");
    assert!(!valid(&file.replace("ERROR", "DEBUG"), &stderr, &stdout));
    assert!(!valid(&file, "", &stdout));
    assert!(!valid(
        &file,
        &stderr.replace("actual=\"1.3.0\"", "hidden version"),
        &stdout
    ));
    assert!(!valid(
        &file.replace("actual=\"1.3.0\"", "hidden version"),
        &stderr,
        &stdout
    ));
    assert!(!valid(&file, &stderr, &format!("{stdout}{stderr}")));
}
