use super::*;

fn route(panels: &mut Panels, state: &Value, key: Key, owner: Option<Panel>) -> Vec<Action> {
    let mut actions = Vec::new();
    keyboard::route(
        &Input {
            keys: vec![(key, Modifiers::default())],
            ..Default::default()
        },
        panels,
        state,
        false,
        owner,
        &mut actions,
    );
    actions
}

#[test]
fn reveal_retains_selected_axis_and_key_for_ease_delete_and_undo() {
    for locale in [Locale::En, Locale::Zh] {
        for host in [Panel::Timeline, Panel::Curves] {
            for floating in [false, true] {
                for axis in [Axis::Y, Axis::Z, Axis::X] {
                    for command in [Key::F9, Key::Delete] {
                        let mut engine = engine();
                        engine
                            .apply(Command::SeparateDimensions {
                                object: 7,
                                property: Property::Position,
                            })
                            .unwrap();
                        let before = engine.snapshot();
                        let state = state(&engine, 32.);
                        let mut panels = panels();
                        panels.locale = locale;
                        panels.focused = Some(host);
                        let owner = floating.then_some(host);
                        panels.animation_controls.select_key_in(
                            &state,
                            7,
                            Property::Position,
                            Some(axis),
                            32,
                        );
                        let selected = panels.animation_controls.user_key_in(&state, Some(7));
                        assert!(route(&mut panels, &state, Key::P, owner).is_empty());
                        assert_eq!(panels.animation_controls.axis, Some(axis));
                        assert_eq!(
                            panels.animation_controls.user_key_in(&state, Some(7)),
                            selected
                        );
                        assert_eq!(engine.project(), &before);
                        let actions = route(&mut panels, &state, command, owner);
                        if command == Key::F9 {
                            assert!(matches!(actions.as_slice(), [Action::EditBatch(commands)]
                                if commands.len() == 2 && commands.iter().all(|command|
                                    matches!(command, Command::Curve { object: 7, property: Property::Position,
                                        axis: Some(target), frame: 12 | 32, .. } if *target == axis))));
                        } else {
                            assert!(
                                matches!(actions.as_slice(), [Action::Edit(Command::DeleteKey {
                                object: 7, property: Property::Position, axis: Some(target), frame: 32
                            })] if *target == axis)
                            );
                        }
                        apply(&mut engine, actions);
                        let old = before.layers[0].transform.position.axes.as_ref().unwrap();
                        let new = engine.project().layers[0]
                            .transform
                            .position
                            .axes
                            .as_ref()
                            .unwrap();
                        for (candidate, original, actual) in [
                            (Axis::X, &old.x, &new.x),
                            (Axis::Y, &old.y, &new.y),
                            (Axis::Z, &old.z, &new.z),
                        ] {
                            if candidate != axis {
                                assert_eq!(actual, original);
                            } else if command == Key::F9 {
                                assert_eq!(actual.keys.len(), original.keys.len());
                                assert!(actual
                                    .keys
                                    .iter()
                                    .zip(&original.keys)
                                    .all(|(actual, original)| actual.frame == original.frame
                                        && actual.value == original.value));
                                assert!(actual.keys[1].curve.is_some());
                            } else {
                                assert_eq!(actual.keys.len(), original.keys.len() - 1);
                                assert_eq!(actual.keys[0], original.keys[0]);
                                assert_eq!(actual.keys[1], original.keys[2]);
                            }
                        }
                        engine.undo().unwrap();
                        assert_eq!(engine.project(), &before);
                    }
                }
            }
        }
    }
}

#[test]
fn property_inspector_reveal_retains_the_selected_axis_and_key() {
    let mut engine = engine();
    engine
        .apply(Command::SeparateDimensions {
            object: 7,
            property: Property::Position,
        })
        .unwrap();
    let state = state(&engine, 32.);
    for locale in [Locale::En, Locale::Zh] {
        for owner in [None, Some(Panel::Properties)] {
            for axis in [Axis::Y, Axis::Z, Axis::X] {
                let mut panels = panels();
                panels.locale = locale;
                panels.focused = Some(Panel::Properties);
                panels.animation_controls.select_key_in(
                    &state,
                    7,
                    Property::Position,
                    Some(axis),
                    32,
                );
                let selected = panels.animation_controls.user_key_in(&state, Some(7));
                assert!(route(&mut panels, &state, Key::P, owner).is_empty());
                assert_eq!(panels.animation_controls.axis, Some(axis));
                assert_eq!(
                    panels.animation_controls.user_key_in(&state, Some(7)),
                    selected
                );
            }
        }
    }
}

#[test]
fn scale_and_rotation_reveal_keep_each_valid_selected_axis() {
    for (property, key) in [(Property::Scale, Key::S), (Property::Rotation, Key::R)] {
        let mut engine = engine();
        engine
            .apply(Command::SeparateDimensions {
                object: 7,
                property,
            })
            .unwrap();
        let state = state(&engine, 32.);
        for axis in [Axis::Y, Axis::Z, Axis::X] {
            let mut panels = panels();
            panels
                .animation_controls
                .choose_track_in(&state, 7, property, Some(axis));
            let selected = panels.animation_controls.track_selection(&state, Some(7));
            assert!(matches!(
                selected,
                animation_controls::TrackSelection::Current(_)
            ));
            assert!(route(&mut panels, &state, key, None).is_empty());
            assert_eq!(
                panels.animation_controls.track_selection(&state, Some(7)),
                selected
            );
        }
    }
}

#[test]
fn reveal_does_not_reuse_a_stale_scope_or_another_property_axis() {
    let mut engine = engine();
    engine
        .apply(Command::SeparateDimensions {
            object: 7,
            property: Property::Position,
        })
        .unwrap();
    for stale_scope in [false, true] {
        let mut state = state(&engine, 32.);
        state["root"] = "first-project".into();
        let mut panels = panels();
        panels.animation_controls.select_key_in(
            &state,
            7,
            if stale_scope {
                Property::Position
            } else {
                Property::Opacity
            },
            if stale_scope { Some(Axis::Y) } else { None },
            32,
        );
        if stale_scope {
            state["root"] = "second-project".into();
        }
        assert!(route(&mut panels, &state, Key::P, None).is_empty());
        assert_eq!(panels.animation_controls.axis, Some(Axis::X));
        assert!(panels
            .animation_controls
            .user_key_in(&state, Some(7))
            .is_none());
        assert!(route(&mut panels, &state, Key::F9, None).is_empty());
    }
}
