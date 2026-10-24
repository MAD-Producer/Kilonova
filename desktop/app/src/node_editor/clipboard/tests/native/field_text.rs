//! Production graph input on both native surfaces; field selection is fixture setup.
use super::*;
use motion_ui::text_clipboard::TextClipboard;
use std::path::Path;

pub(super) fn run(
    events: &mut EventLoop<()>,
    engine: &crate::engine::Engine,
    editor: &mut NodeEditor,
    atlas: &mut TextAtlas,
    window: u64,
    size: [f32; 2],
    scale: f32,
    root: &Path,
    language: Language,
    suffix: &str,
) {
    let cache = Arc::new(TextClipboard::default());
    cache.store("剪贴板 Text").unwrap();
    let input = |key: Key, control: bool| Input {
        text_clipboard: Some(cache.clone()),
        keys: vec![(
            key,
            Modifiers {
                control,
                ..Default::default()
            },
        )],
        ..Default::default()
    };
    let before = editor.document.clone();
    let clipboard = editor.clipboard.as_ref().unwrap().document.clone();
    let undo = editor.undo.len();
    editor.inspector_open = true;
    editor.select(None, false);
    editor.field = Some(Field {
        binding: Binding::GraphName(true),
        text: before.name.clone(),
        cursor: before.name.len(),
        replace: true,
        editor: Default::default(),
        error: None,
    });
    draw(editor, atlas, size, &input(Key::V, true), language);
    assert_eq!(editor.field.as_ref().unwrap().text, "剪贴板 Text");
    draw(editor, atlas, size, &input(Key::Z, true), language);
    assert_eq!(editor.field.as_ref().unwrap().text, before.name);
    draw(editor, atlas, size, &input(Key::Y, true), language);
    assert_eq!(editor.undo.len(), undo);
    assert_eq!(editor.document, before);
    capture(
        events,
        engine,
        editor,
        atlas,
        window,
        size,
        scale,
        root.join(format!("text-field-{suffix}.png")),
        language,
    );
    draw(editor, atlas, size, &input(Key::Enter, false), language);
    assert!(editor.field.is_none());
    assert_eq!(editor.document.name, "剪贴板 Text");
    assert_eq!(editor.undo.len(), undo + 1);
    let path = root.join(format!("text-field-{suffix}.msgraph"));
    editor.snapshot().save(&path).unwrap();
    assert_eq!(GraphDocument::load(&path).unwrap(), editor.document);
    assert_eq!(editor.clipboard.as_ref().unwrap().document, clipboard);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    editor.inspector_open = false;
}
