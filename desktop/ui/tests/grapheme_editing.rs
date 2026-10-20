use motion_ui::{
    widgets::{TextEditor, TextEvent},
    Input, Key,
};

// Independently authored examples: combining marks, Hangul Jamo, Indic conjunct,
// modifier/ZWJ emoji, regional indicators, family emoji and keycap sequence.
const CLUSTERS: &[&str] = &[
    "e\u{301}",
    "a\u{308}\u{301}",
    "\u{1100}\u{1161}\u{11a8}",
    "\u{915}\u{94d}\u{937}",
    "👩🏽\u{200d}🚀",
    "🇯🇵",
    "👨\u{200d}👩\u{200d}👧",
    "1\u{fe0f}\u{20e3}",
];

fn key(editor: &mut TextEditor, key: Key) -> Option<TextEvent> {
    editor.input(&Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    })
}
fn type_text(editor: &mut TextEditor, text: &str) {
    assert!(editor
        .input(&Input {
            text: text.into(),
            ..Default::default()
        })
        .is_none());
}
fn start(text: &str) -> TextEditor {
    let mut editor = TextEditor::default();
    editor.begin(text);
    editor
}

#[test]
fn backspace_removes_an_entire_extended_grapheme() {
    for cluster in CLUSTERS {
        let mut editor = start(&format!("A{cluster}B"));
        key(&mut editor, Key::End);
        key(&mut editor, Key::Left);
        key(&mut editor, Key::Backspace);
        assert_eq!(editor.text, "AB", "cluster={cluster:?}");
        type_text(&mut editor, "X");
        assert_eq!(editor.text, "AXB");
    }
}

#[test]
fn delete_removes_an_entire_extended_grapheme() {
    for cluster in CLUSTERS {
        let mut editor = start(&format!("A{cluster}B"));
        key(&mut editor, Key::Home);
        key(&mut editor, Key::Right);
        key(&mut editor, Key::Delete);
        assert_eq!(editor.text, "AB", "cluster={cluster:?}");
        type_text(&mut editor, "X");
        assert_eq!(editor.text, "AXB");
    }
}

#[test]
fn right_navigation_keeps_insertions_outside_graphemes() {
    for cluster in CLUSTERS {
        let mut editor = start(&format!("A{cluster}B"));
        key(&mut editor, Key::Home);
        key(&mut editor, Key::Right);
        key(&mut editor, Key::Right);
        type_text(&mut editor, "|");
        assert_eq!(editor.text, format!("A{cluster}|B"));
    }
}

#[test]
fn left_navigation_keeps_insertions_outside_graphemes() {
    for cluster in CLUSTERS {
        let mut editor = start(&format!("A{cluster}B"));
        key(&mut editor, Key::End);
        key(&mut editor, Key::Left);
        key(&mut editor, Key::Left);
        type_text(&mut editor, "|");
        assert_eq!(editor.text, format!("A|{cluster}B"));
    }
}

#[test]
fn inserted_combining_marks_keep_the_caret_before_the_following_letter() {
    let mut editor = start("ab");
    key(&mut editor, Key::Home);
    key(&mut editor, Key::Right);
    type_text(&mut editor, "\u{301}");
    type_text(&mut editor, "X");
    assert_eq!(editor.text, "a\u{301}Xb");
}

#[test]
fn insertion_that_merges_with_the_next_grapheme_places_the_caret_after_it() {
    for (initial, insertion, expected) in [
        ("👩🚀Z", "\u{200d}", "👩\u{200d}🚀XZ"),
        ("🇦🇧🇨Z", "🇩", "🇦🇧🇩🇨XZ"),
    ] {
        let mut editor = start(initial);
        key(&mut editor, Key::Home);
        key(&mut editor, Key::Right);
        type_text(&mut editor, insertion);
        type_text(&mut editor, "X");
        assert_eq!(editor.text, expected);
    }
}

#[test]
fn deleting_a_separator_resegments_the_joined_neighbors() {
    let mut editor = start("🇦x🇧Z");
    key(&mut editor, Key::Home);
    key(&mut editor, Key::Right);
    key(&mut editor, Key::Delete);
    type_text(&mut editor, "X");
    assert_eq!(editor.text, "🇦🇧XZ");
}

#[test]
fn backspacing_a_separator_resegments_the_joined_neighbors() {
    let mut editor = start("🇦x🇧Z");
    key(&mut editor, Key::Home);
    key(&mut editor, Key::Right);
    key(&mut editor, Key::Right);
    key(&mut editor, Key::Backspace);
    type_text(&mut editor, "X");
    assert_eq!(editor.text, "🇦🇧XZ");
}

#[test]
fn ascii_selection_commit_cancel_and_repeat_keep_their_existing_contracts() {
    let mut editor = start("original");
    type_text(&mut editor, "abcd");
    key(&mut editor, Key::End);
    let repeated = Input {
        keys: vec![(Key::Backspace, Default::default())],
        key_repeats: vec![true],
        ..Default::default()
    };
    assert!(editor.input(&repeated).is_none());
    assert_eq!(editor.text, "abc");
    assert_eq!(
        key(&mut editor, Key::Enter),
        Some(TextEvent::Commit("abc".into()))
    );
    assert_eq!(key(&mut editor, Key::Escape), Some(TextEvent::Cancel));
    assert!(!editor.active());
}
