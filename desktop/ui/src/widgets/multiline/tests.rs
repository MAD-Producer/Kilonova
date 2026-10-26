use super::*;
use crate::{input::Modifiers, text_clipboard::TextClipboard, widgets::TextCommand, Input};
use std::sync::Arc;
fn key(editor: &mut TextEditor, key: Key, control: bool, shift: bool) -> Option<TextEvent> {
    editor.input(&Input {
        keys: vec![(
            key,
            Modifiers {
                control,
                shift,
                alt: false,
            },
        )],
        ..Default::default()
    })
}

#[test]
fn multiline_enter_selection_navigation_ime_and_draft_history_preserve_graphemes() {
    let mut editor = TextEditor::default();
    editor.begin_multiline("你好e\u{301}\n12345\nx", 8192);
    key(&mut editor, Key::Home, true, false);
    key(&mut editor, Key::Right, false, true);
    key(&mut editor, Key::Right, false, true);
    assert_eq!(editor.selected_text(), Some("你好"));
    editor.input(&Input {
        text: "替换".into(),
        ..Default::default()
    });
    assert_eq!(editor.text, "替换e\u{301}\n12345\nx");
    key(&mut editor, Key::End, false, false);
    assert_eq!(editor.caret_line_column(), (1, 4));
    key(&mut editor, Key::Down, false, false);
    assert_eq!(editor.caret_line_column(), (2, 4));
    key(&mut editor, Key::Down, false, false);
    assert_eq!(editor.caret_line_column(), (3, 2));
    key(&mut editor, Key::Up, false, false);
    assert_eq!(editor.caret_line_column(), (2, 4));
    assert_eq!(key(&mut editor, Key::Enter, false, false), None);
    assert_eq!(editor.text, "替换e\u{301}\n123\n45\nx");
    editor.input(&Input {
        ime_consumed: true,
        keys: vec![(
            Key::Enter,
            Modifiers {
                control: true,
                ..Default::default()
            },
        )],
        text: "中文".into(),
        ..Default::default()
    });
    assert_eq!(editor.text, "替换e\u{301}\n123\n中文45\nx");
    assert_eq!(
        editor.input(&Input {
            focus_lost: true,
            ..Default::default()
        }),
        None
    );
    assert!(editor.active());
    key(&mut editor, Key::Z, true, false);
    assert_eq!(editor.text, "替换e\u{301}\n123\n45\nx");
    assert_eq!(
        key(&mut editor, Key::Enter, true, false),
        Some(TextEvent::Commit(editor.text.clone()))
    );
}

#[test]
fn native_text_commands_preserve_partial_selection_newlines_sessions_and_failed_pastes() {
    let clipboard = Arc::new(TextClipboard::default());
    let mut editor = TextEditor::default();
    let original = "a👩🏽‍🚀\n二e\u{301}z";
    editor.begin_multiline(original, 64);
    let start = "a".len();
    let end = "a👩🏽‍🚀\n二".len();
    editor.select_bytes(start, end);
    let session = editor.session().unwrap();
    assert!(editor.command_enabled(TextCommand::Copy));
    editor.command(TextCommand::Copy, Some(&clipboard));
    assert_eq!(&*clipboard.read().unwrap().unwrap(), "👩🏽‍🚀\n二");
    editor.command(TextCommand::Cut, Some(&clipboard));
    assert_eq!(editor.text, "ae\u{301}z");
    editor.command(TextCommand::Undo, None);
    assert_eq!(editor.text, original);
    assert_eq!(editor.selected_text(), Some("👩🏽‍🚀\n二"));
    editor.command(TextCommand::Paste, Some(&clipboard));
    assert_eq!(editor.text, original);
    clipboard.store(&"x".repeat(65)).unwrap();
    let before = editor.text.clone();
    editor.command(TextCommand::Paste, Some(&clipboard));
    assert_eq!(editor.text, before);
    assert!(editor.error.as_ref().unwrap().contains("64"));
    assert!(editor.owns_session(&session));
    editor.cancel();
    assert!(!editor.owns_session(&session));
    editor.begin("single line");
    clipboard.store("one\ntwo\tthree").unwrap();
    editor.command(TextCommand::Paste, Some(&clipboard));
    assert_eq!(editor.text, "onetwothree");
    assert!(matches!(
        key(&mut editor, Key::Enter, false, false),
        Some(TextEvent::Commit(_))
    ));
}

#[test]
fn multiline_pointer_selection_stops_on_release_and_scroll_reveals_the_caret() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut editor = TextEditor::default();
    editor.begin_multiline("alpha\nbeta\ngamma\ndelta\nepsilon", 8192);
    let mut view = TextViewport::default();
    let area = Rect::new(0., 0., 250., 75.);
    editor.select_bytes(0, 0);
    let draw =
        |editor: &mut TextEditor, view: &mut TextViewport, atlas: &mut TextAtlas, input: Input| {
            let mut list = PaintList::default();
            editor.multiline_view(view, &mut list, atlas, area, &input);
            assert!(!list.shapes.is_empty());
        };
    draw(
        &mut editor,
        &mut view,
        &mut atlas,
        Input {
            mouse: [45., 10.],
            pressed: Some((MouseButton::Left, [45., 10.])),
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    draw(
        &mut editor,
        &mut view,
        &mut atlas,
        Input {
            mouse: [75., 33.],
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    assert!(editor.selected_text().is_some());
    draw(
        &mut editor,
        &mut view,
        &mut atlas,
        Input {
            mouse: [75., 33.],
            released: Some((MouseButton::Left, [75., 33.])),
            ..Default::default()
        },
    );
    let selection = editor.selection_bytes();
    draw(
        &mut editor,
        &mut view,
        &mut atlas,
        Input {
            mouse: [180., 60.],
            ..Default::default()
        },
    );
    assert_eq!(editor.selection_bytes(), selection);
    key(&mut editor, Key::End, true, false);
    draw(&mut editor, &mut view, &mut atlas, Input::default());
    assert!(view.scroll[1] > 0.);
}
