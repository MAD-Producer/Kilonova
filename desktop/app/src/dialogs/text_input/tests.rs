use super::*;
use motion_ui::text_clipboard::TextClipboard;
use std::sync::Arc;

fn key(cache: &Arc<TextClipboard>, key: Key, control: bool) -> Input {
    Input {
        text_clipboard: Some(cache.clone()),
        keys: vec![(
            key,
            motion_ui::input::Modifiers {
                control,
                ..Default::default()
            },
        )],
        ..Default::default()
    }
}
fn draw(
    dialog: &mut Dialog,
    atlas: &mut TextAtlas,
    input: &Input,
    locale: Locale,
) -> Option<Event> {
    dialog.render(
        &mut PaintList::default(),
        atlas,
        [1280., 800.],
        input,
        locale,
    )
}

#[test]
fn clipboard_form_edits_keep_local_history_and_emit_only_the_final_validated_event() {
    for locale in [Locale::En, Locale::Zh] {
        let state = serde_json::json!({"project":{"name":"Original", "width":200,"height":100,"fps":30,"frames":300}});
        for mut dialog in [
            Dialog::text(vec![]),
            Dialog::settings(&state),
            Dialog::new_composition(200, 100),
        ] {
            let mut atlas = TextAtlas::from_system(13.).unwrap();
            let cache = Arc::new(TextClipboard::default());
            let original = dialog.fields[0].clone();
            assert!(draw(&mut dialog, &mut atlas, &key(&cache, Key::X, true), locale).is_none());
            assert_eq!(dialog.fields[0], "");
            assert_eq!(&*cache.read().unwrap().unwrap(), original);
            assert!(draw(&mut dialog, &mut atlas, &key(&cache, Key::Z, true), locale).is_none());
            assert_eq!(dialog.fields[0], original);
            cache.store("新名称 👩🏽‍🚀").unwrap();
            assert!(draw(&mut dialog, &mut atlas, &key(&cache, Key::V, true), locale).is_none());
            assert_eq!(dialog.fields[0], "新名称 👩🏽‍🚀");
            assert!(draw(&mut dialog, &mut atlas, &key(&cache, Key::Z, true), locale).is_none());
            assert_eq!(dialog.fields[0], original);
            assert!(draw(&mut dialog, &mut atlas, &key(&cache, Key::Y, true), locale).is_none());
            assert_eq!(dialog.fields[0], "新名称 👩🏽‍🚀");
            let event = draw(
                &mut dialog,
                &mut atlas,
                &key(&cache, Key::Enter, false),
                locale,
            )
            .unwrap();
            match event {
                Event::Text {
                    text, size, width, ..
                } => {
                    assert_eq!(text, "新名称 👩🏽‍🚀");
                    assert_eq!((size, width), (64., 1024));
                }
                Event::Settings(value) | Event::CreateComposition(value) => {
                    assert_eq!(value["name"], "新名称 👩🏽‍🚀");
                    assert_eq!(value["width"], 200);
                    assert_eq!(value["height"], 100);
                }
                _ => panic!("unexpected dialog event"),
            }
        }
    }
}

#[test]
fn clipboard_composition_width_undo_recomputes_locked_height_and_tab_keeps_field_ownership() {
    let state = serde_json::json!({"project":{"name":"Original", "width":200,"height":100,"fps":30,"frames":300}});
    let mut dialog = Dialog::settings(&state);
    dialog.aspect_locked = true;
    dialog.aspect = 2.;
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let cache = Arc::new(TextClipboard::default());
    cache.store("400").unwrap();
    assert!(draw(
        &mut dialog,
        &mut atlas,
        &key(&cache, Key::Tab, false),
        Locale::En
    )
    .is_none());
    assert_eq!(dialog.focus, 1);
    assert!(draw(
        &mut dialog,
        &mut atlas,
        &key(&cache, Key::V, true),
        Locale::En
    )
    .is_none());
    assert_eq!(
        (&dialog.fields[1], &dialog.fields[2]),
        (&"400".to_string(), &"200".to_string())
    );
    assert!(draw(
        &mut dialog,
        &mut atlas,
        &key(&cache, Key::Z, true),
        Locale::En
    )
    .is_none());
    assert_eq!(
        (&dialog.fields[1], &dialog.fields[2]),
        (&"200".to_string(), &"100".to_string())
    );
    let mut tab = key(&cache, Key::Tab, false);
    tab.key_repeats.push(true);
    assert!(draw(&mut dialog, &mut atlas, &tab, Locale::En).is_none());
    assert_eq!(dialog.focus, 1);
    tab.key_repeats[0] = false;
    tab.ime_consumed = true;
    assert!(draw(&mut dialog, &mut atlas, &tab, Locale::En).is_none());
    assert_eq!(dialog.focus, 1);
    tab.ime_consumed = false;
    tab.modifiers.shift = true;
    tab.keys[0].1.shift = true;
    assert!(draw(&mut dialog, &mut atlas, &tab, Locale::En).is_none());
    assert_eq!(dialog.focus, 0);
    assert!(draw(
        &mut dialog,
        &mut atlas,
        &key(&cache, Key::Y, true),
        Locale::En
    )
    .is_none());
    assert_eq!(dialog.fields[0], "Original");
}

#[test]
fn clipboard_dialogs_keep_ime_confirmation_and_popup_inputs_out_of_apply() {
    let cache = Arc::new(TextClipboard::default());
    let mut dialog = Dialog::text(vec![]);
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let mut ime = key(&cache, Key::Enter, false);
    ime.ime_consumed = true;
    ime.text = "中文 👩🏽‍🚀".into();
    assert!(draw(&mut dialog, &mut atlas, &ime, Locale::Zh).is_none());
    assert_eq!(dialog.fields[0], "中文 👩🏽‍🚀");
    let mut repeat = key(&cache, Key::Enter, false);
    repeat.key_repeats.push(true);
    assert!(draw(&mut dialog, &mut atlas, &repeat, Locale::Zh).is_none());
    dialog.font_open = true;
    cache.store("should not enter a field").unwrap();
    assert!(draw(
        &mut dialog,
        &mut atlas,
        &key(&cache, Key::V, true),
        Locale::Zh
    )
    .is_none());
    assert_eq!(dialog.fields[0], "中文 👩🏽‍🚀");
    dialog.font_open = false;
    let mut cancel = key(&cache, Key::Escape, false);
    cancel.composing = true;
    assert!(draw(&mut dialog, &mut atlas, &cancel, Locale::Zh).is_none());
    cancel.composing = false;
    assert!(matches!(
        draw(&mut dialog, &mut atlas, &cancel, Locale::Zh),
        Some(Event::Cancelled)
    ));
}
