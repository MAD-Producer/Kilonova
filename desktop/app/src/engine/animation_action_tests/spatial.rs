use super::*;

#[test]
fn spatial_key_ids_reach_the_worker_and_rejections_keep_default_error_channels_and_state() {
    use motion_model::keyframe_clipboard::spatial::SpatialKeyEdit;
    const NAME: &str="engine::animation_action_tests::spatial::spatial_key_ids_reach_the_worker_and_rejections_keep_default_error_channels_and_state";
    const CHILD: &str = "MOTION_SPATIAL_KEYFRAME_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let mut project = Project::new(640, 360, 30, 120).unwrap();
        let mut layer = Layer::solid(7, "Private spatial layer", [32.; 2], [0.; 3], [1.; 4]);
        for (frame, x) in [(0, 0.), (4, 1.), (10, 7.)] {
            layer
                .transform
                .position
                .upsert(frame, [x, 0., 0.], Ease::Linear)
                .unwrap();
        }
        project.layers.push(layer);
        let directory = root.join("project");
        motion_core::storage::save(&directory, &project).unwrap();
        let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
        let apply = |state: &Value, edit| {
            engine.call(|reply| Command::EditorAction {
                source: crate::layer_clipboard::KeyframeSource::of(state),
                commands: vec![motion_core::Command::SpatialKeyframes {
                    target: KeyframeTarget::Property {
                        object: 7,
                        property: Property::Position,
                        axis: None,
                    },
                    edit,
                }],
                reply,
            })
        };
        let original = engine.state().unwrap();
        let enabled = apply(&original, SpatialKeyEdit::Enable { frames: vec![4] }).unwrap();
        let after_enable = engine.state().unwrap();
        assert_eq!(enabled["project"], after_enable["project"]);
        let saved = &enabled["project"]["layers"][0]["transform"]["position"];
        assert!(saved.get("keys").is_none());
        assert_eq!(saved["spatial_animation"]["keys"][1]["id"], 2);
        assert_eq!(saved["spatial_animation"]["keys"][1]["fixed"], Value::Null);
        let error = apply(
            &after_enable,
            SpatialKeyEdit::Time {
                key: 999,
                time: 1.25,
            },
        )
        .unwrap_err();
        assert!(
            error.contains("operation=spatial_keyframes") && error.contains("object=7"),
            "{error}"
        );
        assert!(
            error.contains("phase=time key=999")
                && error.contains("selected spatial key identity is missing"),
            "{error}"
        );
        let after = engine.state().unwrap();
        for field in ["project", "revision", "history"] {
            assert_eq!(after[field], after_enable[field], "{field}");
        }
        let move_error = apply(&after, SpatialKeyEdit::Move { keys: vec![2, 999], delta: 1.25 }).unwrap_err();
        assert!(move_error.contains("operation=spatial_keyframes")
            && move_error.contains("phase=move delta=1.25")
            && move_error.contains("selected spatial key identity is missing"), "{move_error}");
        let after_failed_move = engine.state().unwrap();
        for field in ["project", "revision", "history"] {
            assert_eq!(after_failed_move[field], after_enable[field], "failed group move {field}");
        }
        let moved = apply(&after, SpatialKeyEdit::Time { key: 2, time: 1.25 }).unwrap();
        assert_eq!(
            moved["project"]["layers"][0]["transform"]["position"]["spatial_animation"]["keys"][1]
                ["fixed"],
            1.25
        );
        assert_eq!(
            engine.history(0).unwrap()["project"],
            after_enable["project"]
        );
        assert_eq!(engine.history(0).unwrap()["project"], original["project"]);
        std::fs::write(root.join("response.json"),serde_json::to_vec_pretty(&json!({"enabled":enabled["project"],"moved":moved["project"],"rejection":error,"groupMoveRejection":move_error,"unchanged":after["project"]==after_enable["project"],"groupMoveUnchanged":after_failed_move["project"]==after_enable["project"]})).unwrap()).unwrap();
        engine.request_shutdown().unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_SPATIAL_KEYFRAME_ERROR_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let cause = "operation=spatial_keyframes";
    assert_eq!(
        file.lines()
            .filter(|line| line.contains(" ERROR ") && line.contains(cause) && line.contains("phase=time key=999"))
            .count(),
        1
    );
    assert_eq!(file.lines().filter(|line| line.contains(" ERROR ") && line.contains(cause) && line.contains("phase=move delta=1.25")).count(), 1);
    assert_eq!(file.lines().filter(|line| line.contains(" ERROR ") && line.contains(cause)).count(), 2);
    assert!(stderr.contains(cause));
    assert!(stderr.contains("phase=time key=999") && stderr.contains("phase=move delta=1.25"));
    assert!(file.lines().any(|line| line.contains(" INFO ")
        && line.contains("start #")
        && line.ends_with(" editor_action")));
    assert!(!stdout.contains(cause) && !stdout.contains("motion_studio::"));
    for text in [&*file, &*stderr, &*stdout] {
        assert!(!text.contains("Private spatial layer"));
    }
}
