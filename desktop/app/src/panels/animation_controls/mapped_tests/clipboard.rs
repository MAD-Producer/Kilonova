use super::*;
use crate::layer_clipboard::controller::Intent;
use crate::panels::{animation_menu, key_selection, Panel, Panels};

#[test]
fn fractional_mini_clipboard_buttons_keep_local_key_in_both_directions_and_languages() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            let engine = fixture(reverse);
            let state = state(&engine, projected(&engine, 3.));
            let mut c = controls();
            assert!(draw(&mut c, &mut atlas, &state, &Input::default(), locale).is_empty());
            let hits = c.clipboard_test_rects().unwrap();
            for (index, intent) in [(0, Intent::Copy), (1, Intent::Paste { in_place: true })] {
                let point = [
                    (hits[index].min[0] + hits[index].max[0]) * 0.5,
                    (hits[index].min[1] + hits[index].max[1]) * 0.5,
                ];
                let actions = draw(&mut c, &mut atlas, &state, &click(point), locale);
                let [Action::CurveClipboard {
                    intent: actual,
                    source,
                    selections,
                    local_selections,
                }] = actions.as_slice()
                else {
                    panic!("{actions:?}")
                };
                assert_eq!(*actual, intent);
                assert_eq!(source, &crate::layer_clipboard::KeyframeSource::of(&state));
                assert!(selections.is_empty());
                assert_eq!(local_selections.len(), 1);
                assert_eq!(local_selections[0].frames, [3]);
                assert_eq!(
                    local_selections[0].target,
                    motion_core::KeyframeTarget::Property {
                        object: 7,
                        property: Property::Opacity,
                        axis: None
                    }
                );
                for input in [
                    Input {
                        focus_lost: true,
                        ..click(point)
                    },
                    Input {
                        composing: true,
                        ..click(point)
                    },
                    Input {
                        ime_consumed: true,
                        ..click(point)
                    },
                ] {
                    assert!(draw(&mut c, &mut atlas, &state, &input, locale).is_empty());
                }
            }
        }
    }
}

#[test]
fn local_menu_copy_and_paste_follow_owned_selection_lock_and_stale_guards() {
    for reverse in [false, true] {
        let engine = fixture(reverse);
        let state = state(&engine, projected(&engine, 3.));
        let mut p = Panels {
            focused: Some(Panel::Curves),
            selected: Some(7),
            ..Default::default()
        };
        p.selected_keys
            .apply_addressed(
                &state,
                [key_selection::LocalKey {
                    target: motion_core::KeyframeTarget::Property {
                        object: 7,
                        property: Property::Opacity,
                        axis: None,
                    },
                    frame: 3,
                }
                .into()],
                key_selection::Mode::Replace,
            )
            .unwrap();
        for op in [
            animation_menu::Operation::CopyEasing,
            animation_menu::Operation::PasteEasing,
        ] {
            assert!(animation_menu::enabled(&p, &state, op));
            let mut actions = vec![];
            animation_menu::apply(&mut p, &state, op, &mut actions);
            assert!(
                matches!(actions.as_slice(),[Action::CurveClipboard {selections,local_selections,..}]
                if selections.is_empty() && local_selections.len()==1 && local_selections[0].frames==[3])
            );
            p.clipboard_busy = true;
            assert!(!animation_menu::enabled(&p, &state, op));
            p.clipboard_busy = false;
            let mut stale = state.clone();
            stale["document_generation"] = 1000.into();
            assert!(!animation_menu::enabled(&p, &stale, op));
        }
        let mut locked = state.clone();
        locked["project"]["layers"][0]["locked"] = true.into();
        assert!(animation_menu::enabled(
            &p,
            &locked,
            animation_menu::Operation::CopyEasing
        ));
        assert!(!animation_menu::enabled(
            &p,
            &locked,
            animation_menu::Operation::PasteEasing
        ));
    }
}
