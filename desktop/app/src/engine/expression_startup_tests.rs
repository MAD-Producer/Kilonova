use super::*;

fn project(source: &str) -> motion_core::Project {
    let mut project = motion_core::Project::new(64, 48, 30, 60).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Loaded expression",
        [20.; 2],
        [50., 0., 0.],
        [1.; 4],
    ));
    project.expressions.push(motion_core::PropertyExpression {
        target: motion_core::ExpressionTarget::Property {
            object: 1,
            property: motion_core::Property::Position,
            axis: None,
        },
        source: source.into(),
        enabled: true,
        seed: 7,
        profile: motion_core::EXPRESSION_PROFILE.into(),
    });
    project
}

#[test]
fn persisted_expression_preparation_keeps_first_frame_authored_data_and_default_error_sinks() {
    const CHILD: &str = "MOTION_EXPRESSION_STARTUP_ROOT";
    if let Some(directory) = std::env::var_os(CHILD) {
        let directory = PathBuf::from(directory);
        let _logger = crate::logging::init(directory.clone()).unwrap();
        for (name, source, valid) in [("valid", "[500,0,0]", true), ("syntax", "[500,", false)] {
            let root = directory.join(name);
            std::fs::create_dir(&root).unwrap();
            let project = project(source);
            motion_core::storage::save(&root, &project).unwrap();
            let host =
                Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
            let initial = host.state().unwrap();
            assert_eq!(initial["project"], serde_json::to_value(&project).unwrap());
            assert_eq!(
                initial["sampledLayers"][0]["position"][0],
                if valid { 500. } else { 50. }
            );
            if valid {
                assert!(initial["renderError"].is_null(), "{initial}");
            } else {
                let error = initial["renderError"].as_str().unwrap();
                assert!(
                    error.contains("expression") && error.contains("JavaScript"),
                    "{error}"
                );
                // Exercise the existing per-session retained-error threshold.
                for _ in 0..70 {
                    assert_eq!(host.state().unwrap()["renderError"], initial["renderError"]);
                }
                let state = host
                    .edit(
                        serde_json::to_string(&motion_core::Command::RemoveExpression {
                            target: project.expressions[0].target.clone(),
                        })
                        .unwrap(),
                    )
                    .unwrap();
                assert!(state["renderError"].is_null(), "{state}");
                assert_eq!(state["sampledLayers"][0]["position"][0], 50.);
                assert_eq!(host.history(0).unwrap()["project"], initial["project"]);
            }
            // No failed preparation rewrites persisted authored data.
            assert_eq!(motion_core::storage::load(&root).unwrap(), project);
        }
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap()).args([
        "--exact", "engine::expression_startup_tests::persisted_expression_preparation_keeps_first_frame_authored_data_and_default_error_sinks",
    ]).env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "child status={}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let marker = "site=host.session.last_error";
    assert!(
        log.lines().any(|s| s.contains("ERROR")
            && s.contains(marker)
            && s.contains("calls_with_latched_error=60")),
        "{log}"
    );
    assert!(
        stderr.contains(marker) && stderr.contains("JavaScript"),
        "{stderr}"
    );
    assert!(
        !stdout.contains(marker) && !stdout.contains("JavaScript"),
        "{stdout}"
    );
    if let Some(artifacts) = std::env::var_os("MOTION_EXPRESSION_STARTUP_ARTIFACTS") {
        let artifacts = PathBuf::from(artifacts);
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::write(artifacts.join("default-file.log"), log).unwrap();
        std::fs::write(artifacts.join("default-stderr.log"), &output.stderr).unwrap();
        std::fs::write(artifacts.join("default-stdout.log"), &output.stdout).unwrap();
    }
}
