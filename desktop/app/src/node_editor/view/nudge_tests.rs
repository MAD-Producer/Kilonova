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
fn remapped_movement_holds_repeat_until_release_and_undoes_in_one_step() {
    use crate::shortcuts::{bindings::Bindings,Command,Scope};
    let mut editor=NodeEditor::default();editor.focus_panel();
    let mut bindings=Bindings::default();
    bindings.bind(Scope::Nodes,Command::MoveRight,Some(motion_ui::input::KeyChord::new(Key::B,false,false,false))).unwrap();
    editor.shortcut_bindings=std::sync::Arc::new(bindings);
    let before=editor.document.clone();let history=editor.undo.len();let semantic=editor.semantic_revision;
    let mut actions=vec![];
    for repeat in [false,true,true,true] {
        editor.keyboard(&Input {keys:vec![(Key::B,Modifiers::default())],key_repeats:vec![repeat],held_keys:vec![Key::B],..Default::default()},&mut actions);
        assert_eq!(editor.undo.len(),history);
        assert!(editor.nudge_before.is_some());
    }
    assert_ne!(editor.document,before);
    editor.keyboard(&Input {released_keys:vec![Key::B],..Default::default()},&mut actions);
    let moved=editor.document.clone();
    assert_eq!(editor.undo.len(),history+1);assert_eq!(editor.semantic_revision,semantic);
    assert!(editor.undo());assert_eq!(editor.document,before);
    assert!(editor.redo());assert_eq!(editor.document,moved);
    editor.keyboard(&press(false),&mut actions);assert_eq!(editor.document,moved,"old arrow binding is disabled");
}

#[test]
fn unmodified_remapped_file_command_defers_to_node_search_text() {
    use crate::shortcuts::{bindings::Bindings,Command,Scope};
    let mut editor=NodeEditor::default();editor.focus_panel();
    let mut bindings=Bindings::default();
    bindings.bind(Scope::Nodes,Command::Save,Some(motion_ui::input::KeyChord::new(Key::Q,false,false,false))).unwrap();
    editor.shortcut_bindings=std::sync::Arc::new(bindings);editor.search_focus=true;
    let mut actions=vec![];
    editor.keyboard(&Input {keys:vec![(Key::Q,Modifiers::default())],text:"q".into(),..Default::default()},&mut actions);
    assert!(actions.is_empty(),"typing must not save a document");
    assert_eq!(editor.search,"q");
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
