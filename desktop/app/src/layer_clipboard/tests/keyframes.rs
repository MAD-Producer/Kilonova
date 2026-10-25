//! Exercise the real serialized edit, rollback and default logging boundary.
use motion_core::{Command, KeyframeClipboard, KeyframeSelection, KeyframeTarget, Property};
use std::{fs, path::PathBuf};

#[test]
fn keyframe_clipboard_failures_use_default_engine_error_channel_and_preserve_state() {
    const TEST: &str = "layer_clipboard::tests::keyframes::keyframe_clipboard_failures_use_default_engine_error_channel_and_preserve_state";
    const CHILD: &str = "MOTION_KEYFRAME_CLIPBOARD_CHANNEL_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        for id in [7, 9] {
            project.layers.push(motion_core::Layer::solid(
                id, "Layer", [32.; 2], [16.; 3], [1.; 4],
            ));
        }
        project.layers[0]
            .transform
            .position
            .set_animated(0, true)
            .unwrap();
        project.layers[0]
            .transform
            .position
            .set_at(15, [32.; 3])
            .unwrap();
        let clipboard = KeyframeClipboard::capture(
            &project,
            &[KeyframeSelection {
                target: KeyframeTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: None,
                },
                frames: vec![0, 15],
            }],
        )
        .unwrap();
        let source = root.join("project");
        motion_core::storage::save(&source, &project).unwrap();
        let engine = super::engine(&source);
        let paste = |property| Command::PasteKeyframes {
            clipboard: Box::new(clipboard.clone()),
            targets: vec![KeyframeTarget::Property {
                object: 9,
                property,
                axis: None,
            }],
            at_frame: Some(30),
        };
        engine
            .edit(serde_json::to_string(&paste(Property::Position)).unwrap())
            .unwrap();
        let before = engine.state().unwrap();
        for (command, cause) in [
            (
                paste(Property::Position),
                "keyframe destination is occupied",
            ),
            (
                paste(Property::Rotation),
                "clipboard source and destination properties differ",
            ),
        ] {
            let commands = vec![
                Command::Rename {
                    object: 9,
                    name: "must roll back".into(),
                },
                command,
            ];
            let error = engine
                .edit(serde_json::to_string(&commands).unwrap())
                .unwrap_err();
            assert!(error.contains(cause), "{error}");
            let after = engine.state().unwrap();
            assert_eq!(after["project"], before["project"]);
            assert_eq!(after["revision"], before["revision"]);
        }
        engine
            .edit(
                serde_json::to_string(&Command::SetVector {
                    object: 7,
                    property: Property::Position,
                    frame: 0,
                    value: [42.; 3],
                })
                .unwrap(),
            )
            .unwrap();
        let before_cut = engine.state().unwrap();
        let error = engine
            .edit(
                serde_json::to_string(&Command::RemoveKeyframes {
                    clipboard: Box::new(clipboard),
                })
                .unwrap(),
            )
            .unwrap_err();
        assert!(error.contains("clipboard cut source changed"), "{error}");
        let after_cut = engine.state().unwrap();
        assert_eq!(after_cut["project"], before_cut["project"]);
        assert_eq!(after_cut["revision"], before_cut["revision"]);
        drop(engine);
        crate::logging::flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_KEYFRAME_CLIPBOARD_CHANNEL_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(file.contains("logging initialized level=INFO"));
    assert!(file.contains("start #") && file.contains(" edit"));
    for cause in [
        "keyframe destination is occupied",
        "clipboard source and destination properties differ",
        "clipboard cut source changed",
    ] {
        assert!(file.contains(cause), "missing file cause: {cause}");
        assert!(stderr.contains(cause), "missing stderr cause: {cause}");
        assert!(!stdout.contains(cause), "stdout leaked failure: {cause}");
    }
    assert!(
        !stderr.contains("start #"),
        "healthy start must only enter the file log"
    );
    assert!(file.contains("phase=paste object=9 channel=Position/None"));
    assert!(file.contains("phase=cut object=7 channel=Position/None"));
}
