use super::*;
use crate::panels::{Action, Panel};
use motion_ui::input::{Key as InputKey, Modifiers};

#[test]
fn authored_nudges_use_one_or_ten_frames_and_one_atomic_history_entry() {
    for locale in [crate::i18n::Locale::Zh, crate::i18n::Locale::En] {
        for owner in [None, Some(Panel::Timeline)] {
            for (key, shift, delta) in [
                (InputKey::Left, false, -1),
                (InputKey::Right, false, 1),
                (InputKey::Left, true, -10),
                (InputKey::Right, true, 10),
            ] {
                let mut engine = fixture();
                let before = engine.snapshot();
                let mut panels = panels(&engine);
                panels.locale = locale;
                // The +10 case deliberately moves into another selected source
                // key, proving the UI and model agree about overlap ordering.
                panels
                    .selected_keys
                    .apply(
                        &state(&engine),
                        [
                            super::key(7, None, 10),
                            super::key(7, None, 20),
                            super::key(9, Some(Axis::Y), 43),
                            super::key(9, Some(Axis::Y), 53),
                        ],
                        Mode::Replace,
                    )
                    .unwrap();
                let selected = panels.selected_keys.selections();
                let input_state = state(&engine);
                let actions = route(
                    &mut panels,
                    &input_state,
                    key,
                    Modifiers {
                        alt: true,
                        shift,
                        ..Default::default()
                    },
                    owner,
                );
                let [Action::EditorAction { source, commands }] = actions.as_slice() else {
                    panic!("{locale:?} owner={owner:?} delta={delta}: {actions:?}");
                };
                assert_eq!(
                    source,
                    &crate::layer_clipboard::KeyframeSource::of(&input_state)
                );
                let [Command::TransformKeyframes {
                    selections,
                    delta_frame,
                    value_offset,
                    copy,
                }] = commands.as_slice()
                else {
                    panic!("{commands:?}");
                };
                assert_eq!(selections, &selected);
                assert_eq!(*delta_frame, delta);
                assert!(value_offset.is_none() && !copy);
                engine.apply_batch(commands.clone()).unwrap();
                for selection in selected {
                    for frame in selection.frames {
                        let moved = (i64::from(frame) + i64::from(delta)) as u32;
                        assert!(present(&engine, &selection.target, moved));
                        assert!(panels.selected_keys.contains(&Key {
                            target: selection.target.clone(),
                            frame: moved
                        }));
                    }
                }
                assert_eq!(
                    panels.selected_keys.status(&state(&engine)),
                    Status::Current
                );
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
                assert!(!engine.can_undo());
                engine.redo().unwrap();
                assert_ne!(engine.project(), &before);
            }
        }
    }
}

#[test]
fn rejected_nudge_keeps_selection_data_and_history_and_never_seeks_or_moves_layers() {
    for reason in [
        "collision",
        "bounds",
        "locked",
        "stale",
        "observing",
        "empty",
    ] {
        let engine = fixture();
        let mut input_state = state(&engine);
        let mut panels = panels(&engine);
        panels
            .selected_keys
            .apply(&input_state, [key(7, None, 10)], Mode::Replace)
            .unwrap();
        let mut shift = true;
        match reason {
            "collision" => {} // 10 + 10 lands on the unselected key at 20.
            "bounds" => {
                panels
                    .selected_keys
                    .apply(&input_state, [key(9, Some(Axis::Y), 53)], Mode::Replace)
                    .unwrap();
                // Exact same authored source but a shortened composition.
                input_state["project"]["frames"] = json!(60);
            }
            "locked" => input_state["project"]["layers"][0]["locked"] = json!(true),
            "stale" => input_state["document_generation"] = json!(999),
            "observing" => input_state["observing"] = json!(true),
            "empty" => {
                panels.selected_keys.clear(&input_state);
                shift = false;
            }
            _ => unreachable!(),
        }
        let selected = panels.selected_keys.selections();
        let actions = route(
            &mut panels,
            &input_state,
            InputKey::Right,
            Modifiers {
                alt: true,
                shift,
                ..Default::default()
            },
            None,
        );
        assert!(actions.is_empty(), "{reason}: {actions:?}");
        assert_eq!(panels.selected_keys.selections(), selected, "{reason}");
        assert!(panels.message.is_some(), "{reason}");
        assert!(!engine.can_undo());
    }
}

#[test]
fn nudge_bindings_repeat_but_respect_text_ime_and_other_panel_focus() {
    use crate::shortcuts::{resolve, Command as Shortcut, Scope};
    use motion_ui::input::Input;
    let modifiers = Modifiers {
        alt: true,
        ..Default::default()
    };
    assert_eq!(
        resolve(Scope::Animation, InputKey::Left, modifiers, true),
        Some(Shortcut::NudgeKeyEarlier)
    );
    assert_eq!(
        resolve(
            Scope::Animation,
            InputKey::Right,
            Modifiers {
                shift: true,
                ..modifiers
            },
            true
        ),
        Some(Shortcut::NudgeKeysLaterTen)
    );
    for reason in ["text", "ime", "composition", "effects"] {
        let engine = fixture();
        let mut panels = panels(&engine);
        let input_state = state(&engine);
        let selected = panels.selected_keys.selections();
        let mut input = Input {
            keys: vec![(InputKey::Right, modifiers)],
            ..Default::default()
        };
        match reason {
            "text" => panels.timeline.timeline_search = true,
            "ime" => input.composing = true,
            "composition" => panels.focused = Some(Panel::Composition),
            "effects" => panels.focused = Some(Panel::Effects),
            _ => unreachable!(),
        }
        let mut actions = vec![];
        crate::panels::keyboard::route(
            &input,
            &mut panels,
            &input_state,
            false,
            None,
            &mut actions,
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::EditorAction { .. })),
            "{reason}: {actions:?}"
        );
        assert_eq!(panels.selected_keys.selections(), selected, "{reason}");
    }
}

#[test]
fn remapped_nudge_repeats_through_keyboard_routing_and_disables_its_old_default() {
    use crate::shortcuts::{Command as Shortcut, Scope};
    use motion_ui::input::Input;
    use motion_ui::input::KeyChord;
    let engine = fixture();
    let input_state = state(&engine);
    let mut panels = panels(&engine);
    let chord = Modifiers {
        control: true,
        alt: true,
        shift: true,
    };
    std::sync::Arc::make_mut(&mut panels.shortcut_bindings)
        .bind(
            Scope::Animation,
            Shortcut::NudgeKeyLater,
            Some(KeyChord {
                key: InputKey::N,
                modifiers: chord,
            }),
        )
        .unwrap();
    let mut actions = vec![];
    crate::panels::keyboard::route(
        &Input {
            keys: vec![(InputKey::N, chord)],
            key_repeats: vec![true],
            ..Default::default()
        },
        &mut panels,
        &input_state,
        false,
        None,
        &mut actions,
    );
    assert!(
        matches!(actions.as_slice(),[Action::EditorAction {commands,..}] if matches!(commands.as_slice(),[Command::TransformKeyframes {delta_frame:1,..}]))
    );
    assert_eq!(
        panels.shortcut_bindings.resolve(
            Scope::Animation,
            InputKey::Right,
            Modifiers {
                alt: true,
                ..Default::default()
            },
            false
        ),
        None
    );
}

#[test]
fn malformed_drag_occupancy_reports_default_channels_and_recovery_can_nudge_again() {
    const NAME:&str="panels::key_selection::tests::transforms::malformed_drag_occupancy_reports_default_channels_and_recovery_can_nudge_again";
    const CHILD: &str = "MOTION_KEY_TRANSFORM_OCCUPANCY_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let mut engine = fixture();
        let mut panels = panels(&engine);
        let original = state(&engine);
        let before = engine.snapshot();
        let selected = panels.selected_keys.selections();
        let mut malformed = original.clone();
        // The selected key is present and searchable; a malformed unselected
        // tail reaches occupancy capture rather than the normal stale guard.
        malformed["project"]["layers"][0]["transform"]["position"]["keys"]
            .as_array_mut()
            .unwrap()
            .push(json!({"frame":"invalid","value":[0.,0.,0.],"ease":"linear"}));
        assert_eq!(panels.selected_keys.status(&malformed), Status::Current);
        let actions = route(
            &mut panels,
            &malformed,
            InputKey::Right,
            Modifiers {
                alt: true,
                ..Default::default()
            },
            None,
        );
        assert!(actions.is_empty());
        assert_eq!(panels.selected_keys.selections(), selected);
        assert!(panels
            .message
            .as_deref()
            .unwrap()
            .contains("key transform authored time is not an integer"));
        let actions = route(
            &mut panels,
            &original,
            InputKey::Right,
            Modifiers {
                alt: true,
                ..Default::default()
            },
            None,
        );
        let [Action::EditorAction { commands, .. }] = actions.as_slice() else {
            panic!("{actions:?}");
        };
        engine.apply_batch(commands.clone()).unwrap();
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        crate::logging::flush();
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_KEY_TRANSFORM_OCCUPANCY_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().into());
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
    let event = "key transform failed operation=prepare phase=occupancy";
    assert_eq!(file.lines().filter(|l| l.contains(event)).count(), 1);
    for output in [&file, stderr.as_ref()] {
        assert!(
            output.contains(event)
                && output.contains("key transform authored time is not an integer")
                && output.contains("revision=Some(0)")
                && output.contains("primary_object="),
            "{output}"
        );
    }
    assert!(!stdout.contains(event) && !stdout.contains("key transform authored time"));
}
