use super::*;
use crate::node_editor::tests::{frame, simple};
use motion_ui::{input::Modifiers, text_clipboard::TextClipboard};
use std::sync::Arc;

fn key(cache: &Arc<TextClipboard>, key: Key, control: bool) -> Input {
    Input {
        text_clipboard: Some(cache.clone()),
        keys: vec![(
            key,
            Modifiers {
                control,
                ..Default::default()
            },
        )],
        ..Default::default()
    }
}
fn field(binding: Binding, text: String) -> Field {
    Field {
        binding,
        cursor: text.len(),
        text,
        replace: true,
        editor: Default::default(),
        error: None,
    }
}

#[test]
fn clipboard_node_field_history_stays_local_then_saves_and_undoes_once() {
    let mut editor = simple();
    editor.focus_panel();
    editor.select(Some("number"), false);
    let cache = Arc::new(TextClipboard::default());
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, key(&cache, Key::C, true));
    assert!(editor.clipboard.is_some());
    let before = editor.document.clone();
    let revision = editor.revision;
    editor.field = Some(field(Binding::GraphName(true), before.name.clone()));
    frame(&mut editor, &mut atlas, key(&cache, Key::X, true));
    assert_eq!(&*cache.read().unwrap().unwrap(), before.name);
    assert_eq!(editor.field.as_ref().unwrap().text, "");
    frame(&mut editor, &mut atlas, key(&cache, Key::Z, true));
    assert_eq!(editor.field.as_ref().unwrap().text, before.name);
    cache.store("节点名称 👩🏽‍🚀").unwrap();
    frame(&mut editor, &mut atlas, key(&cache, Key::V, true));
    frame(&mut editor, &mut atlas, key(&cache, Key::Z, true));
    assert_eq!(editor.field.as_ref().unwrap().text, before.name);
    frame(&mut editor, &mut atlas, key(&cache, Key::Y, true));
    assert_eq!(editor.field.as_ref().unwrap().text, "节点名称 👩🏽‍🚀");
    assert_eq!(editor.document, before);
    assert_eq!(editor.revision, revision);
    assert!(editor.undo.is_empty());
    frame(&mut editor, &mut atlas, key(&cache, Key::Enter, false));
    assert!(editor.field.is_none());
    assert_eq!(editor.document.name, "节点名称 👩🏽‍🚀");
    assert_eq!(editor.undo.len(), 1);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("field.msgraph");
    editor.snapshot().save(&path).unwrap();
    assert_eq!(GraphDocument::load(&path).unwrap(), editor.document);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    frame(&mut editor, &mut atlas, key(&cache, Key::V, true));
    assert_eq!(editor.document.nodes.len(), before.nodes.len() + 1);
    assert_eq!(&*cache.read().unwrap().unwrap(), "节点名称 👩🏽‍🚀");
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}

#[test]
fn clipboard_vector_field_keeps_validation_and_ime_out_of_graph_history() {
    let mut editor = simple();
    let id = editor
        .add_node(
            NodeOp::Math {
                operation: MathOp::Add,
                value_type: ValueType::Vec2,
            },
            [100., 80.],
        )
        .unwrap();
    editor.undo.clear();
    editor.focus_panel();
    let before = editor.document.clone();
    editor.field = Some(field(
        Binding::InputValue(id.clone(), "a".into()),
        "0, 0".into(),
    ));
    let cache = Arc::new(TextClipboard::default());
    cache.store("1, 2, 3").unwrap();
    editor.field_input(&key(&cache, Key::V, true));
    editor.field_input(&key(&cache, Key::Enter, false));
    assert!(editor.field.as_ref().unwrap().error.is_some());
    assert_eq!(editor.document, before);
    assert!(editor.undo.is_empty());
    editor.field_input(&key(&cache, Key::Z, true));
    assert_eq!(editor.field.as_ref().unwrap().text, "0, 0");
    assert!(editor.field.as_ref().unwrap().error.is_none());
    cache.store("12.34567, -8.12345").unwrap();
    editor.field_input(&key(&cache, Key::V, true));
    let mut ime = key(&cache, Key::Enter, false);
    ime.composing = true;
    editor.field_input(&ime);
    assert!(editor.field.is_some() && editor.undo.is_empty());
    ime.composing = false;
    ime.ime_consumed = true;
    editor.field_input(&ime);
    assert!(editor.field.is_some() && editor.undo.is_empty());
    ime.ime_consumed = false;
    ime.key_repeats.push(true);
    editor.field_input(&ime);
    assert!(editor.field.is_some() && editor.undo.is_empty());
    editor.field_input(&key(&cache, Key::Enter, false));
    assert_eq!(
        editor.input_default(&id, "a").unwrap(),
        &InputValue::Vec2([12.34567, -8.12345])
    );
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}

#[test]
fn clipboard_node_field_failure_preserves_selection_and_grapheme_navigation() {
    let mut editor = simple();
    let before = editor.document.clone();
    let original = "字 e\u{301}👩🏽‍🚀";
    editor.field = Some(field(Binding::GraphName(true), original.into()));
    let cache = Arc::new(TextClipboard::default());
    let mut missing = key(&cache, Key::X, true);
    missing.text_clipboard = None;
    editor.field_input(&missing);
    let draft = editor.field.as_ref().unwrap();
    assert_eq!(draft.text, original);
    assert!(draft.replace && draft.editor.error.is_some());
    editor.field_input(&key(&cache, Key::End, false));
    editor.field_input(&key(&cache, Key::Backspace, false));
    let draft = editor.field.as_ref().unwrap();
    assert_eq!(draft.text, "字 e\u{301}");
    assert_eq!(draft.cursor, draft.text.len());
    editor.field_input(&key(&cache, Key::Backspace, false));
    assert_eq!(editor.field.as_ref().unwrap().text, "字 ");
    editor.field_input(&key(&cache, Key::Z, true));
    assert_eq!(editor.field.as_ref().unwrap().text, "字 e\u{301}");
    editor.field_input(&Input {
        focus_lost: true,
        ..Default::default()
    });
    assert!(editor.field.is_none());
    assert_eq!(editor.document, before);
    assert!(editor.undo.is_empty());
}
