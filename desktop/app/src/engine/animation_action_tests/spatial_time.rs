use super::*;
use motion_core::{layer_time::*, LayerTimeline, SpatialKeyEdit};

fn project(reverse: bool) -> Project {
    let mut p = Project::new(64, 64, 30, 120).unwrap();
    let mut layer = Layer::solid(
        7,
        "Private mapped spatial layer",
        [32.; 2],
        [0.; 3],
        [1.; 4],
    );
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 120,
        offset_frame: 2,
    });
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if reverse { 0. } else { -12. },
        )
        .unwrap(),
        remap: None,
    });
    for (frame, x) in [(-12, 0.), (-9, 1.), (-6, 7.), (0, 10.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 0., 0.], Ease::Linear)
            .unwrap();
    }
    layer.transform.position.enable_roving(&[-9]).unwrap();
    p.layers.push(layer);
    p
}
fn edit(edit: SpatialKeyEdit) -> motion_core::Command {
    motion_core::Command::SpatialKeyframes {
        target: KeyframeTarget::Property {
            object: 7,
            property: Property::Position,
            axis: None,
        },
        edit,
    }
}

#[test]
fn mapped_spatial_worker_preserves_clocks_atomicity_and_default_error_outputs() {
    const NAME: &str = "engine::animation_action_tests::spatial_time::mapped_spatial_worker_preserves_clocks_atomicity_and_default_error_outputs";
    const CHILD: &str = "MOTION_MAPPED_SPATIAL_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        for reverse in [false, true] {
            let project = project(reverse);
            let directory = root.join(if reverse { "reverse" } else { "forward" });
            motion_core::storage::save(&directory, &project).unwrap();
            let engine =
                Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
            let apply = |state: &Value, commands| {
                engine.call(|reply| Command::EditorAction {
                    source: crate::layer_clipboard::KeyframeSource::of(state),
                    commands,
                    reply,
                })
            };
            let original = engine.state().unwrap();
            let moved = apply(
                &original,
                vec![edit(SpatialKeyEdit::Move {
                    keys: vec![1, 2, 3, 4],
                    delta: 1.125,
                })],
            )
            .unwrap();
            let mut expected = project.clone();
            expected.layers[0]
                .transform
                .position
                .spatial_animation
                .as_mut()
                .unwrap()
                .move_keys(&[1, 2, 3, 4], if reverse { -0.75 } else { 0.75 })
                .unwrap();
            assert_eq!(moved["project"], serde_json::to_value(&expected).unwrap());
            let frame = project.layers[0]
                .time_mapping
                .as_ref()
                .unwrap()
                .clock
                .composition_frame(-10.25, 2)
                .unwrap();
            let timed = apply(
                &moved,
                vec![edit(SpatialKeyEdit::Time {
                    key: 2,
                    time: frame,
                })],
            )
            .unwrap();
            expected.layers[0]
                .transform
                .position
                .spatial_animation
                .as_mut()
                .unwrap()
                .set_fixed_time(2, -10.25)
                .unwrap();
            assert_eq!(timed["project"], serde_json::to_value(&expected).unwrap());
            let before = engine.state().unwrap();
            let cause = apply(
                &before,
                vec![
                    edit(SpatialKeyEdit::Move {
                        keys: vec![1, 2, 3, 4],
                        delta: 1.125,
                    }),
                    edit(SpatialKeyEdit::Time {
                        key: 999,
                        time: frame,
                    }),
                ],
            )
            .unwrap_err();
            assert!(
                cause.contains("operation=spatial_keyframes")
                    && cause.contains("object=7")
                    && cause.contains("phase=time key=999")
                    && cause.contains("selected spatial key identity is missing"),
                "{cause}"
            );
            let after = engine.state().unwrap();
            for field in ["project", "revision", "history"] {
                assert_eq!(after[field], before[field], "{field}");
            }
            assert_eq!(engine.history(0).unwrap()["project"], moved["project"]);
            assert_eq!(engine.history(0).unwrap()["project"], original["project"]);
            assert_eq!(engine.history(1).unwrap()["project"], moved["project"]);
            assert_eq!(engine.history(1).unwrap()["project"], timed["project"]);
            std::fs::write(
                root.join(if reverse {
                    "reverse.json"
                } else {
                    "forward.json"
                }),
                serde_json::to_vec_pretty(&json!({"original":original["project"],
                    "moved":moved["project"], "timed":timed["project"], "failure":cause,
                    "unchanged":after["project"]==before["project"]}))
                .unwrap(),
            )
            .unwrap();
            engine.request_shutdown().unwrap();
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_MAPPED_SPATIAL_ERROR_ARTIFACTS")
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
    for text in [&*file, &*stderr] {
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("operation=spatial_keyframes")
                    && line.contains("phase=time key=999"))
                .count(),
            2,
            "{text}"
        );
        assert!(text.contains("selected spatial key identity is missing"));
    }
    assert_eq!(
        file.lines().filter(|line| line.contains(" ERROR ")).count(),
        2
    );
    assert_eq!(
        stderr.lines().filter(|line| line.contains("ERROR")).count(),
        2
    );
    assert!(file.lines().any(|line| line.contains(" INFO ")
        && line.contains("start #")
        && line.ends_with(" editor_action")));
    assert!(!stdout.contains("operation=spatial_keyframes") && !stdout.contains("motion_studio::"));
    for text in [&*file, &*stderr, &*stdout] {
        assert!(!text.contains("Private mapped spatial layer"));
    }
}
