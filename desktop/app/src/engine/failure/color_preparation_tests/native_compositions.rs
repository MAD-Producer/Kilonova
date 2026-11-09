//! Managed composition failures use the existing serial command/error channel.
use super::*;
use crate::engine::failure::color_tests::runtime_platform::RuntimePlatform;

#[test]
#[ignore = "requires the pinned native OCIO runtime"]
fn managed_composition_worker_keeps_original_causes_and_default_error_channels() {
    const TEST:&str="engine::failure::color_preparation_tests::native_authored::compositions::managed_composition_worker_keeps_original_causes_and_default_error_channels";
    const CHILD: &str = "MOTION_OCIO_COMPOSITION_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let active = root.join("library/default");
        let mut managed = project(&active);
        managed.layers[0].three_d = false;
        let main = managed.composition_id.clone();
        let mut target = motion_core::Composition {
            id: "comp-target".into(),
            name: "managed target".into(),
            width: managed.width,
            height: managed.height,
            fps: managed.fps,
            frames: managed.frames,
            work_area: managed.work_area,
            guides: vec![],
            background: managed.background,
            camera: managed.camera.clone(),
            layers: managed.layers.clone(),
            expressions: vec![],
        };
        managed.compositions.push(target.clone());
        target.id = "comp-unsupported".into();
        target.layers[0].three_d = true;
        managed.compositions.push(target.clone());
        target.id = "comp-expression".into();
        target.layers[0].three_d = false;
        target.expressions.push(motion_core::PropertyExpression {
            target: motion_core::ExpressionTarget::Property {
                object: 1,
                property: motion_core::Property::Opacity,
                axis: None,
            },
            source: "throw new Error('managed-composition-original-cause')".into(),
            enabled: true,
            seed: 0,
            profile: motion_core::EXPRESSION_PROFILE.into(),
        });
        managed.compositions.push(target);
        let mut initial = managed.clone();
        initial.color_management =
            Arc::new(motion_core::color_management::ProjectColorManagement::Unmanaged {});
        motion_core::storage::save(&active, &initial).unwrap();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = calls.clone();
        let engine = Engine::start(
            active.clone(),
            Arc::new(RuntimePlatform {
                manifest: PathBuf::from(std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap()),
                before_manifest: Some(Arc::new(move || {
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                })),
                desktop: motion_desktop_media::platform(),
            }),
            8 << 30,
        )
        .unwrap();
        engine
            .edit(
                serde_json::to_string(&motion_core::Command::SetColorManagement {
                    settings: managed.color_management.clone(),
                })
                .unwrap(),
            )
            .unwrap();
        fs::remove_file(active.join(&managed.color_management.resources().unwrap()[0].path))
            .unwrap();
        let open = |id: &str| {
            engine.composition(
                serde_json::json!({"version":1,"composition":id,"op":"open"}).to_string(),
            )
        };
        for id in ["comp-target", &main] {
            open(id).unwrap();
        }
        for id in ["comp-unsupported", "comp-expression"] {
            let before = engine.state().unwrap();
            let error = open(id).unwrap_err();
            let Some(motion_host::Error::CompositionContext { cause, .. }) = error.host() else {
                panic!("{error:?}")
            };
            if id == "comp-unsupported" {
                let motion_host::Error::ColorPreparation(prep::Error::Composition {
                    composition,
                    source,
                }) = cause.as_ref()
                else {
                    panic!("{error:?}")
                };
                assert_eq!(composition, id);
                assert!(
                    matches!(
                        source.as_ref(),
                        motion_host::Error::Render(motion_render::RenderError::AuthoredScene(
                            motion_render::AuthoredSceneError::Admission {
                                phase: "admission",
                                object: Some(1),
                                ..
                            }
                        ))
                    ),
                    "{error:?}"
                );
            } else {
                let motion_host::Error::Context { context, cause } = cause.as_ref() else {
                    panic!("{error:?}")
                };
                assert_eq!(context, "composition=comp-expression phase=sample");
                assert!(
                    matches!(
                        cause.as_ref(),
                        motion_host::Error::Render(motion_render::RenderError::AuthoredScene(
                            motion_render::AuthoredSceneError::Model {
                                source: motion_core::Error::Expression { .. },
                                ..
                            }
                        ))
                    ),
                    "{error:?}"
                );
                assert!(
                    error
                        .to_string()
                        .contains("managed-composition-original-cause"),
                    "{error}"
                );
            }
            let after = engine.state().unwrap();
            for key in [
                "project",
                "revision",
                "history",
                "canUndo",
                "canRedo",
                "frame",
                "composition_context",
                "composition",
                "document_generation",
                "viewRevision",
                "surfaceEpoch",
                "renderError",
            ] {
                assert!(before.get(key).is_some(), "missing state contract {key}");
                assert_eq!(
                    after[key], before[key],
                    "failed composition={id} changed {key}"
                );
            }
            assert!(!engine.failed());
            open("comp-target").unwrap();
            open(&main).unwrap();
        }
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "activation must use retained native/config owners"
        );
        drop(engine);
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
        .stdout(fs::File::create(root.path().join("stdout")).unwrap())
        .stderr(fs::File::create(root.path().join("stderr")).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let killed = child.kill();
            let waited = child.wait();
            panic!("composition worker exceeded 45s: kill={killed:?} wait={waited:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(root.path().join("stdout")).unwrap();
    let stderr = fs::read_to_string(root.path().join("stderr")).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(dir) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let dir = PathBuf::from(dir).join("managed-composition-worker");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("stdout"), &stdout).unwrap();
        fs::write(dir.join("stderr"), &stderr).unwrap();
        fs::write(dir.join("motion-studio.log"), &file).unwrap();
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
            .filter(|line| line.contains("phase=composition") && line.contains("phase=admission"))
            .count(),
        2,
        "{file}"
    );
    assert_eq!(
        errors
            .iter()
            .filter(|line| line.contains("managed-composition-original-cause"))
            .count(),
        1,
        "{file}"
    );
    for error in errors {
        let message = error.split_once(": ").unwrap().1;
        assert!(stderr.contains(message), "{stderr}");
        assert!(!stdout.contains(message), "{stdout}");
        if let Some(request) = message
            .strip_prefix("error #")
            .and_then(|s| s.split_whitespace().next())
        {
            let start = format!("start #{request} composition");
            assert!(
                file.find(&start).unwrap() < file.find(error).unwrap(),
                "{file}"
            );
            let duration = message
                .split_whitespace()
                .nth(3)
                .unwrap()
                .strip_suffix("ms:")
                .unwrap();
            assert!(duration.parse::<u64>().is_ok(), "{message}");
        } else {
            assert!(message.contains("elapsed_ms="), "{message}");
        }
    }
    assert!(file.contains("operation=prepare_color_composition phase=start"));
    assert!(file.contains("composition=comp-unsupported"));
    assert!(file.contains("composition=comp-expression"));
    assert!(!stdout.contains("start #") && !stderr.contains("start #"));
    assert!(!stdout.contains("phase=start") && !stderr.contains("phase=start"));
    assert!(!file.contains("phase=abandoned"));
}
