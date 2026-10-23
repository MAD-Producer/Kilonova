use super::*;

fn snapshot(engine: &Engine) -> Value {
    let mut snapshot = state(engine, 32.);
    snapshot["root"] = "project-a".into();
    snapshot["composition"] = "main".into();
    snapshot
}

fn notice(locale: Locale) -> &'static str {
    match locale {
        Locale::Zh => "所选关键帧已失效，请重新选择",
        Locale::En => "Selected keyframe is no longer available; select it again",
    }
}

#[test]
fn stale_edits_are_explained_without_clearing_ownership_or_editing_the_project() {
    for locale in [Locale::En, Locale::Zh] {
        for focus in [Panel::Timeline, Panel::Curves] {
            for owner in [None, Some(focus)] {
                for case in 0..4 {
                    for (key, modifiers) in [
                        (Key::Delete, Modifiers::default()),
                        (Key::F9, Modifiers::default()),
                        (
                            Key::F9,
                            Modifiers {
                                shift: true,
                                ..Default::default()
                            },
                        ),
                        (
                            Key::F9,
                            Modifiers {
                                control: true,
                                shift: true,
                                ..Default::default()
                            },
                        ),
                    ] {
                        let mut engine = engine();
                        let original = snapshot(&engine);
                        let mut panels = panels();
                        panels.locale = locale;
                        panels.focused = Some(focus);
                        panels.animation_controls.select_key_in(
                            &original,
                            7,
                            Property::Position,
                            None,
                            32,
                        );
                        let selected = panels.animation_controls.user_key();
                        assert!(selected.is_some());
                        let mut changed = original.clone();
                        match case {
                            0 => changed["root"] = "project-b".into(),
                            1 => changed["composition"] = "other".into(),
                            2 => {
                                engine
                                    .apply(Command::DeleteKey {
                                        object: 7,
                                        property: Property::Position,
                                        axis: None,
                                        frame: 32,
                                    })
                                    .unwrap();
                                changed = snapshot(&engine);
                            }
                            _ => {
                                engine.apply(Command::Delete { object: 7 }).unwrap();
                                changed = snapshot(&engine);
                            }
                        }
                        assert!(panels
                            .animation_controls
                            .user_key_in(&changed, Some(7))
                            .is_none());
                        let before = engine.project().clone();
                        let history = (engine.can_undo(), engine.can_redo());
                        for _ in 0..2 {
                            panels.message = None;
                            let mut actions = Vec::new();
                            keyboard::route(
                                &Input {
                                    keys: vec![(key, modifiers)],
                                    ..Default::default()
                                },
                                &mut panels,
                                &changed,
                                false,
                                owner,
                                &mut actions,
                            );
                            assert!(actions.is_empty(), "case={case} key={key:?}: {actions:?}");
                            assert_eq!(panels.animation_controls.user_key(), selected);
                            assert_eq!(engine.project(), &before);
                            assert_eq!((engine.can_undo(), engine.can_redo()), history);
                            assert_eq!(panels.message.as_deref(), Some(notice(locale)), "locale={locale:?} focus={focus:?} owner={owner:?} case={case} key={key:?}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn reselecting_an_available_key_allows_real_easing_delete_and_single_undo() {
    for locale in [Locale::En, Locale::Zh] {
        for focus in [Panel::Timeline, Panel::Curves] {
            let mut engine = engine();
            let original = snapshot(&engine);
            let mut panels = panels();
            panels.locale = locale;
            panels.focused = Some(focus);
            panels
                .animation_controls
                .select_key_in(&original, 7, Property::Position, None, 32);
            engine
                .apply(Command::DeleteKey {
                    object: 7,
                    property: Property::Position,
                    axis: None,
                    frame: 32,
                })
                .unwrap();
            let changed = snapshot(&engine);
            let before = engine.project().clone();
            assert!(invoke(&mut panels, &changed, Key::F9, Modifiers::default()).is_empty());
            for key in [Key::F9, Key::Delete] {
                panels.message = None;
                panels
                    .animation_controls
                    .select_key_in(&changed, 7, Property::Position, None, 52);
                let actions = invoke(&mut panels, &changed, key, Modifiers::default());
                assert!(!actions.is_empty());
                assert!(panels.message.is_none());
                apply(&mut engine, actions);
                assert_ne!(engine.project(), &before);
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
            }
        }
    }
}

#[test]
fn ime_text_and_repeat_ownership_keep_existing_feedback() {
    let engine = engine();
    let original = snapshot(&engine);
    let mut changed = original.clone();
    changed["root"] = "project-b".into();
    for key in [Key::Delete, Key::F9] {
        for suppressed in 0..4 {
            let mut panels = panels();
            panels
                .animation_controls
                .select_key_in(&original, 7, Property::Position, None, 32);
            panels.message = Some("keep current".into());
            let mut input = Input {
                keys: vec![(key, Modifiers::default())],
                ..Default::default()
            };
            match suppressed {
                0 => input.composing = true,
                1 => input.ime_consumed = true,
                2 => panels.timeline.timeline_search = true,
                _ => input.key_repeats = vec![true],
            }
            let mut actions = Vec::new();
            keyboard::route(&input, &mut panels, &changed, false, None, &mut actions);
            assert!(actions.is_empty());
            assert_eq!(panels.message.as_deref(), Some("keep current"));
        }
    }
}

#[test]
fn stale_notice_is_painted_in_both_window_hosts_and_locales() {
    for locale in [Locale::En, Locale::Zh] {
        for size in [[900., 600.], [1600., 1000.]] {
            for floating in [false, true] {
                let mut engine = engine();
                let original = snapshot(&engine);
                let mut panels = panels();
                panels.locale = locale;
                panels
                    .animation_controls
                    .select_key_in(&original, 7, Property::Position, None, 32);
                engine
                    .apply(Command::DeleteKey {
                        object: 7,
                        property: Property::Position,
                        axis: None,
                        frame: 32,
                    })
                    .unwrap();
                let changed = snapshot(&engine);
                let mut actions = Vec::new();
                keyboard::route(
                    &Input {
                        keys: vec![(Key::F9, Modifiers::default())],
                        ..Default::default()
                    },
                    &mut panels,
                    &changed,
                    false,
                    floating.then_some(Panel::Timeline),
                    &mut actions,
                );
                assert!(actions.is_empty());
                let before = engine.project().clone();
                let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
                let mut paint = PaintList::default();
                let actions = if floating {
                    floating_chrome(
                        &mut paint,
                        &mut atlas,
                        Panel::Timeline,
                        size,
                        &mut panels,
                        &changed,
                        &Input::default(),
                    )
                } else {
                    chrome(
                        &mut paint,
                        &mut atlas,
                        size,
                        &dock::Node::dock(Panel::Timeline.dock()),
                        &mut panels,
                        &changed,
                        &Input::default(),
                    )
                };
                assert!(actions.is_empty());
                assert_eq!(engine.project(), &before);
                let mut expected = PaintList::default();
                let height = if floating { 22. } else { 20. };
                helpers::label(
                    &mut expected,
                    &mut atlas,
                    Rect::new(8., size[1] - height, size[0] - 16., height),
                    notice(locale),
                    true,
                );
                assert!(!expected.glyphs.is_empty());
                let actual: Vec<_> = paint
                    .glyphs
                    .iter()
                    .filter(|g| g.position[1] >= size[1] - 20.)
                    .collect();
                assert_eq!(
                    format!("{actual:?}"),
                    format!("{:?}", expected.glyphs),
                    "locale={locale:?} size={size:?} floating={floating}"
                );
            }
        }
    }
}

#[test]
fn notice_lifetime_follows_reselection_clear_and_restored_keys_without_erasing_other_messages() {
    for initial_locale in [Locale::En, Locale::Zh] {
        for next_locale in [Locale::En, Locale::Zh] {
            for recovery in 0..3 {
                let mut engine = engine();
                let original = snapshot(&engine);
                let mut panels = panels();
                panels.locale = initial_locale;
                panels
                    .animation_controls
                    .select_key_in(&original, 7, Property::Position, None, 32);
                engine
                    .apply(Command::DeleteKey {
                        object: 7,
                        property: Property::Position,
                        axis: None,
                        frame: 32,
                    })
                    .unwrap();
                let mut changed = snapshot(&engine);
                assert!(invoke(&mut panels, &changed, Key::F9, Modifiers::default()).is_empty());
                assert_eq!(panels.message.as_deref(), Some(notice(initial_locale)));
                panels.locale = next_locale;
                let mut actions = Vec::new();
                keyboard::route(
                    &Input::default(),
                    &mut panels,
                    &changed,
                    false,
                    None,
                    &mut actions,
                );
                assert_eq!(panels.message.as_deref(), Some(notice(initial_locale)));
                match recovery {
                    0 => panels.animation_controls.select_key_in(
                        &changed,
                        7,
                        Property::Position,
                        None,
                        52,
                    ),
                    1 => panels.animation_controls.clear_track(),
                    _ => {
                        engine.undo().unwrap();
                        changed = snapshot(&engine);
                    }
                }
                keyboard::route(
                    &Input::default(),
                    &mut panels,
                    &changed,
                    false,
                    None,
                    &mut actions,
                );
                assert!(actions.is_empty());
                assert!(
                    panels.message.is_none(),
                    "recovery={recovery} initial={initial_locale:?} next={next_locale:?}"
                );
                panels.message = Some("existing failure".into());
                keyboard::route(
                    &Input::default(),
                    &mut panels,
                    &changed,
                    false,
                    None,
                    &mut actions,
                );
                assert_eq!(panels.message.as_deref(), Some("existing failure"));
            }
        }
    }
}
