use super as keyboard;
use crate::shortcuts::{
    bindings, effect_search_pressed, Command, Scope, EFFECT_SEARCH, EFFECT_SEARCH_ALT,
};
use crate::{
    i18n::Locale,
    panels::{self, Panels},
};
use motion_ui::{
    dock::Panel,
    input::{Input, Key, Modifiers},
};
use serde_json::Value;

#[test]
fn only_exact_search_chords_without_repeat_or_ime_are_recognized() {
    for key in [Key::Space, Key::Enter] {
        for mask in 0..8 {
            for repeat in [false, true] {
                for composing in [false, true] {
                    for ime_consumed in [false, true] {
                        let modifiers = Modifiers {
                            control: mask & 1 != 0,
                            shift: mask & 2 != 0,
                            alt: mask & 4 != 0,
                        };
                        let input = Input {
                            keys: vec![(key, modifiers)],
                            key_repeats: vec![repeat],
                            composing,
                            ime_consumed,
                            ..Default::default()
                        };
                        assert_eq!(effect_search_pressed(&input), key == Key::Space && modifiers.control && !modifiers.shift
                            && !repeat && !composing && !ime_consumed,
                            "{key:?} {modifiers:?} repeat={repeat} composing={composing} ime_consumed={ime_consumed}");
                    }
                }
            }
        }
    }
}

#[test]
fn both_search_chords_open_and_close_main_floating_and_node_owners_without_actions() {
    for locale in [Locale::En, Locale::Zh] {
        for (owner, node_context) in [
            (None, false),
            (Some(Panel::Info), false),
            (None, true),
            (Some(Panel::NodeProperties), true),
        ] {
            for opening in [EFFECT_SEARCH, EFFECT_SEARCH_ALT] {
                for closing in [EFFECT_SEARCH, EFFECT_SEARCH_ALT] {
                    let mut panels = Panels {
                        locale,
                        ..Default::default()
                    };
                    let before = panels.node_editor.snapshot();
                    let mut actions = vec![];
                    let input = Input {
                        mouse: [50., 60.],
                        keys: vec![(opening.key, opening.modifiers)],
                        ..Default::default()
                    };
                    assert!(keyboard::route(
                        &input,
                        &mut panels,
                        &Value::Null,
                        node_context,
                        owner,
                        &mut actions
                    ));
                    let search = panels.effect_search.as_ref().unwrap();
                    assert_eq!(search.owner, owner);
                    assert_eq!(search.position, input.mouse);
                    assert!(search.query.is_empty() && !search.continuous);
                    assert!(actions.is_empty());
                    let other = if owner.is_none() {
                        Some(Panel::Info)
                    } else {
                        None
                    };
                    let input = Input {
                        keys: vec![
                            (closing.key, closing.modifiers),
                            (Key::Delete, Default::default()),
                        ],
                        ..Default::default()
                    };
                    assert!(keyboard::route(
                        &input,
                        &mut panels,
                        &Value::Null,
                        node_context,
                        other,
                        &mut actions
                    ));
                    assert!(panels.effect_search.is_none());
                    assert!(
                        actions.is_empty(),
                        "closing frame escaped into editor commands"
                    );
                    assert_eq!(panels.node_editor.snapshot().document, before.document);
                    assert_eq!(panels.node_editor.snapshot().revision, before.revision);
                }
            }
        }
    }
}

#[test]
fn alternate_defers_to_text_menus_repeat_and_ime_and_cannot_be_rebound() {
    for blocked in 0..5 {
        let mut panels = Panels::default();
        let mut input = Input {
            keys: vec![(EFFECT_SEARCH_ALT.key, EFFECT_SEARCH_ALT.modifiers)],
            ..Default::default()
        };
        match blocked {
            0 => input.key_repeats = vec![true],
            1 => input.composing = true,
            2 => input.ime_consumed = true,
            3 => panels.project.search = true,
            _ => panels.menu = Some(0),
        }
        let mut actions = vec![];
        keyboard::route(&input, &mut panels, &Value::Null, false, None, &mut actions);
        assert!(panels.effect_search.is_none(), "blocked case {blocked}");
        assert!(actions.is_empty());
    }
    let mut panels = Panels {
        effect_search: Some(panels::EffectSearch::default()),
        ..Default::default()
    };
    for (repeat, composing, ime_consumed) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let input = Input {
            keys: vec![(EFFECT_SEARCH_ALT.key, EFFECT_SEARCH_ALT.modifiers)],
            key_repeats: vec![repeat],
            composing,
            ime_consumed,
            ..Default::default()
        };
        assert!(keyboard::route(
            &input,
            &mut panels,
            &Value::Null,
            false,
            None,
            &mut vec![]
        ));
        assert!(panels.effect_search.is_some());
    }
    assert!(bindings::Bindings::default()
        .bind(
            Scope::Application,
            Command::Settings,
            Some(EFFECT_SEARCH_ALT)
        )
        .unwrap_err()
        .contains("reserved"));
}
