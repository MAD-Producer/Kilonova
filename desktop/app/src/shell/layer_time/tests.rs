use super::*;
use motion_core::{
    layer_time::{LayerTimeAction, TimeStretch},
    Layer, Project,
};

#[test]
fn actual_time_failures_use_default_error_channels_and_stale_actions_cancel_without_partial_history(
) {
    const NAME:&str="shell::layer_time::tests::actual_time_failures_use_default_error_channels_and_stale_actions_cancel_without_partial_history";
    const CHILD: &str = "MOTION_LAYER_TIME_EDITOR_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _log = crate::logging::init(root.join("logs")).unwrap();
        let mut p = Project::new(64, 64, 30, 120).unwrap();
        for id in [7, 8] {
            p.layers
                .push(Layer::solid(id, "Time", [24.; 2], [0.; 3], [1.; 4]));
        }
        p.layers[1].locked = true;
        let directory = root.join("project");
        motion_core::storage::save(&directory, &p).unwrap();
        let engine =
            engine::Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
        let before = engine.state().unwrap();
        let apply = |s: &Value, commands| {
            engine.call(|reply| engine::Command::EditorAction {
                source: crate::layer_clipboard::KeyframeSource::of(s),
                commands,
                reply,
            })
        };
        let reverse = |object| Command::LayerTime {
            object,
            action: LayerTimeAction::Reverse {},
        };
        let error = apply(&before, vec![reverse(7), reverse(8)]).unwrap_err();
        assert!(error.contains("locked"), "{error}");
        let after = engine.state().unwrap();
        for field in ["project", "revision", "history"] {
            assert_eq!(after[field], before[field]);
        }
        let error = apply(
            &before,
            vec![Command::LayerTime {
                object: 7,
                action: LayerTimeAction::Stretch {
                    stretch: TimeStretch::new(1000, 1).unwrap(),
                    anchor_frame: 0.,
                    resize_clip: true,
                },
            }],
        )
        .unwrap_err();
        assert!(error.contains("clip interval"), "{error}");
        let after = engine.state().unwrap();
        for field in ["project", "revision", "history"] {
            assert_eq!(after[field], before[field]);
        }
        let changed = apply(&before, vec![reverse(7)]).unwrap();
        assert_eq!(changed["history"]["undoEntries"], 1);
        let stale = apply(&before, vec![reverse(7)]).unwrap();
        assert_eq!(stale["editorAction"]["cancelled"], true);
        assert_eq!(stale["editorAction"]["operation"], "layer_time");
        for field in ["project", "revision", "history"] {
            assert_eq!(stale[field], changed[field]);
        }
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LAYER_TIME_EDITOR_ERROR_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
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
    for cause in ["locked", "clip interval"] {
        let line = file
            .lines()
            .find(|l| l.contains(" ERROR ") && l.contains(cause) && l.contains("edit"))
            .unwrap_or_else(|| panic!("missing {cause}: {file}"));
        let request = line
            .split_once("error #")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap();
        assert!(
            file.lines()
                .any(|l| l.contains(&format!("start #{request}")) && l.contains("edit")),
            "{line}"
        );
        assert!(stderr.contains(cause), "{stderr}");
    }
    assert!(!String::from_utf8_lossy(&output.stdout).contains("motion_studio::"));
}
