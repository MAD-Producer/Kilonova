//! Surface-specific preparation failure through the real default desktop sink.
use super::*;
use motion_core::color_management::{ColorTransfer, ProjectColorManagement};
use motion_effects::{color_processor::ColorProcessorPhase, native_color::NativeColorRuntime};
use std::sync::mpsc;

#[test]
#[ignore = "requires explicit checksum-verified native OCIO prefix"]
fn surface_preparation_publishes_before_blocking_and_preserves_default_failure_channels() {
    const CHILD: &str = "MOTION_OCIO_SURFACE_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::native_authored::surface::surface_preparation_publishes_before_blocking_and_preserves_default_failure_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut project = project(&root);
        project.layers[0].three_d = false;
        let ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut project.color_management)
        else {
            unreachable!()
        };
        settings.display.transfer = ColorTransfer::Srgb;
        let before = serde_json::to_value(&project).unwrap();
        let manifest = PathBuf::from(std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap());
        let runtime = NativeColorRuntime::load(&manifest).unwrap();
        let prepared = prep::prepare_native_project_colors(
            &project,
            &root,
            &manifest,
            &Observation::new(9600),
        )
        .unwrap()
        .unwrap();
        assert_eq!(prepared.program_count(), 4);
        let retained_key = prepared.surface_display().unwrap().key();

        for fault in 0..2 {
            let observation = Observation::new(9601 + fault);
            let (entered, waiting) = mpsc::sync_channel(1);
            let (release, released) = mpsc::sync_channel(1);
            std::thread::scope(|scope| {
                let monitor_observation = &observation;
                let log_path = root.join("logs/motion-studio.log");
                let monitor = scope.spawn(move || {
                    waiting.recv_timeout(Duration::from_secs(10)).unwrap();
                    let running = monitor_observation.running();
                    let file = fs::read_to_string(log_path).unwrap();
                    // Release before assertions so a diagnostic failure cannot strand the provider.
                    release.send(()).unwrap();
                    assert!(running);
                    let prefix = format!("color preparation request={}", monitor_observation.request());
                    assert!(file.contains(&format!("{prefix} operation=prepare_project_colors phase=start")), "{file}");
                    assert!(file.contains(&format!("{prefix} operation=prepare_project_colors target=SurfaceDisplay phase=provider")), "{file}");
                    assert!(!file.lines().any(|line| line.contains(&prefix) && line.contains("phase=failed")));
                });
                let error = prep::prepare_project_colors(
                    &project,
                    &root,
                    |request| {
                        assert!(observation.running());
                        if request.target == prep::Target::SurfaceDisplay {
                            assert_eq!(request.phase, ColorProcessorPhase::WorkingToDisplay);
                            entered.send(()).unwrap();
                            released.recv_timeout(Duration::from_secs(10)).unwrap();
                            if fault == 0 {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::NotFound,
                                    "original surface provider unavailable",
                                )
                                .into());
                            }
                        }
                        let native = runtime.prepare(
                            request.config,
                            request.selection,
                            request.phase,
                            request.input_alpha,
                            request.output_alpha,
                        )?;
                        let mut source = native.source().clone();
                        if request.target == prep::Target::SurfaceDisplay {
                            source.shader = "invalid original surface GLSL".into();
                        }
                        Ok(source)
                    },
                    &observation,
                )
                .err()
                .unwrap();
                monitor.join().unwrap();
                match (fault, error) {
                    (
                        0,
                        prep::Error::Provider {
                            target: prep::Target::SurfaceDisplay,
                            phase: ColorProcessorPhase::WorkingToDisplay,
                            source,
                        },
                    ) => {
                        let motion_host::Error::Io(cause) = *source else {
                            panic!("original typed IO cause must survive")
                        };
                        assert_eq!(cause.kind(), std::io::ErrorKind::NotFound);
                        assert_eq!(cause.to_string(), "original surface provider unavailable");
                    }
                    (
                        1,
                        prep::Error::Compile {
                            target: prep::Target::SurfaceDisplay,
                            phase: ColorProcessorPhase::WorkingToDisplay,
                            ..
                        },
                    ) => {}
                    (_, error) => panic!("wrong failure: {error:?}"),
                }
            });
            assert!(!observation.running());
            assert_eq!(serde_json::to_value(&project).unwrap(), before);
            assert_eq!(prepared.surface_display().unwrap().key(), retained_key);
            assert_eq!(prepared.program_count(), 4);
        }
        let recovered = prep::prepare_native_project_colors(
            &project,
            &root,
            &manifest,
            &Observation::new(9603),
        )
        .unwrap()
        .unwrap();
        assert_eq!(recovered.surface_display().unwrap().key(), retained_key);
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
        let mut incompatible = project.clone();
        let ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut incompatible.color_management)
        else {
            unreachable!()
        };
        settings.display.transfer = ColorTransfer::ConfigDefined;
        let explicit = prep::prepare_native_project_colors(
            &incompatible,
            &root,
            &manifest,
            &Observation::new(9604),
        )
        .unwrap()
        .unwrap();
        assert!(explicit.surface_display().is_none());
        assert_eq!(explicit.program_count(), 3);
        assert_eq!(prepared.surface_display().unwrap().key(), retained_key);
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
            panic!("surface preparation child exceeded 30s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("surface-preparation");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("motion-studio.log"), &file).unwrap();
        fs::write(directory.join("child.stdout"), &stdout).unwrap();
        fs::write(directory.join("child.stderr"), &stderr).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains(" ERROR "))
        .collect();
    assert_eq!(errors.len(), 2, "{file}");
    for (request, cause) in [
        (9601, "original surface provider unavailable"),
        (9602, "compilation failed"),
    ] {
        let prefix = format!("color preparation request={request}");
        let error = errors.iter().find(|line| line.contains(&prefix)).unwrap();
        for context in [
            "target=SurfaceDisplay",
            "WorkingToDisplay",
            "elapsed_ms=",
            cause,
        ] {
            assert!(error.contains(context), "{error}");
            assert!(stderr.contains(context), "{stderr}");
        }
        let message = error.split_once(": ").unwrap().1;
        assert!(stderr.contains(message), "{stderr}");
        assert!(!stdout.contains(message), "{stdout}");
        assert!(!stdout.contains(&prefix), "{stdout}");
    }
    assert!(!stderr.contains("phase=start") && !stderr.contains("phase=provider"));
    assert!(!stdout.contains("phase=start") && !stdout.contains("phase=provider"));
    assert!(!file.contains("phase=abandoned"));
    let fingerprint =
        "shader_sha256=c9125f1ffde5481767d9a579257f32c8cfc894444a343d29270792c982f70f2d";
    let compile = file
        .lines()
        .find(|line| {
            line.contains("request=9602")
                && line.contains("phase=compile ")
                && line.contains("target=SurfaceDisplay")
        })
        .unwrap();
    for context in [
        fingerprint,
        "shader_bytes=29",
        "config_key=",
        "selection_key=",
    ] {
        assert!(compile.contains(context), "{compile}");
    }
    let failed = errors
        .iter()
        .find(|line| line.contains("request=9602"))
        .unwrap();
    for context in [fingerprint, "shader_bytes=29", "phase=glsl-parse"] {
        assert!(failed.contains(context), "{failed}");
        assert!(stderr.contains(context), "{stderr}");
        assert!(!stdout.contains(context), "{stdout}");
    }
    assert!(!file.contains("invalid original surface GLSL"));
    assert!(!stderr.contains("invalid original surface GLSL"));
}
