use motion_ui::{
    input::Modifiers,
    text_clipboard::TextClipboard,
    widgets::TextEditor,
    Input, Key,
};
use std::sync::Arc;

fn type_text(editor: &mut TextEditor, text: &str) {
    editor.input(&Input {
        text: text.into(),
        ..Default::default()
    });
}

fn control(editor: &mut TextEditor, key: Key, clipboard: &Arc<TextClipboard>) {
    editor.input(&Input {
        keys: vec![(key, Modifiers { control: true, ..Default::default() })],
        text_clipboard: Some(clipboard.clone()),
        ..Default::default()
    });
}

#[test]
fn typing_into_an_empty_selected_field_appends_and_undo_restores_the_selection() {
    let clipboard = Arc::new(TextClipboard::default());
    let mut editor = TextEditor::default();
    editor.begin("");
    type_text(&mut editor, "圆");
    assert!(!editor.is_all_selected());
    assert_eq!(editor.selected_text(), None);
    type_text(&mut editor, "形");
    assert_eq!(editor.text, "圆形");
    control(&mut editor, Key::Z, &clipboard);
    assert_eq!(editor.text, "圆");
    control(&mut editor, Key::Z, &clipboard);
    assert_eq!(editor.text, "");
    assert!(editor.is_all_selected());
    control(&mut editor, Key::Y, &clipboard);
    assert_eq!(editor.text, "圆");
    assert!(!editor.is_all_selected());
    type_text(&mut editor, "👩🏽‍🚀");
    assert_eq!(editor.text, "圆👩🏽‍🚀");
}

#[test]
fn native_clipboard_paste_consumes_an_empty_select_all_state() {
    let clipboard = Arc::new(TextClipboard::default());
    clipboard.store("圆").unwrap();
    let mut editor = TextEditor::default();
    editor.begin("");
    control(&mut editor, Key::V, &clipboard);
    assert!(!editor.is_all_selected());
    control(&mut editor, Key::V, &clipboard);
    assert_eq!(editor.text, "圆圆");
    control(&mut editor, Key::A, &clipboard);
    type_text(&mut editor, "新");
    assert_eq!(editor.text, "新");
    type_text(&mut editor, "值");
    assert_eq!(editor.text, "新值");
}

#[test]
fn multiline_typing_after_selecting_an_empty_draft_keeps_both_inputs() {
    let clipboard = Arc::new(TextClipboard::default());
    let mut editor = TextEditor::default();
    editor.begin_multiline("", 1024);
    control(&mut editor, Key::A, &clipboard);
    type_text(&mut editor, "首行\n");
    assert!(!editor.is_all_selected());
    type_text(&mut editor, "次行");
    assert_eq!(editor.text, "首行\n次行");
}
