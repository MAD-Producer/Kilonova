use motion_ui::{
    text_clipboard::TextClipboard,
    widgets::{TextCommand as C, TextEditor},
};
use std::sync::Arc;

#[test]
fn menu_session_expires_on_cancel_restart_and_independent_clone() {
    let mut editor = TextEditor::default();
    assert!(editor.session().is_none());
    editor.begin("draft");
    let session = editor.session().unwrap();
    assert!(editor.owns_session(&session));
    let clone = editor.clone();
    assert!(!clone.owns_session(&session));
    editor.cancel();
    assert!(!editor.owns_session(&session));
    editor.begin("draft");
    assert!(!editor.owns_session(&session), "same text is a new draft");
}

#[test]
fn menu_cut_paste_and_history_share_failure_preservation_and_capabilities() {
    let cache = Arc::new(TextClipboard::default());
    let mut editor = TextEditor::default();
    editor.begin("original");
    assert!(!editor.command_enabled(C::Undo) && !editor.command_enabled(C::Redo));
    editor.command(C::Cut, None);
    assert_eq!(editor.text, "original");
    assert!(editor.is_all_selected() && editor.error.is_some());
    editor.command(C::Cut, Some(&cache));
    assert_eq!(editor.text, "");
    assert_eq!(&*cache.read().unwrap().unwrap(), "original");
    assert!(editor.command_enabled(C::Undo) && !editor.command_enabled(C::Copy));
    editor.command(C::Undo, Some(&cache));
    assert_eq!(editor.text, "original");
    assert!(editor.command_enabled(C::Redo));
    cache.store("粘贴 👩🏽‍🚀").unwrap();
    editor.command(C::Paste, Some(&cache));
    assert_eq!(editor.text, "粘贴 👩🏽‍🚀");
    assert!(!editor.command_enabled(C::Redo));
    editor.command(C::Undo, Some(&cache));
    assert_eq!(editor.text, "original");
    editor.command(C::Redo, Some(&cache));
    assert_eq!(editor.text, "粘贴 👩🏽‍🚀");
    editor.command(C::SelectAll, Some(&cache));
    editor.command(C::Copy, Some(&cache));
    assert_eq!(&*cache.read().unwrap().unwrap(), "粘贴 👩🏽‍🚀");
}
