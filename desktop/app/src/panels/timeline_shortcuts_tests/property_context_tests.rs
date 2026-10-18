use super::super::animation_controls::{SelectedTrack, TrackSelection};
use super::*;

#[test]
fn explicit_property_navigation_uses_one_channel_in_normal_and_graph_hosts() {
    let engine = engine();
    let before = engine.snapshot();
    for locale in [Locale::En, Locale::Zh] {
        for owner in [None, Some(Panel::Timeline)] {
            for graph in [false, true] {
                for (key, expected) in [(Key::J, 12.), (Key::K, 72.)] {
                    let mut panels = panels();
                    panels.locale = locale;
                    panels.timeline.graph_mode = graph;
                    panels.timeline.expanded.insert(7);
                    let state = state(&engine, 40.);
                    panels
                        .animation_controls
                        .choose_track_in(&state, 7, Property::Opacity, None);
                    let mut actions = vec![];
                    keyboard::route(
                        &Input {
                            keys: vec![(key, Modifiers::default())],
                            ..Default::default()
                        },
                        &mut panels,
                        &state,
                        false,
                        owner,
                        &mut actions,
                    );
                    assert!(
                        matches!(actions.as_slice(), [Action::Seek(frame)] if *frame == expected),
                        "locale={locale:?} owner={owner:?} graph={graph} actions={actions:?}"
                    );
                    assert_eq!(engine.project(), &before);
                }
            }
        }
    }
}

#[test]
fn projection_keeps_unselected_layer_navigation_and_cannot_rebind_explicit_selection() {
    let engine = engine();
    let state = state(&engine, 40.);
    let mut panels = panels();
    panels.timeline.expanded.insert(7);
    panels
        .animation_controls
        .display_track(7, Property::Opacity, None);
    assert_eq!(
        panels.animation_controls.track_selection(&state, Some(7)),
        TrackSelection::Unselected
    );
    let actions = invoke(&mut panels, &state, Key::K, Modifiers::default());
    assert!(matches!(actions.as_slice(), [Action::Seek(52.)]));
    panels
        .animation_controls
        .choose_track_in(&state, 7, Property::Opacity, None);
    panels
        .animation_controls
        .display_track(8, Property::Position, None);
    assert_eq!(
        panels.animation_controls.track_selection(&state, Some(7)),
        TrackSelection::Current(SelectedTrack {
            object: 7,
            property: Property::Opacity,
            axis: None
        })
    );
    assert_eq!(
        panels.animation_controls.track_selection(&state, Some(8)),
        TrackSelection::Invalid
    );
}

#[test]
fn changed_project_composition_target_or_dimensions_blocks_stale_navigation() {
    let mut engine = engine();
    let mut original = state(&engine, 40.);
    original["root"] = "project-a".into();
    original["composition"] = motion_core::composition::MAIN_COMPOSITION.into();
    for case in 0..7 {
        let mut panels = panels();
        panels
            .animation_controls
            .choose_track_in(&original, 7, Property::Position, None);
        let mut changed = original.clone();
        match case {
            0 => changed["root"] = "project-b".into(),
            1 => changed["composition"] = "other-composition".into(),
            2 => panels.selected = Some(8),
            3 => changed["project"]["layers"] = serde_json::json!([]),
            4 => {
                engine
                    .apply(Command::SeparateDimensions {
                        object: 7,
                        property: Property::Position,
                    })
                    .unwrap();
                changed = state(&engine, 40.);
                changed["root"] = original["root"].clone();
                changed["composition"] = original["composition"].clone();
            }
            5 => changed["timeline_layers"] = serde_json::json!([]),
            6 => changed["timeline_layers"][0]["properties"]["position"] = Value::Null,
            _ => unreachable!(),
        }
        assert_eq!(
            panels
                .animation_controls
                .track_selection(&changed, panels.selected),
            TrackSelection::Invalid,
            "case={case}"
        );
        let actions = invoke(&mut panels, &changed, Key::K, Modifiers::default());
        assert!(actions.is_empty(), "case={case} actions={actions:?}");
    }
    engine.undo().unwrap();
    assert_eq!(
        engine.project(),
        &serde_json::from_value::<Project>(original["project"].clone()).unwrap()
    );
}

#[test]
fn stale_key_owns_delete_without_editing_reused_objects_or_falling_through_to_layer_delete() {
    let engine = engine();
    let mut original = state(&engine, 32.);
    original["root"] = "project-a".into();
    original["composition"] = "main".into();
    for focus in [Panel::Timeline, Panel::Curves] {
        for case in 0..4 {
            let mut changed = original.clone();
            match case {
                0 => changed["root"] = "project-b".into(),
                1 => changed["composition"] = "other".into(),
                2 => {
                    changed["timeline_layers"][0]["properties"]["position"]["keys"] =
                        serde_json::json!([])
                }
                3 => changed["project"]["layers"] = serde_json::json!([]),
                _ => unreachable!(),
            }
            for key in [Key::Delete, Key::F9] {
                let mut panels = panels();
                panels.focused = Some(focus);
                panels
                    .animation_controls
                    .select_key_in(&original, 7, Property::Position, None, 32);
                assert!(panels
                    .animation_controls
                    .user_key_in(&changed, Some(7))
                    .is_none());
                let actions = invoke(&mut panels, &changed, key, Modifiers::default());
                assert!(
                    actions.is_empty(),
                    "focus={focus:?} case={case} key={key:?}: {actions:?}"
                );
                assert!(
                    panels.animation_controls.user_key().is_some(),
                    "ownership must survive validation"
                );
            }
        }
    }
}

#[test]
fn property_reveal_selects_an_available_separated_axis_including_2d_rotation_z() {
    let mut engine = engine();
    for property in [Property::Position, Property::Rotation] {
        engine
            .apply(Command::SeparateDimensions {
                object: 7,
                property,
            })
            .unwrap();
    }
    for three_d in [true, false] {
        let mut state = state(&engine, 32.);
        state["project"]["layers"][0]["three_d"] = three_d.into();
        for (key, property, axis) in [
            (Key::P, Property::Position, Axis::X),
            (
                Key::R,
                Property::Rotation,
                if three_d { Axis::X } else { Axis::Z },
            ),
        ] {
            let mut panels = panels();
            assert!(invoke(&mut panels, &state, key, Modifiers::default()).is_empty());
            assert_eq!(
                panels.animation_controls.track_selection(&state, Some(7)),
                TrackSelection::Current(SelectedTrack {
                    object: 7,
                    property,
                    axis: Some(axis)
                })
            );
        }
    }
}

#[test]
fn invalid_scope_cancels_an_active_graph_transaction_exactly_once_and_restores_authored_data() {
    let mut engine = engine();
    let before = engine.project().clone();
    let mut original = state(&engine, 32.);
    original["root"] = "project-a".into();
    let mut panels = panels();
    panels
        .animation_controls
        .select_key_in(&original, 7, Property::Position, None, 32);
    let source = animation_data::channel(&original, 7, Property::Position, None).unwrap();
    let press = Input {
        mouse: [320., 100.],
        pressed: Some((MouseButton::Left, [320., 100.])),
        ..Default::default()
    };
    let mut actions = vec![];
    graph_keys::begin(
        &mut panels,
        &original,
        7,
        Property::Position,
        None,
        0,
        source,
        32,
        [320., 100.],
        100.,
        [0., 200.],
        &press,
        &mut actions,
    );
    let held = Input {
        mouse: [340., 100.],
        held: Some(MouseButton::Left),
        ..Default::default()
    };
    graph_keys::update(
        &mut panels,
        &original,
        Rect::new(0., 0., 600., 200.),
        10.,
        1.,
        source,
        &held,
        &mut actions,
    );
    assert!(panels.timeline.key_drag.as_ref().unwrap().begun());
    apply_graph_actions(&mut engine, actions);
    assert_ne!(engine.project(), &before);
    let mut changed = state(&engine, 34.);
    changed["root"] = "project-b".into();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut actions = vec![];
    for pass in 0..2 {
        timeline_graph::render(
            &mut PaintList::default(),
            &mut atlas,
            Rect::new(0., 0., 600., 300.),
            &mut panels,
            &changed,
            &held,
            0.,
            10.,
            &mut actions,
        );
        if pass == 0 {
            assert!(matches!(actions.as_slice(), [Action::History(4)]));
            apply_graph_actions(&mut engine, std::mem::take(&mut actions));
            assert_eq!(engine.project(), &before);
        } else {
            assert!(actions.is_empty());
        }
        assert!(panels.timeline.key_drag.is_none());
        assert!(panels.timeline.graph_drag.is_none());
    }
}

#[test]
fn legacy_curve_projection_is_passive_and_explicit_curve_input_binds_the_displayed_key() {
    let engine = engine();
    let mut original = state(&engine, 32.);
    original["root"] = "project-a".into();
    let layer = &original["project"]["layers"][0];
    let timeline = &original["timeline_layers"][0];
    let sample = serde_json::json!({"position":[100.,50.,20.],"opacity":1.});
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut controls = animation_controls::Controls::default();
    controls.expanded = true;
    let mut draw = |controls: &mut animation_controls::Controls, state: &Value, input: Input| {
        let mut actions = vec![];
        controls.render(
            &mut PaintList::default(),
            &mut atlas,
            Rect::new(0., 0., 720., 800.),
            &mut 0.,
            layer,
            &sample,
            timeline,
            state,
            &input,
            Locale::En,
            &mut actions,
        );
        actions
    };
    let point = [30., 186.]; // Existing Linear control, after the read-only key preview.
    let click = || Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    };
    assert!(draw(&mut controls, &original, Input::default()).is_empty());
    assert_eq!(
        controls.track_selection(&original, Some(7)),
        TrackSelection::Unselected
    );
    assert!(controls.user_key().is_none());
    assert!(matches!(
        draw(&mut controls, &original, click()).as_slice(),
        [Action::Edit(Command::Curve {
            object: 7,
            property: Property::Position,
            frame: 32,
            ..
        })]
    ));
    assert_eq!(controls.user_key_in(&original, Some(7)).unwrap().frame, 32);
    let mut changed = original.clone();
    changed["root"] = "project-b".into();
    assert!(draw(&mut controls, &changed, click()).is_empty());
    assert_eq!(
        controls.track_selection(&changed, Some(7)),
        TrackSelection::Invalid
    );
    // Deleting this key must not silently redirect the curve buttons to its
    // surviving predecessor in the same valid object/property/axis context.
    let mut deleted = original.clone();
    deleted["timeline_layers"][0]["properties"]["position"]["keys"]
        .as_array_mut()
        .unwrap()
        .retain(|key| key["frame"] != 32);
    let mut actions = vec![];
    controls.render(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 0., 720., 800.),
        &mut 0.,
        layer,
        &sample,
        &deleted["timeline_layers"][0],
        &deleted,
        &click(),
        Locale::En,
        &mut actions,
    );
    assert!(actions.is_empty());
    assert_eq!(controls.user_key().unwrap().frame, 32);
}
