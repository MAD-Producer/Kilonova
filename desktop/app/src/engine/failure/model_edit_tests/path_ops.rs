use super::*;
use motion_core::vector::{path_ops, SampledDashes, SampledPath, SampledTrimPaths, TrimMode};

#[test]
fn actual_path_refusals_keep_native_identity_and_reach_default_reply_file_and_stderr() {
    const CHILD: &str = "MOTION_PATH_INPUT_LOG_CHILD";
    const NAME: &str = "engine::failure::model_edit_tests::path_ops::actual_path_refusals_keep_native_identity_and_reach_default_reply_file_and_stderr";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let trim = SampledTrimPaths {
            start: 0.,
            end: 100.,
            offset: 0.,
            mode: TrimMode::Simultaneously,
        };
        let dashes = SampledDashes {
            pattern: vec![10., 0.],
            offset: 0.,
        };
        let mut path = SampledPath {
            closed: false,
            nodes: vec![[0.; 6], [100., 0., 0., 0., 0., 0.]],
        };
        for (request, operation, value) in [(301, "trim", f32::NAN), (302, "dash", f32::INFINITY)] {
            path.nodes[1][4] = value;
            let error = if operation == "trim" {
                path_ops::trim(std::slice::from_ref(&path), &trim).unwrap_err()
            } else {
                path_ops::dash(std::slice::from_ref(&path), &dashes).unwrap_err()
            };
            let original = error.to_string();
            assert!(matches!(&error, motion_core::Error::Invalid(_)));
            let native = motion_host::Error::from(error);
            assert!(matches!(
                &native,
                motion_host::Error::Model(motion_core::Error::Invalid(_))
            ));
            let (reply, receive) = channel::<Result<(), String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_path_ops",
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
        // A normal empty interval or passthrough remains successful. Repeated
        // successes must not emit an Error merely because a prior call failed.
        path.nodes[1][4] = 0.;
        for _ in 0..60 {
            assert!(path_ops::trim(
                std::slice::from_ref(&path),
                &SampledTrimPaths { end: 0., ..trim }
            )
            .unwrap()
            .is_empty());
            assert_eq!(
                path_ops::dash(std::slice::from_ref(&path), &dashes).unwrap(),
                [path.clone()]
            );
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
    for (request, operation, value) in [(301, "trim", "NaN"), (302, "dash", "inf")] {
        let marker = format!("error #{request} test_path_ops ");
        assert_eq!(file.matches(&marker).count(), 1);
        assert_eq!(stderr.matches(&marker).count(), 1);
        let record = file.lines().find(|line| line.contains(&marker)).unwrap();
        for context in [
            format!("operation={operation}"),
            "contour=0 node=1 component=4".into(),
            format!("value={value}"),
        ] {
            assert!(record.contains(&context), "{record}");
            assert!(stderr.contains(&context), "{stderr}");
        }
    }
    assert_eq!(file.matches("invalid sampled path coordinate").count(), 2);
    assert_eq!(stderr.matches("invalid sampled path coordinate").count(), 2);
    assert!(!stderr.contains("logging initialized"));
    for marker in [
        "invalid sampled path coordinate",
        "test_path_ops",
        "logging initialized",
    ] {
        assert!(
            !stdout.contains(marker),
            "diagnostic leaked to stdout: {stdout}"
        );
    }
}
