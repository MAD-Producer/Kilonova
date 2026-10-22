use super::*;
use motion_core::vector::{ShapeKind, VectorAction, VectorContent, VectorSource, SHAPES};

#[test]
fn actual_missing_shape_parameters_reach_native_reply_and_default_error_sinks() {
    const CHILD: &str = "MOTION_SHAPE_PARAMETER_LOG_CHILD";
    const NAME: &str = "engine::failure::model_edit_tests::shape_parameters::actual_missing_shape_parameters_reach_native_reply_and_default_error_sinks";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let mut vector = VectorContent::shape(ShapeKind::Rectangle);
        let VectorSource::Shape { parameters, .. } = &mut vector.source else {
            panic!("expected parametric fixture");
        };
        assert!(parameters.remove("angle").is_some());
        let before = vector.clone();
        for (request, operation) in [(411, "sample"), (412, "edit")] {
            let error = if operation == "sample" {
                vector.sample(7., [20.; 2]).unwrap_err()
            } else {
                vector
                    .edit(
                        VectorAction::SetParameter {
                            parameter: "angle".into(),
                            frame: 7,
                            value: 0.,
                            animated: true,
                        },
                        [20.; 2],
                        0,
                    )
                    .unwrap_err()
            };
            assert_eq!(vector, before);
            assert!(matches!(&error, motion_core::Error::Invalid(_)));
            let original = error.to_string();
            let native = motion_host::Error::from(error);
            assert!(matches!(
                &native,
                motion_host::Error::Model(motion_core::Error::Invalid(_))
            ));
            let (reply, receive) = channel::<Result<(), String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_shape_parameter",
                started: Instant::now(),
            }));
            respond(reply, Err(native));
            CURRENT_COMMAND.set(None);
            assert_eq!(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err(),
                original
            );
        }
        for (shape, ..) in SHAPES {
            VectorContent::shape(shape).sample(7., [20.; 2]).unwrap();
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME])
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
    assert!(file.contains("logging initialized level=INFO"));
    for (request, operation) in [(411, "sample"), (412, "edit")] {
        let marker = format!("error #{request} test_shape_parameter ");
        for text in [&file, &stderr] {
            assert_eq!(text.matches(&marker).count(), 1);
            let line = text.lines().find(|line| line.contains(&marker)).unwrap();
            for context in [
                format!("operation={operation}"),
                "shape=Rectangle".into(),
                "parameter=angle frame=7".into(),
            ] {
                assert!(line.contains(&context), "{line}");
            }
        }
    }
    assert_eq!(file.matches("missing shape parameter").count(), 2);
    assert_eq!(stderr.matches("missing shape parameter").count(), 2);
    assert!(!stderr.contains("logging initialized"));
    for marker in [
        "missing shape parameter",
        "test_shape_parameter",
        "logging initialized",
    ] {
        assert!(!stdout.contains(marker), "{stdout}");
    }
}
