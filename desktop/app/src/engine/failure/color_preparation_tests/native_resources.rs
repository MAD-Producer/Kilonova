//! Real edit channel and default sink for verified resource parsing failures.
use super::*;
use crate::engine::failure::color_tests::runtime_platform::RuntimePlatform;
use motion_core::color_management::*;
use motion_effects::{color_config::ReferenceSpace, image_contract::ChannelPrecision, AlphaMode};
#[path = "../../../../../../crates/motion-effects/tests/support/native_resources.rs"]
mod resources;

fn settings(case: &resources::Case, root: &std::path::Path) -> Arc<ProjectColorManagement> {
    let verified = resources::config(case);
    let selection = resources::selection(&verified);
    let input = fixture::signal("scene-linear", ReferenceSpace::Scene);
    let mut working = input.clone();
    working.alpha = AlphaMode::Premultiplied;
    let mut display = input.clone();
    display.reference = ReferenceSpace::Display;
    let settings = OcioProject {
        manifest: verified.manifest().clone(),
        selection: selection.clone(),
        default_input: input.clone(),
        authored_input: ColorInput {
            space: selection.input,
            signal: input.clone(),
        },
        working,
        display,
        output: input,
        sources: vec![],
    };
    let result = Arc::new(ProjectColorManagement::Ocio {
        settings: Box::new(settings),
    });
    for resource in result.resources().unwrap() {
        let path = root.join(&resource.path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            if resource.path.ends_with("/config.ocio") {
                case.config.as_bytes()
            } else {
                case.cube.as_bytes()
            },
        )
        .unwrap();
    }
    result
}

#[test]
#[ignore = "requires fresh native resource oracle and actual GPU"]
fn resource_failures_reach_default_channels_and_preserve_the_complete_edit_state() {
    const CHILD: &str = "MOTION_OCIO_RESOURCE_LOG_CHILD";
    const TEST:&str="engine::failure::color_preparation_tests::native_resources::resource_failures_reach_default_channels_and_preserve_the_complete_edit_state";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let active = root.join("active");
        let mut project = motion_core::Project::new(32, 32, 30, 120).unwrap();
        project.working_precision = ChannelPrecision::Float32;
        motion_core::storage::save(&active, &project).unwrap();
        let platform = Arc::new(RuntimePlatform {
            manifest: std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap().into(),
            before_manifest: None,
            desktop: motion_desktop_media::platform(),
        });
        let engine = Engine::start(active.clone(), platform, 8 << 30).unwrap();
        let before = engine.state().unwrap();
        let case = resources::cases().remove(3);
        let mut bad = case.clone();
        bad.cube = "LUT_3D_SIZE 5\ninvalid values\n".into();
        let mut undeclared = case.clone();
        undeclared.config = undeclared.config.replace(&case.name, "../missing.cube");
        assert_ne!(undeclared.config, case.config);
        let mut unknown = case.clone();
        unknown.name = unknown.name.replace(".cube", ".clf");
        unknown.config = unknown.config.replace(&case.name, &unknown.name);
        let mut failures = Vec::new();
        for (candidate, cause) in [
            (bad, "expected exactly three finite numeric values"),
            (
                undeclared,
                "external FileTransform has no verified resource",
            ),
            (unknown, "only strict single-table .cube resources"),
        ] {
            let declaration = settings(&candidate, &active);
            let error = engine
                .edit(
                    serde_json::to_string(&motion_core::Command::SetColorManagement {
                        settings: declaration,
                    })
                    .unwrap(),
                )
                .unwrap_err();
            let mut chain = Some(&error as &(dyn std::error::Error + 'static));
            let mut explanation = Vec::new();
            while let Some(source) = chain {
                assert!(explanation.len() < 32, "error source cycle");
                explanation.push(source.to_string());
                chain = source.source();
            }
            let explanation = explanation.join(": ");
            assert!(explanation.contains(cause), "{explanation}");
            let after = engine.state().unwrap();
            for key in ["project", "revision", "history", "canUndo", "canRedo"] {
                assert!(before.get(key).is_some(), "state must expose {key}");
                assert_eq!(after[key], before[key], "failed candidate changed {key}");
            }
            failures.push(cause);
        }
        let valid = settings(&case, &active);
        engine
            .edit(
                serde_json::to_string(&motion_core::Command::SetColorManagement {
                    settings: valid.clone(),
                })
                .unwrap(),
            )
            .unwrap();
        let installed = engine.state().unwrap();
        assert_eq!(
            installed["project"]["color_management"],
            serde_json::to_value(valid).unwrap()
        );
        assert_ne!(installed["project"], before["project"]);
        fs::write(
            root.join("causes.json"),
            serde_json::to_vec(&failures).unwrap(),
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
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(90) {
            let kill = child.kill();
            let wait = child.wait();
            panic!("resource worker child exceeded 90s: kill={kill:?} wait={wait:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("native-resources");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("default.log"), &file).unwrap();
        fs::write(directory.join("stderr.txt"), &stderr).unwrap();
        fs::write(directory.join("stdout.txt"), &stdout).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let causes: Vec<String> =
        serde_json::from_slice(&fs::read(root.path().join("causes.json")).unwrap()).unwrap();
    let valid = |file: &str, stderr: &str, stdout: &str| {
        file.contains("logging initialized level=INFO")
            && file.contains("operation=prepare_project_colors phase=start")
            && causes.iter().all(|cause| {
                file.lines().any(|line| {
                    line.contains("ERROR") && line.contains(cause) && line.contains("elapsed_ms=")
                }) && stderr.contains(cause)
                    && !stdout.contains(cause)
            })
            && !stdout.contains("start #")
            && !stdout.contains("color preparation request=")
            && !stderr.contains("phase=start")
    };
    assert!(valid(&file, &stderr, &stdout), "{file}\n{stderr}\n{stdout}");
    assert!(!valid(&file.replace("ERROR", "DEBUG"), &stderr, &stdout));
    assert!(!valid(&file, "", &stdout));
    assert!(!valid(&file, &stderr, &format!("{stdout}{stderr}")));
    assert!(!valid(
        &file.replace(&causes[0], "lost cause"),
        &stderr,
        &stdout
    ));
}
