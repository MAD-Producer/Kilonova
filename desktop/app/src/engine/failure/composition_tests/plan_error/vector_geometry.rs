use super::*;
use motion_render::effect_plan::PlanBuilder;
use std::{error::Error, sync::Arc};

mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/motion-render/tests/support/vector_error_fixture.rs"
    ));
}

fn native_model(error: &(dyn Error + 'static)) -> bool {
    error.is::<motion_model::Error>() || error.source().is_some_and(native_model)
}

#[test]
fn actual_vector_failures_preserve_native_causes_and_default_reply_channels() {
    const CHILD: &str = "MOTION_VECTOR_GEOMETRY_ERROR_CHILD";
    const NAME: &str = "engine::failure::composition_tests::plan_error::vector_geometry::actual_vector_failures_preserve_native_causes_and_default_reply_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let (project, mut scene) = fixture::scene();
        let before = serde_json::to_value(&project).unwrap();
        let healthy = scene.layers[0].vector.clone();
        let mut builder = PlanBuilder::new(motion_effects::Registry::default()).unwrap();
        let expected = fixture::bits(builder.build(&scene, &[0], 64, 64, true).unwrap());
        let mut causes = Vec::new();
        for (index, (name, vector)) in fixture::malformed().into_iter().enumerate() {
            let input = Arc::new(vector);
            scene.layers[0].vector = Some(input.clone());
            let failure = builder.build(&scene, &[0], 64, 64, true).unwrap_err();
            let host = motion_host::Error::from(failure);
            if name.contains("trim") || name == "dash" {
                assert!(native_model(&host));
            }
            let original = host.to_string();
            for part in ["layer 7", "vector rasterize", "frame 5.5"] {
                assert!(original.contains(part));
            }
            let (reply, receive) = channel::<Result<Value, String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request: 1101 + index as u64,
                operation: "test_vector_geometry",
                started: Instant::now(),
            }));
            respond(reply, Err(host));
            CURRENT_COMMAND.set(None);
            assert_eq!(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err(),
                original
            );
            assert!(Arc::ptr_eq(
                scene.layers[0].vector.as_ref().unwrap(),
                &input
            ));
            assert_eq!(serde_json::to_value(&project).unwrap(), before);
            scene.layers[0].vector = healthy.clone();
            assert_eq!(
                fixture::bits(builder.build(&scene, &[0], 64, 64, true).unwrap()),
                expected
            );
            causes.push(original);
        }
        std::fs::write(
            root.join("primary-causes.json"),
            serde_json::to_vec(&causes).unwrap(),
        )
        .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let causes: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.path().join("primary-causes.json")).unwrap())
            .unwrap();
    assert_eq!(causes.len(), 6);
    assert!(file.contains("logging initialized level=INFO"));
    for (request, cause) in (1101..1107).zip(causes) {
        let marker = format!("error #{request} test_vector_geometry ");
        for text in [&file, &stderr] {
            let lines: Vec<_> = text.lines().filter(|line| line.contains(&marker)).collect();
            assert_eq!(lines.len(), 1);
            for part in [&cause, "layer 7", "vector rasterize", "frame 5.5", "ms:"] {
                assert!(lines[0].contains(part), "{text}");
            }
        }
        assert!(!stdout.contains(&cause));
        assert!(!stdout.contains(&marker));
    }
    assert!(!stderr.contains("logging initialized"));
    assert!(!stdout.contains("logging initialized"));
}
