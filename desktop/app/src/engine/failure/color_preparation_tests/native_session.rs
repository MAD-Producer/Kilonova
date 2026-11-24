//! Actual worker admission, blocked start publication, retained-program edits,
//! transactional native failure, explicit recovery and default output channels.
use super::*;
use crate::engine::failure::color_tests::runtime_platform::RuntimePlatform;

#[test]
#[ignore = "requires explicit checksum-verified native OCIO prefix"]
fn managed_session_worker_publishes_before_blocking_and_preserves_failed_edits() {
    const CHILD: &str = "MOTION_OCIO_SESSION_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::native_authored::session::managed_session_worker_publishes_before_blocking_and_preserves_failed_edits";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let active = root.join("library/default");
        let mut managed = project(&active);
        managed.layers[0].three_d = false;
        managed.layers[0].name = "managed worker solid".into();
        let expected = serde_json::to_value(&managed).unwrap();
        let mut initial = managed.clone();
        initial.color_management =
            Arc::new(motion_core::color_management::ProjectColorManagement::Unmanaged {});
        motion_core::storage::save(&active, &initial).unwrap();
        let (entered, waiting) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let released = std::sync::Mutex::new(released);
        let first = std::sync::atomic::AtomicBool::new(true);
        let platform = Arc::new(RuntimePlatform {
            manifest: PathBuf::from(std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap()),
            before_manifest: Some(Arc::new(move || {
                if first.swap(false, std::sync::atomic::Ordering::AcqRel) {
                    entered.send(()).unwrap();
                    released
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                }
            })),
            desktop: motion_desktop_media::platform(),
        });
        let engine = Engine::start(active.clone(), platform, 8 << 30).unwrap();
        // A reply is sent before the worker clears its flight. Observe eventual
        // completion without assuming the reply and cleanup are simultaneous.
        let wait_for_idle = || {
            let began = Instant::now();
            while engine.in_flight().is_some() {
                assert!(
                    began.elapsed() < Duration::from_secs(5),
                    "worker did not clear in-flight state"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(engine.in_flight().is_none());
        };
        let flight = engine.in_flight.clone();
        let log_path = root.join("logs/motion-studio.log");
        let monitor = std::thread::spawn(move || {
            waiting.recv_timeout(Duration::from_secs(10)).unwrap();
            let current = *flight.lock().unwrap();
            let file = fs::read_to_string(log_path).unwrap();
            // Always release the provider before asserting diagnostic evidence.
            release.send(()).unwrap();
            let current = current.expect("published in-flight before runtime capability");
            assert_eq!(current.operation, "edit");
            assert!(
                file.contains(&format!("start #{} edit", current.request)),
                "{file}"
            );
            current.request
        });
        let set = |settings| {
            serde_json::to_string(&motion_core::Command::SetColorManagement { settings }).unwrap()
        };
        engine.edit(set(managed.color_management.clone())).unwrap();
        let blocked_request = monitor.join().unwrap();
        assert_eq!(engine.state().unwrap()["project"], expected);
        wait_for_idle();
        let resource = &managed.color_management.resources().unwrap()[0];
        let config_path = active.join(&resource.path);
        let bytes = fs::read(&config_path).unwrap();
        fs::remove_file(&config_path).unwrap();
        engine
            .edit(r#"{"op":"set_color","object":1,"value":[0.2,0.4,0.6,1]}"#.into())
            .unwrap();
        let before = engine.state().unwrap();
        let mut changed = managed.color_management.clone();
        let motion_core::color_management::ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut changed)
        else {
            unreachable!()
        };
        settings.output.range = SignalRange::Limited {
            black: 0.1,
            white: 0.9,
        };
        let error = engine.edit(set(changed.clone())).unwrap_err();
        assert!(matches!(
            error.host(),
            Some(motion_host::Error::ColorPreparation(
                prep::Error::Resources(_)
            ))
        ));
        let after = engine.state().unwrap();
        for key in ["project", "revision", "history", "canUndo", "canRedo"] {
            assert_eq!(after[key], before[key], "{key}");
        }
        assert!(!engine.failed());
        wait_for_idle();
        fs::write(config_path, bytes).unwrap();
        engine.edit(set(changed.clone())).unwrap();
        assert_eq!(
            engine.state().unwrap()["project"]["color_management"],
            serde_json::to_value(changed).unwrap()
        );
        assert!(!engine.failed());
        engine.edit(serde_json::to_string(&motion_core::Command::SetExpression {
            expression: motion_core::PropertyExpression {
                target: motion_core::ExpressionTarget::Property {
                    object: 1, property: motion_core::Property::Position, axis: Some(motion_core::Axis::X),
                },
                source: "time > 0 ? (() => { throw new Error('managed-frame-cause') })() : value".into(),
                enabled: true, seed: 42, profile: motion_core::EXPRESSION_PROFILE.into(),
            }, frame: 0,
        }).unwrap()).unwrap();
        let before_evaluation = engine.state().unwrap();
        let error = engine.seek(30.).unwrap_err();
        assert!(matches!(
            error.host(),
            Some(motion_host::Error::Render(
                motion_render::RenderError::AuthoredScene(
                    motion_render::AuthoredSceneError::Model {
                        source: motion_core::Error::Expression { .. },
                        ..
                    }
                )
            ))
        ));
        assert!(error.to_string().contains("managed-frame-cause"), "{error}");
        let after_evaluation = engine.state().unwrap();
        assert!(after_evaluation["renderError"]
            .as_str()
            .unwrap()
            .contains("managed-frame-cause"));
        for key in ["project", "revision", "history", "canUndo", "canRedo"] {
            assert_eq!(
                after_evaluation[key], before_evaluation[key],
                "evaluation changed {key}"
            );
        }
        let recovered = engine.seek(0.).unwrap();
        assert!(recovered["renderError"].is_null(), "{recovered}");
        assert!(!engine.failed());
        drop(engine);
        fs::write(
            root.join("blocked-request.txt"),
            blocked_request.to_string(),
        )
        .unwrap();
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
        if began.elapsed() > Duration::from_secs(30) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("managed Session child exceeded 30s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(evidence) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let evidence = PathBuf::from(evidence).join("managed-session-worker");
        fs::create_dir_all(&evidence).unwrap();
        fs::write(evidence.join("motion-studio.log"), &file).unwrap();
        fs::write(evidence.join("child.stdout"), &stdout).unwrap();
        fs::write(evidence.join("child.stderr"), &stderr).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains(" ERROR "))
        .collect();
    assert_eq!(errors.len(), 3, "{file}");
    assert_eq!(
        errors
            .iter()
            .filter(|line| line.contains("phase=resources") && line.contains("phase=resolve"))
            .count(),
        2
    );
    assert_eq!(
        errors
            .iter()
            .filter(|line| line.contains(" seek ") && line.contains("managed-frame-cause"))
            .count(),
        1
    );
    for error in errors {
        let message = error.split_once(": ").unwrap().1;
        assert!(stderr.contains(message), "{stderr}");
        assert!(!stdout.contains(message), "{stdout}");
    }
    assert!(file
        .lines()
        .any(|line| line.contains("phase=start") && line.contains("deferred=true")));
    assert!(!stderr.contains("start #") && !stdout.contains("start #"));
    assert!(!stderr.contains("phase=start") && !stdout.contains("phase=start"));
    assert!(!file.contains("phase=abandoned"));
}
