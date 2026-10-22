use super::*;
use motion_core::{AxisTracks, Content, Layer, Project, Track, VideoClip};

#[test]
fn actual_timeline_refusals_keep_native_causes_and_default_error_channels() {
    const CHILD: &str = "MOTION_TIMELINE_QUERY_LOG_CHILD";
    const NAME: &str = "engine::failure::model_edit_tests::timeline_queries::actual_timeline_refusals_keep_native_causes_and_default_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut project = Project::new(32, 32, 30, 60).unwrap();
        let mut layer = Layer::solid(91, "private fixture name", [10.; 2], [0.; 3], [1.; 4]);
        layer.content = Content::Video {
            video: VideoClip::new(82),
        };
        project.layers.push(layer);
        let before = project.clone();
        let model_error = project.timeline_layers(7.).unwrap_err();
        let motion_core::Error::Timeline { object, source } = &model_error else {
            panic!("{model_error}")
        };
        assert_eq!(*object, 91);
        assert!(matches!(source.as_ref(), motion_core::Error::Invalid(_)));
        assert_eq!(project, before);

        // This second refusal goes through the real registered host state handler.
        // Session's public frame can be changed without changing the validated model.
        let healthy = Project::new(32, 32, 30, 60).unwrap();
        std::fs::create_dir_all(root.join("project")).unwrap();
        let id = motion_host::session::open(
            root.join("project"),
            healthy,
            motion_desktop_media::platform(),
        )
        .unwrap();
        let original_state = motion_host::ops::project::state(id).unwrap();
        motion_host::session::with_session(id, |s| {
            s.frame = f64::NAN;
            Ok(())
        })
        .unwrap();
        let state_error = motion_host::ops::project::state(id).unwrap_err();
        assert!(matches!(
            &state_error,
            motion_host::Error::Model(motion_core::Error::Invalid(_))
        ));
        motion_host::session::with_session(id, |s| {
            assert!(s.frame.is_nan());
            s.frame = 0.;
            Ok(())
        })
        .unwrap();

        let mut scalar = Track::constant(1.);
        scalar.axes = Some(Box::new(AxisTracks {
            x: Track::constant(1.),
            y: Track::constant(2.),
            z: Track::constant(3.),
        }));
        let scalar_error = scalar.timeline(0).unwrap_err();
        assert!(matches!(scalar_error, motion_core::Error::Invalid(_)));
        let errors = [
            motion_host::Error::from(model_error),
            state_error,
            motion_host::Error::from(scalar_error),
        ];
        let mut causes = Vec::new();
        for (request, error) in (511..514).zip(errors) {
            let original = error.to_string();
            let (reply, receive) = channel::<Result<Value, String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_timeline_query",
                started: Instant::now(),
            }));
            respond(reply, Err(error));
            CURRENT_COMMAND.set(None);
            assert_eq!(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err(),
                original
            );
            causes.push(original);
        }
        let recovered = motion_host::ops::project::state(id).unwrap();
        for key in [
            "project",
            "timeline_layers",
            "timeline_camera",
            "revision",
            "history",
            "frame",
            "canUndo",
            "canRedo",
        ] {
            assert_eq!(recovered[key], original_state[key], "{key}");
        }
        motion_host::session::close(id);
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
    assert!(file.contains("logging initialized level=INFO"));
    let causes: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.path().join("primary-causes.json")).unwrap())
            .unwrap();
    assert_eq!(causes.len(), 3);
    for (request, cause) in (511..514).zip(causes) {
        let marker = format!("error #{request} test_timeline_query ");
        for text in [&file, &stderr] {
            assert_eq!(text.matches(&marker).count(), 1);
            assert!(text
                .lines()
                .find(|line| line.contains(&marker))
                .unwrap()
                .contains(&cause));
        }
        assert!(!stdout.contains(&cause));
    }
    for text in [&file, &stderr] {
        assert_eq!(
            text.lines().filter(|line| line.contains("ERROR")).count(),
            3
        );
        assert!(!text.contains("private fixture name"));
    }
    assert!(!stderr.contains("logging initialized"));
    for marker in [
        "logging initialized",
        "test_timeline_query",
        "INFO",
        "ERROR",
    ] {
        assert!(!stdout.contains(marker));
    }
}
