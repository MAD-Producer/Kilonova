use motion_ui::{
    text_clipboard::{TextClipboard, MAX_BYTES},
    widgets::{TextEditor, TextEvent},
    Input, Key,
};
use std::sync::Arc;

fn chord(cache: &Arc<TextClipboard>, key: Key, shift: bool) -> Input {
    Input {
        text_clipboard: Some(cache.clone()),
        keys: vec![(
            key,
            motion_ui::input::Modifiers {
                control: true,
                shift,
                alt: false,
            },
        )],
        ..Default::default()
    }
}
fn begin(text: &str) -> TextEditor {
    let mut editor = TextEditor::default();
    editor.begin(text);
    editor
}

#[test]
fn resumed_search_appends_at_grapheme_end_and_clones_own_their_history() {
    let cache = Arc::new(TextClipboard::default());
    let mut source = TextEditor::default();
    source.begin_at_end("搜索 👩🏽‍🚀");
    source.input(&Input {
        text: "追加".into(),
        ..Default::default()
    });
    assert_eq!(source.text, "搜索 👩🏽‍🚀追加");
    let mut cloned = source.clone();
    source.input(&chord(&cache, Key::Z, false));
    assert_eq!(source.text, "搜索 👩🏽‍🚀");
    assert_eq!(cloned.text, "搜索 👩🏽‍🚀追加");
    cloned.input(&chord(&cache, Key::Z, false));
    assert_eq!(cloned.text, source.text);
    cloned.input(&chord(&cache, Key::Z, true));
    assert_eq!(cloned.text, "搜索 👩🏽‍🚀追加");
    assert_eq!(source.text, "搜索 👩🏽‍🚀");
    let debug = format!("{source:?} {cloned:?}");
    assert!(debug.contains("<private>"));
    assert!(!debug.contains("搜索"));
}

#[test]
fn independent_editors_share_owned_text_and_cut_paste_each_undo_once() {
    let cache = Arc::new(TextClipboard::default());
    let source_text = "e\u{301} 中文 👩🏽‍🚀";
    let mut source = begin(source_text);
    assert!(source.input(&chord(&cache, Key::X, false)).is_none());
    assert_eq!(source.text, "");
    source.input(&chord(&cache, Key::Z, false));
    assert_eq!(source.text, source_text);
    source.input(&chord(&cache, Key::Z, true));
    assert_eq!(source.text, "");
    source.cancel();
    drop(source);
    let mut target = begin("previous");
    target.input(&chord(&cache, Key::V, false));
    assert_eq!(target.text, source_text);
    target.input(&chord(&cache, Key::Z, false));
    assert_eq!(target.text, "previous");
    target.input(&chord(&cache, Key::Y, false));
    assert_eq!(target.text, source_text);
    target.input(&chord(&cache, Key::V, false));
    assert_eq!(target.text, source_text.repeat(2));
    target.input(&chord(&cache, Key::Z, false));
    assert_eq!(target.text, source_text);
    assert_eq!(&*cache.read().unwrap().unwrap(), source_text);
    assert_eq!(
        target.input(&Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        }),
        Some(TextEvent::Commit(source_text.into()))
    );
}

#[test]
fn paste_uses_grapheme_cursor_and_local_history_does_not_merge_following_typing() {
    let cache = Arc::new(TextClipboard::default());
    cache.store("\u{301}\n").unwrap();
    let mut target = begin("ab");
    target.input(&Input {
        keys: vec![
            (Key::Home, Default::default()),
            (Key::Right, Default::default()),
        ],
        ..Default::default()
    });
    target.input(&chord(&cache, Key::V, false));
    assert_eq!(target.text, "a\u{301}b");
    target.input(&Input {
        text: "X".into(),
        ..Default::default()
    });
    assert_eq!(target.text, "a\u{301}Xb");
    target.input(&chord(&cache, Key::Z, false));
    assert_eq!(target.text, "a\u{301}b");
    target.input(&chord(&cache, Key::Z, false));
    assert_eq!(target.text, "ab");
    target.input(&chord(&cache, Key::Y, false));
    assert_eq!(target.text, "a\u{301}b");
}

#[test]
fn repeated_ime_lost_focus_and_unselected_text_do_not_mutate_cache_or_draft() {
    let cache = Arc::new(TextClipboard::default());
    cache.store("cached").unwrap();
    for (composing, ime_consumed, repeat) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        for key in [Key::C, Key::X, Key::V] {
            let mut target = begin("draft");
            let mut input = chord(&cache, key, false);
            input.composing = composing;
            input.ime_consumed = ime_consumed;
            input.key_repeats = vec![repeat];
            target.input(&input);
            assert_eq!(target.text, "draft");
            assert_eq!(&*cache.read().unwrap().unwrap(), "cached");
        }
    }
    let mut target = begin("draft");
    target.input(&Input {
        keys: vec![(Key::End, Default::default())],
        ..Default::default()
    });
    target.input(&chord(&cache, Key::X, false));
    assert_eq!(target.text, "draft");
    assert_eq!(&*cache.read().unwrap().unwrap(), "cached");
    let mut input = chord(&cache, Key::V, false);
    input.focus_lost = true;
    assert_eq!(target.input(&input), Some(TextEvent::Cancel));
    assert_eq!(target.text, "draft");
    assert_eq!(&*cache.read().unwrap().unwrap(), "cached");
}

#[test]
fn unavailable_or_oversized_operations_preserve_selection_draft_and_last_cache() {
    let cache = Arc::new(TextClipboard::default());
    cache.store("kept").unwrap();
    let mut target = begin("draft");
    let mut input = chord(&cache, Key::X, false);
    input.text_clipboard = None;
    target.input(&input);
    assert_eq!(target.text, "draft");
    assert!(target.error.as_deref().unwrap().contains("unavailable"));
    target.input(&chord(&cache, Key::V, false));
    assert_eq!(target.text, "kept", "failed cut must retain selection");
    target.begin(&"x".repeat(MAX_BYTES + 1));
    target.input(&chord(&cache, Key::X, false));
    assert_eq!(target.text.len(), MAX_BYTES + 1);
    assert!(target.error.as_deref().unwrap().contains("16 MiB"));
    assert_eq!(&*cache.read().unwrap().unwrap(), "kept");
}

#[test]
fn borrowed_filtered_and_owned_input_keep_the_same_service_without_copying_text() {
    use motion_ui::InputSource;
    let cache = Arc::new(TextClipboard::default());
    cache.store("shared").unwrap();
    let input = chord(&cache, Key::V, false);
    let view = input.input_view();
    let owned = view.to_owned();
    assert!(Arc::ptr_eq(owned.text_clipboard.as_ref().unwrap(), &cache));
    let mut target = begin("draft");
    target.input(&view);
    assert_eq!(target.text, "shared");
    let debug = format!("{:?}", cache);
    assert!(!debug.contains("shared"));
}
