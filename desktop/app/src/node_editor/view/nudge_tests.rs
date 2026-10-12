use super::*;
use motion_ui::input::Modifiers;

fn press(repeat: bool) -> Input {
    Input {
        keys: vec![(Key::Right, Modifiers::default())],
        key_repeats: vec![repeat],
        held_keys: vec![Key::Right],
        ..Default::default()
    }
}

#[test]
fn repeated_arrows_keep_earlier_history_and_undo_in_one_step() {
    let mut editor = NodeEditor::default();
    editor.focus_panel();
    for _ in 0..64 {
        editor.move_selected([0., 1.]);
    }
    let before = editor.document.clone();
    let semantic = editor.semantic_revision;
    let preview = editor.preview_revision;
    let mut actions = Vec::new();
    for index in 0..120 {
        editor.keyboard(&press(index > 0), &mut actions);
        assert_eq!(editor.undo.len(), 64);
        assert_eq!(editor.semantic_revision, semantic);
        assert_eq!(editor.preview_revision, preview);
    }
    let moved = editor.document.clone();
    editor.keyboard(
        &Input {
            released_keys: vec![Key::Right],
            ..Default::default()
        },
        &mut actions,
    );
    assert_eq!(editor.undo.len(), 64); // One new entry evicts only the oldest.
    assert!(editor.nudge_before.is_none());
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    assert!(editor.redo());
    assert_eq!(editor.document, moved);
    assert!(editor.undo());
    for _ in 0..63 {
        assert!(editor.undo());
    }
    assert!(!editor.undo());
}

#[test]
fn save_focus_loss_escape_and_undo_settle_pending_movement() {
    for end in ["save", "focus", "escape", "undo"] {
        let mut editor = NodeEditor::default();
        editor.focus_panel();
        let before = editor.document.clone();
        let mut actions = Vec::new();
        editor.keyboard(&press(false), &mut actions);
        editor.keyboard(&press(true), &mut actions);
        match end {
            "save" => {
                editor.request(Request::Save).unwrap();
            }
            "focus" => { editor.cancel_interaction(); },
            "escape" => editor.keyboard(
                &Input {
                    keys: vec![(Key::Escape, Modifiers::default())],
                    ..Default::default()
                },
                &mut actions,
            ),
            _ => {
                assert!(editor.undo());
            }
        }
        assert!(editor.nudge_before.is_none());
        if end == "undo" || end == "escape" {
            assert_eq!(editor.document, before);
        } else {
            assert!(editor.undo());
            assert_eq!(editor.document, before);
        }
    }
}

#[test]
fn release_and_new_press_in_one_frame_start_a_new_undo_step() {
    let mut editor = NodeEditor::default();
    editor.focus_panel();
    let mut actions = Vec::new();
    editor.keyboard(&press(false), &mut actions);
    let first = editor.document.clone();
    let mut again = press(false);
    again.released_keys.push(Key::Right);
    editor.keyboard(&again, &mut actions);
    editor.keyboard(&Input::default(), &mut actions);
    assert_eq!(editor.undo.len(), 2);
    assert!(editor.undo());
    assert_eq!(editor.document, first);
}
