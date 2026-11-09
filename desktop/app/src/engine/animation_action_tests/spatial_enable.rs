//! Real serial worker: local conversion is atomic and failures use default sinks.
use super::*;

fn initial(reverse: bool) -> Project {
    let mut p = project(reverse);
    let layer = &mut p.layers[0];
    layer.transform.position = motion_core::Track::constant([0.; 3]);
    for (frame, x) in [(-12, 0.), (-9, 1.), (-6, 7.), (0, 10.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 0., 0.], Ease::Linear)
            .unwrap();
    }
    let mut second = layer.clone();
    second.id = 8;
    p.layers.push(second);
    p
}
fn command(object: u64, frames: Vec<i32>) -> motion_core::Command {
    motion_core::Command::SpatialKeyframes {
        target: KeyframeTarget::Property {
            object,
            property: Property::Position,
            axis: None,
        },
        edit: SpatialKeyEdit::EnableLocal { frames },
    }
}

#[test]
fn local_roving_worker_rolls_back_complete_batch_and_reports_original_failure() {
    const NAME: &str = "engine::animation_action_tests::spatial_time::enable::local_roving_worker_rolls_back_complete_batch_and_reports_original_failure";
    const CHILD: &str = "MOTION_LOCAL_ROVING_WORKER_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        for reverse in [false, true] {
            let tag = if reverse { "reverse" } else { "forward" };
            let project = initial(reverse);
            let path = root.join(tag);
            motion_core::storage::save(&path, &project).unwrap();
            let engine = Engine::start(path, motion_desktop_media::platform(), 8 << 30).unwrap();
            let apply = |state: &Value, commands| {
                engine.call(|reply| Command::EditorAction {
                    source: crate::layer_clipboard::KeyframeSource::of(state),
                    commands,
                    reply,
                })
            };
            let original = engine.state().unwrap();
            let cause =
                apply(&original, vec![command(7, vec![-9]), command(8, vec![-8])]).unwrap_err();
            assert!(
                cause.to_string().contains("operation=spatial_keyframes")
                    && cause.to_string().contains("object=8")
                    && cause.to_string().contains("phase=enable_local frames=[-8]"),
                "{cause}"
            );
            let rejected = engine.state().unwrap();
            for field in ["project", "revision", "history"] {
                assert_eq!(rejected[field], original[field], "{field}");
            }
            let converted =
                apply(&original, vec![command(7, vec![-9]), command(8, vec![-6])]).unwrap();
            let mut expected = project.clone();
            expected.layers[0]
                .transform
                .position
                .enable_roving(&[-9])
                .unwrap();
            expected.layers[1]
                .transform
                .position
                .enable_roving(&[-6])
                .unwrap();
            assert_eq!(
                converted["project"],
                serde_json::to_value(&expected).unwrap()
            );
            assert_eq!(converted["history"]["undoEntries"], 1);
            assert_eq!(engine.history(0).unwrap()["project"], original["project"]);
            assert_eq!(engine.history(1).unwrap()["project"], converted["project"]);
            std::fs::write(root.join(format!("{tag}.json")), serde_json::to_vec_pretty(&json!({"before":original,"failure":cause,"rejected":rejected,"converted":converted})).unwrap()).unwrap();
            engine.request_shutdown().unwrap();
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LOCAL_ROVING_WORKER_ARTIFACTS")
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
                .filter(|l| l.contains("ERROR")
                    && l.contains("phase=enable_local frames=[-8]")
                    && l.contains("object=8"))
                .count(),
            2,
            "{text}"
        );
    }
    assert!(file
        .lines()
        .any(|l| l.contains(" INFO ") && l.contains("start #") && l.ends_with(" editor_action")));
    assert!(!stdout.contains("motion_studio::") && !stdout.contains("operation=spatial_keyframes"));
    for text in [&*file, &*stderr, &*stdout] {
        assert!(!text.contains("Private mapped spatial layer"));
    }
}
