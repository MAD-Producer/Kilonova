use motion_ui::{
    widgets::{Button, NumberEditor, NumberEvent, NumberOptions, TextEditor, TextEvent},
    Input, Key, MouseButton, PaintList, Rect, TextAtlas,
};

fn key(key: Key) -> Input {
    Input { keys: vec![(key, Default::default())], ..Default::default() }
}
#[test]
fn text_commit_keeps_ime_text_without_triggering_enter_and_allows_repeat_backspace() {
    for composing in [false, true] {
        let mut editor = TextEditor::default();
        editor.begin("original");
        let input = Input { composing, ime_consumed: !composing, text: "输入".into(), ..key(Key::Enter) };
        assert!(editor.input(&input).is_none());
        assert_eq!(editor.text, "输入");
        assert!(matches!(editor.input(&key(Key::Enter)), Some(TextEvent::Commit(_))));
    }
    let mut editor = TextEditor::default();
    editor.begin("abcd");
    editor.input(&key(Key::End));
    let repeated = Input { key_repeats: vec![true], ..key(Key::Backspace) };
    editor.input(&repeated);
    assert_eq!(editor.text, "abc");
}
#[test]
fn keyboard_button_and_global_chords_ignore_ime_and_repeat() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let area = Rect::new(0., 0., 120., 24.);
    for key_code in [Key::Enter, Key::Space] {
        for blocked in [
            Input { composing: true, ..key(key_code) },
            Input { ime_consumed: true, ..key(key_code) },
            Input { key_repeats: vec![true], ..key(key_code) },
        ] {
            let mut button = Button::default();
            assert!(!button.draw(&mut PaintList::default(), &mut atlas, area, "test", &blocked, true).clicked);
            assert!(!motion_ui::ui::matches(&blocked, key_code, false));
        }
        assert!(Button::default().draw(&mut PaintList::default(), &mut atlas, area, "test", &key(key_code), true).clicked);
    }
}
#[test]
fn invalid_numeric_configuration_cancels_an_open_drag_with_the_original_value() {
    let area = Rect::new(0., 0., 120., 24.);
    let mut editor = NumberEditor::default();
    editor.update(7., NumberOptions::default(), &Input {
        pressed: Some((MouseButton::Left, [10., 10.])), mouse: [10., 10.],
        held: Some(MouseButton::Left), ..Default::default()
    }, area);
    let events = editor.update(7., NumberOptions::default(), &Input {
        mouse: [50., 10.], held: Some(MouseButton::Left), ..Default::default()
    }, area);
    assert!(events.iter().any(|event| matches!(event, NumberEvent::Begin { value } if *value == 7.)));
    assert!(editor.interacting());
    let events = editor.update(f64::NAN, NumberOptions::default(), &Input::default(), area);
    assert!(matches!(events.as_slice(), [NumberEvent::Cancel { value }] if *value == 7.));
    assert!(!editor.interacting());
    assert!(editor.text.error.is_some());
}
#[test]
fn ellipsis_never_overflows_a_space_smaller_than_the_ellipsis() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let ellipsis = atlas.measure("…");
    for width in [0., ellipsis * 0.5, ellipsis, ellipsis * 2., 40.] {
        let text = atlas.ellipsize("Effect Controls", width);
        assert!(atlas.measure(&text) <= width, "{width}: {text}");
    }
    assert!(atlas.ellipsize("text", f32::NAN).is_empty());
    assert_eq!(atlas.ellipsize("text", f32::INFINITY), "text");
}
