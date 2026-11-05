use motion_ui::{
    input::Modifiers,
    paint::PaintList,
    widgets::Button,
    Input, Key, Rect, TextAtlas,
};

fn clicked(atlas: &mut TextAtlas, input: &Input, enabled: bool) -> bool {
    Button::default()
        .draw(
            &mut PaintList::default(),
            atlas,
            Rect::new(0., 0., 100., 30.),
            "Run",
            input,
            enabled,
        )
        .clicked
}

#[test]
fn button_activation_reserves_modified_enter_and_space_for_shortcuts() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for key in [Key::Enter, Key::Space] {
        for bits in 0..8 {
            let modifiers = Modifiers {
                control: bits & 1 != 0,
                shift: bits & 2 != 0,
                alt: bits & 4 != 0,
            };
            let input = Input {
                keys: vec![(key, modifiers)],
                ..Default::default()
            };
            assert_eq!(
                clicked(&mut atlas, &input, true),
                bits == 0,
                "button activation key={key:?} modifiers={modifiers:?}"
            );
        }
    }
}

#[test]
fn stale_keyboard_input_cannot_activate_a_button_after_focus_loss() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for key in [Key::Enter, Key::Space] {
        let stale = Input {
            keys: vec![(key, Modifiers::default())],
            focus_lost: true,
            ..Default::default()
        };
        assert!(!clicked(&mut atlas, &stale, true), "stale key={key:?}");
        let recovered = Input { focus_lost: false, ..stale };
        assert!(clicked(&mut atlas, &recovered, true));
    }
}

#[test]
fn button_activation_keeps_repeat_ime_and_disabled_guards() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for key in [Key::Enter, Key::Space] {
        let plain = Input {
            keys: vec![(key, Modifiers::default())],
            ..Default::default()
        };
        assert!(clicked(&mut atlas, &plain, true));
        assert!(!clicked(&mut atlas, &plain, false));
        for guarded in [
            Input { key_repeats: vec![true], ..plain.clone() },
            Input { composing: true, ..plain.clone() },
            Input { ime_consumed: true, ..plain.clone() },
        ] {
            assert!(!clicked(&mut atlas, &guarded, true));
        }
        assert!(clicked(&mut atlas, &plain, true));
    }
}
