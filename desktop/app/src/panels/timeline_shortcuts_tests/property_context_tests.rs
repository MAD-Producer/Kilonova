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
    for case in 0..5 {
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
