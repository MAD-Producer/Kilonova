use super::*;
use motion_core::{KeyframeSelection, KeyframeTarget, Property};

fn target(object: u64) -> KeyframeTarget {
    KeyframeTarget::Property {
        object,
        property: Property::Position,
        axis: None,
    }
}
fn document(root: &Path) {
    super::project(root, false);
    let mut project = motion_core::storage::load(root).unwrap();
    project.layers[0]
        .transform
        .position
        .set_animated(0, true)
        .unwrap();
    project.layers[0]
        .transform
        .position
        .set_at(5, [10.; 3])
        .unwrap();
    let mut destination = motion_core::Layer::solid(9, "Destination", [32.; 2], [32.; 3], [1.; 4]);
    destination.timeline = Some(motion_core::LayerTimeline {
        in_frame: 30,
        out_frame: 90,
        offset_frame: 30,
    });
    project.layers.push(destination);
    motion_core::storage::save(root, &project).unwrap();
}
fn action(intent: Intent, copy: bool) -> Action {
    Action::KeyframeClipboard {
        intent,
        selections: if copy {
            vec![KeyframeSelection {
                target: target(7),
                frames: vec![10, 15],
            }]
        } else {
            vec![]
        },
        targets: vec![target(9)],
    }
}

#[test]
fn keyframe_shell_paste_keeps_layer_selection_and_cut_uses_one_history_entry() {
    for locale in [Locale::En, Locale::Zh] {
        let root = tempfile::tempdir().unwrap();
        document(root.path());
        let mut shell = super::shell(root.path(), locale);
        shell.action(action(Intent::Copy, true), None).unwrap();
        wait(&mut shell);
        assert!(shell.layer_clipboard.keyframes_available());
        assert!(!shell.layer_clipboard.available());
        assert_eq!(
            shell.panels.message.as_deref(),
            Some(if locale == Locale::Zh {
                "已复制关键帧"
            } else {
                "Keyframes copied"
            })
        );
        shell.panels.select_only(Some(9));
        shell.state = shell.engine.seek(50.).unwrap();
        shell
            .action(action(Intent::Paste { in_place: false }, false), None)
            .unwrap();
        wait(&mut shell);
        assert_eq!(
            shell.panels.selection_ids(),
            [9],
            "key paste must not consume layer edit_result objects"
        );
        let pasted = shell.state["project"].clone();
        let keys = &pasted["layers"][1]["transform"]["position"]["keys"];
        assert_eq!(keys[0]["frame"], 20);
        assert_eq!(keys[1]["frame"], 25);
        assert_eq!(
            shell.panels.message.as_deref(),
            Some(if locale == Locale::Zh {
                "已粘贴关键帧"
            } else {
                "Keyframes pasted"
            })
        );
        shell.action(action(Intent::Cut, true), None).unwrap();
        wait(&mut shell);
        assert!(
            shell.state["project"]["layers"][0]["transform"]["position"]["keys"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(shell.panels.selection_ids(), [9]);
        assert_eq!(shell.engine.history(0).unwrap()["project"], pasted);
        shell.engine.save().unwrap();
        assert_eq!(
            serde_json::to_value(motion_core::storage::load(root.path()).unwrap()).unwrap(),
            pasted
        );
    }
}

#[test]
fn shell_keyframe_cancel_and_stale_commit_share_existing_action_barrier() {
    let root = tempfile::tempdir().unwrap();
    document(root.path());
    let mut shell = super::shell(root.path(), Locale::En);
    shell.playback = Some((Instant::now(), 0.));
    shell.panels.playing = true;
    shell
        .action(
            Action::KeyframeClipboard {
                intent: Intent::Cut,
                selections: vec![],
                targets: vec![],
            },
            None,
        )
        .unwrap();
    assert!(
        shell.playback.is_some() && shell.panels.playing,
        "empty selection is a normal no-op"
    );
    let before = shell.state["project"].clone();
    shell.action(action(Intent::Cut, true), None).unwrap();
    assert!(shell.playback.is_none());
    shell.actions.push_back(Action::Save); // hold the existing dispatch barrier
    until_phase(&mut shell, "ready");
    assert_eq!(shell.engine.state().unwrap()["project"], before);
    shell.actions.push_back(Action::CancelLayerClipboard);
    shell.poll_layer_clipboard();
    assert!(!shell.layer_clipboard.busy());
    assert!(shell.layer_clipboard.keyframes_available());
    assert_eq!(shell.engine.state().unwrap()["project"], before);
    shell.actions.clear();
    shell.action(action(Intent::Cut, true), None).unwrap();
    shell.actions.push_back(Action::Save);
    until_phase(&mut shell, "ready");
    shell.state = shell
        .engine
        .edit(
            serde_json::to_string(&motion_core::Command::Rename {
                object: 7,
                name: "Changed".into(),
            })
            .unwrap(),
        )
        .unwrap();
    let changed = shell.state["project"].clone();
    shell.actions.clear();
    wait(&mut shell);
    assert_eq!(
        shell.panels.message.as_deref(),
        Some("Project changed; clipboard operation was not applied")
    );
    assert_eq!(shell.state["project"], changed);
    assert_eq!(shell.engine.state().unwrap()["project"], changed);
}
