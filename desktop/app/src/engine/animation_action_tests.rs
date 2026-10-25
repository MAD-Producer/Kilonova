use super::{Command, Engine};
use motion_core::{Ease, Keyframe, KeyframeSelection, KeyframeTarget, Layer, Project, Property};
use serde_json::{json, Value};
use std::path::PathBuf;

fn edit(engine: &Engine, state: &Value, object: u64, frames: &[u32]) -> Result<Value, String> {
    let source = crate::layer_clipboard::KeyframeSource::of(state);
    let selections = vec![KeyframeSelection {
        target: KeyframeTarget::Property {
            object,
            property: Property::Position,
            axis: None,
        },
        frames: frames.to_vec(),
    }];
    engine.call(|reply| Command::EditorAction {
        source,
        commands: vec![motion_core::Command::KeyframeInterpolation {
            selections,
            ease: Ease::Hold,
        }],
        reply,
    })
}

#[test]
fn animation_actions_keep_default_error_sinks_and_cancel_superseded_document_work_without_editing()
{
    const NAME: &str = "engine::animation_action_tests::animation_actions_keep_default_error_sinks_and_cancel_superseded_document_work_without_editing";
    const CHILD: &str = "MOTION_ANIMATION_ACTION_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let mut project = Project::new(640, 360, 30, 120).unwrap();
        for id in [7, 8] {
            let mut layer = Layer::solid(id, "Animated", [32.; 2], [0.; 3], [1.; 4]);
            layer.transform.position.keys = [0, 10, 20]
                .into_iter()
                .map(|frame| Keyframe {
                    frame,
                    value: [frame as f32, 0., 0.],
                    ease: Ease::Linear,
                    curve: None,
                    spatial: None,
                })
                .collect();
            layer.locked = id == 8;
            project.layers.push(layer);
        }
        let directory = root.join("project");
        motion_core::storage::save(&directory, &project).unwrap();
        let engine = Engine::start(
            directory,
            motion_desktop_media::platform(),
            8 * 1024 * 1024 * 1024,
        )
        .unwrap();
        let before = engine.state().unwrap();
        for (object, frame, reason) in [
            (7, 99, "selected keyframe does not exist"),
            (999, 0, "object 999 does not exist"),
            (8, 0, "object 8 is locked"),
        ] {
            let error = edit(&engine, &before, object, &[frame]).unwrap_err();
            assert!(error.contains(reason), "{error}");
            assert!(error.contains("operation=interpolation"), "{error}");
            assert!(error.contains(&format!("object={object}")), "{error}");
            let after = engine.state().unwrap();
            assert_eq!(after["project"], before["project"]);
            assert_eq!(after["revision"], before["revision"]);
        }
        let applied = edit(&engine, &before, 7, &[0, 10, 20]).unwrap();
        assert!(
            applied["project"]["layers"][0]["transform"]["position"]["keys"]
                .as_array()
                .unwrap()
                .iter()
                .all(|key| key["ease"] == "hold")
        );
        let cancelled = edit(&engine, &before, 7, &[0]).unwrap();
        assert_eq!(
            cancelled["editorAction"],
            json!({"cancelled":true,"reason":"context_changed"})
        );
        assert_eq!(cancelled["project"], applied["project"]);
        assert_eq!(cancelled["revision"], applied["revision"]);
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        let second = root.join("second");
        let mut other = project.clone();
        other.name = "Other document with reused IDs".into();
        motion_core::storage::save(&second, &other).unwrap();
        let ticket = engine.call(Command::ProjectTicket).unwrap();
        let changed = engine
            .call(|reply| Command::ChangeProject {
                target: crate::project_lifecycle::Target::Open(second),
                ticket,
                save: false,
                reply,
            })
            .unwrap();
        assert_eq!(
            changed["revision"], before["revision"],
            "the counter must collide to exercise generation/root checks"
        );
        let cancelled = edit(&engine, &before, 7, &[0]).unwrap();
        assert_eq!(cancelled["editorAction"]["cancelled"], true);
        assert_eq!(cancelled["project"], changed["project"]);
        let recovered = edit(&engine, &changed, 7, &[0, 10, 20]).unwrap();
        assert_ne!(recovered["project"], changed["project"]);
        assert_eq!(engine.history(0).unwrap()["project"], changed["project"]);
        engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_ANIMATION_ACTION_ARTIFACTS")
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    for reason in [
        "selected keyframe does not exist",
        "object 999 does not exist",
        "object 8 is locked",
    ] {
        assert!(
            file.lines()
                .any(|line| line.contains(" ERROR ") && line.contains(reason)),
            "{file}"
        );
        assert!(stderr.contains(reason), "{stderr}");
        assert!(!stdout.contains(reason), "{stdout}");
    }
    assert_eq!(
        file.lines()
            .filter(|line| line.contains(" ERROR ") && line.contains("editor_action"))
            .count(),
        3
    );
    assert!(!stderr.contains("context_changed"));
    assert!(!stdout.contains("context_changed"));
    assert!(file.lines().any(|line| line.contains(" INFO ")
        && line.contains("start #")
        && line.ends_with(" editor_action")));
    assert!(!stderr.contains("start #"));
    assert!(!stdout.contains("start #"));
}

#[test]
fn key_selection_edits_keep_original_failures_on_default_channels_and_recover_atomically() {
    const NAME: &str = "engine::animation_action_tests::key_selection_edits_keep_original_failures_on_default_channels_and_recover_atomically";
    const CHILD: &str = "MOTION_KEY_SELECTION_ACTION_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let mut project = Project::new(640, 360, 30, 120).unwrap();
        for id in [7, 8] {
            let mut layer = Layer::solid(id, "Keys", [32.; 2], [0.; 3], [1.; 4]);
            for frame in [0, 10, 20] {
                layer
                    .transform
                    .position
                    .upsert(frame, [frame as f32; 3], Ease::Linear)
                    .unwrap();
                layer
                    .transform
                    .opacity
                    .upsert(frame, 0.5, Ease::Linear)
                    .unwrap();
            }
            layer.locked = id == 8;
            project.layers.push(layer);
        }
        let directory = root.join("project");
        motion_core::storage::save(&directory, &project).unwrap();
        let engine = Engine::start(
            directory,
            motion_desktop_media::platform(),
            8 * 1024 * 1024 * 1024,
        )
        .unwrap();
        let selected = |object, frames: Vec<u32>| KeyframeSelection {
            target: KeyframeTarget::Property {
                object,
                property: Property::Position,
                axis: None,
            },
            frames,
        };
        let apply = |state: &Value, delete: bool, selections: Vec<KeyframeSelection>| {
            let source = crate::layer_clipboard::KeyframeSource::of(state);
            let command = if delete {
                motion_core::Command::DeleteKeyframes { selections }
            } else {
                motion_core::Command::EaseKeyframes {
                    selections,
                    incoming: true,
                    outgoing: true,
                }
            };
            engine.call(|reply| Command::EditorAction {
                source,
                commands: vec![command],
                reply,
            })
        };
        for delete in [false, true] {
            let before = engine.state().unwrap();
            let operation = if delete { "delete" } else { "quick_ease" };
            for (object, frame, reason) in [
                (7, 99, "selected keyframe does not exist"),
                (999, 0, "object 999 does not exist"),
                (8, 0, "object 8 is locked"),
            ] {
                // A valid, different channel comes first: duplicate addresses
                // cannot mask the original missing-key/locked-object failure.
                let first = KeyframeSelection {
                    target: KeyframeTarget::Property {
                        object: 7,
                        property: Property::Opacity,
                        axis: None,
                    },
                    frames: vec![10],
                };
                let selections = vec![first, selected(object, vec![frame])];
                let error = apply(&before, delete, selections).unwrap_err();
                assert!(error.contains(reason), "{error}");
                assert!(error.contains(&format!("operation={operation}")), "{error}");
                assert!(error.contains(&format!("object={object}")), "{error}");
                let after = engine.state().unwrap();
                assert_eq!(after["project"], before["project"]);
                assert_eq!(after["revision"], before["revision"]);
            }
            let applied = apply(&before, delete, vec![selected(7, vec![0, 20])]).unwrap();
            assert_ne!(applied["project"], before["project"]);
            assert_eq!(
                applied["project"]["layers"][1],
                before["project"]["layers"][1]
            );
            let cancelled = apply(&before, delete, vec![selected(7, vec![0])]).unwrap();
            assert_eq!(
                cancelled["editorAction"],
                json!({"cancelled":true,"reason":"context_changed"})
            );
            assert_eq!(cancelled["project"], applied["project"]);
            assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        }
        engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_KEY_SELECTION_ACTION_ARTIFACTS")
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    for operation in ["quick_ease", "delete"] {
        for reason in [
            "selected keyframe does not exist",
            "object 999 does not exist",
            "object 8 is locked",
        ] {
            assert!(
                file.lines().any(|line| line.contains(" ERROR ")
                    && line.contains(&format!("operation={operation}"))
                    && line.contains(reason)),
                "{file}"
            );
            assert!(
                stderr
                    .lines()
                    .any(|line| line.contains(&format!("operation={operation}"))
                        && line.contains(reason)),
                "{stderr}"
            );
            assert!(!stdout.contains(reason), "{stdout}");
        }
    }
    assert_eq!(
        file.lines()
            .filter(|line| line.contains(" ERROR ") && line.contains("editor_action"))
            .count(),
        6
    );
    assert!(file.lines().any(|line| line.contains(" INFO ")
        && line.contains("start #")
        && line.ends_with(" editor_action")));
    assert!(!stderr.contains("start #") && !stdout.contains("start #"));
    assert!(!stderr.contains("context_changed") && !stdout.contains("context_changed"));
}
