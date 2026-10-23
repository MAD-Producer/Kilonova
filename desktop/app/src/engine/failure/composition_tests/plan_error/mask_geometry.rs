use super::*;
use motion_render::{effect_plan::PlanBuilder, RenderError, VectorGeometryError};
use std::{error::Error, sync::Arc};

mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/motion-render/tests/support/mask_error_fixture.rs"
    ));
}

fn native_geometry(error: &(dyn Error + 'static)) -> bool {
    matches!(error.downcast_ref::<RenderError>(), Some(RenderError::VectorGeometry(VectorGeometryError::Invalid(message)))
        if message == "vector path coordinate exceeds numeric range")
        || error
            .downcast_ref::<Box<RenderError>>()
            .is_some_and(|cause| native_geometry(cause.as_ref()))
        || error.source().is_some_and(native_geometry)
}

#[test]
fn actual_mask_failures_keep_causes_and_default_reply_channels_then_recover() {
    const CHILD: &str = "MOTION_MASK_GEOMETRY_ERROR_CHILD";
    const NAME: &str = "engine::failure::composition_tests::plan_error::mask_geometry::actual_mask_failures_keep_causes_and_default_reply_channels_then_recover";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let (project, mut scene) = fixture::scene();
        let before = serde_json::to_value(&project).unwrap();
        let healthy = scene.layers[0].masks.clone();
        let mut builder = PlanBuilder::new(motion_effects::Registry::default()).unwrap();
        let first = builder.build(&scene, &[0], 64, 64, true).unwrap();
        let expected = fixture::bits(first);
        let cached = first.masks[0].vertices.clone();
        let mut causes = Vec::new();
        for (index, (_, mask)) in fixture::malformed().into_iter().enumerate() {
            let input = Arc::new(vec![mask]);
            scene.layers[0].masks = input.clone();
            let failure = builder.build(&scene, &[0], 64, 64, true).unwrap_err();
            let host = motion_host::Error::from(failure);
            assert!(native_geometry(&host));
            let original = host.to_string();
            let (reply, receive) = channel::<Result<Value, String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request: 1401 + index as u64,
                operation: "test_mask_geometry",
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
            assert!(Arc::ptr_eq(&scene.layers[0].masks, &input));
            assert_eq!(serde_json::to_value(&project).unwrap(), before);
            scene.layers[0].masks = healthy.clone();
            let recovered = builder.build(&scene, &[0], 64, 64, true).unwrap();
            assert_eq!(fixture::bits(recovered), expected);
            assert!(Arc::ptr_eq(&cached, &recovered.masks[0].vertices));
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
    assert_eq!(causes.len(), 3);
    assert!(file.contains("logging initialized level=INFO"));
    for (request, cause) in (1401..1404).zip(causes) {
        let marker = format!("error #{request} test_mask_geometry ");
        for text in [&file, &stderr] {
            let lines: Vec<_> = text.lines().filter(|line| line.contains(&marker)).collect();
            assert_eq!(lines.len(), 1);
            for part in [
                &cause,
                "layer 7",
                "mask 19",
                "mask tessellate",
                "frame 5.5",
                "ms:",
            ] {
                assert!(lines[0].contains(part), "{text}");
            }
        }
        assert!(!stdout.contains(&cause));
        assert!(!stdout.contains(&marker));
    }
    assert!(!stderr.contains("logging initialized"));
    assert!(!stdout.contains("logging initialized"));
}
