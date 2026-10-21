use super::*;
use crate::panels::{animation_controls::TrackSelection, keyboard, property_focus, Panels};
use motion_core::{vector::groups::VectorGroup, Engine, Keyframe, Project, Property};
use motion_ui::dock::Panel;

fn key<T>(frame: i32, value: T) -> Keyframe<T> {
    Keyframe {
        frame,
        value,
        ease: motion_core::Ease::Linear,
        curve: None,
        spatial: None,
    }
}

fn fixture() -> Engine {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    project.version = motion_model::CURRENT_PROJECT_VERSION;
    let mut layer = Layer::solid(7, "Properties", [100.; 2], [0.; 3], [1.; 4]);
    layer.transform.position.keys = vec![key(10, [0.; 3]), key(90, [100.; 3])];
    let path = VectorPath {
        id: 1,
        closed: true,
        nodes: [[0., 0.], [100., 0.], [0., 100.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| PathNode {
                id: i as u64 + 1,
                geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
            })
            .collect(),
    };
    let mut mask = LayerMask::new(1, path.clone());
    mask.opacity.keys = vec![key(20, 25.), key(80, 75.)];
    layer.masks.push(mask);
    let mut vector = new_vector(path);
    vector.fill = None;
    vector.stroke = None;
    let mut group = VectorGroup {
        id: 1,
        name: "Group".into(),
        transform: Default::default(),
        items: vec![],
    };
    group.transform.position.keys = vec![key(30, [0.; 2]), key(70, [100.; 2])];
    vector.source = VectorSource::Group {
        group: Box::new(group),
    };
    layer.content = Content::Vector { vector };
    project.layers.push(layer);
    Engine::new(project).unwrap()
}

fn state(engine: &Engine) -> Value {
    serde_json::json!({"root":"fixture","composition":"main","frame":40,
        "revision":1,"project":engine.project(),"timeline_layers":engine.project().timeline_layers(40.)})
}

fn choose_mask(panels: &mut Panels, state: &Value) {
    let layer = layer(state, 7).unwrap();
    panels
        .path_controls
        .keys
        .choose(state, mask_keys::Owner::from(&layer), 1, 0, &mut vec![]);
    property_focus::settle(panels);
}

fn choose_shape(panels: &mut Panels, state: &Value) {
    let layer = layer(state, 7).unwrap();
    let Content::Vector { vector } = &layer.content else {
        panic!("vector fixture")
    };
    let parameter =
        groups::parameters::lookup_source(vector, layer.size, 1, "position", 40., Locale::En)
            .unwrap();
    panels
        .path_controls
        .groups
        .keys
        .prepare(state, &Input::default(), &mut vec![]);
    panels.path_controls.groups.keys.choose(7, 1, &parameter);
    property_focus::settle(panels);
}

fn route(panels: &mut Panels, state: &Value, key: Key, owner: Option<Panel>) -> Vec<Action> {
    let mut out = vec![];
    keyboard::route(
        &Input {
            keys: vec![(key, Default::default())],
            ..Default::default()
        },
        panels,
        state,
        false,
        owner,
        &mut out,
    );
    out
}

#[test]
fn latest_typed_property_choice_replaces_old_transform_mask_and_shape_owners() {
    let engine = fixture();
    let state = state(&engine);
    let before = engine.project().clone();
    for locale in [Locale::En, Locale::Zh] {
        for owner in [None, Some(Panel::Timeline)] {
            let mut panels = Panels {
                locale,
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            panels.timeline.expanded.insert(7);
            for (kind, expected) in [(0, 90.), (1, 80.), (2, 70.), (0, 90.), (2, 70.), (1, 80.)] {
                match kind {
                    0 => {
                        panels.animation_controls.choose_track_in(
                            &state,
                            7,
                            Property::Position,
                            None,
                        );
                        property_focus::settle(&mut panels);
                    }
                    1 => choose_mask(&mut panels, &state),
                    _ => choose_shape(&mut panels, &state),
                }
                // Passive draws/dispatch must not publish a competing selection.
                property_focus::settle(&mut panels);
                let actions = route(&mut panels, &state, Key::K, owner);
                assert!(
                    matches!(actions.as_slice(),[Action::Seek(frame)] if *frame == expected),
                    "kind={kind} locale={locale:?} owner={owner:?}: {actions:?}"
                );
                if kind != 0 {
                    assert_eq!(
                        panels.animation_controls.track_selection(&state, Some(7)),
                        TrackSelection::Invalid
                    );
                }
            }
            panels.select_only(Some(7));
            assert_eq!(
                panels.animation_controls.track_selection(&state, Some(7)),
                TrackSelection::Unselected
            );
            assert!(matches!(
                route(&mut panels, &state, Key::Delete, owner).as_slice(),
                [Action::Edit(Command::Delete { object: 7 })]
            ));
            assert_eq!(engine.project(), &before);
        }
    }
}

#[test]
fn stale_typed_owners_consume_repeated_keys_before_and_after_their_prepare_pass() {
    let engine = fixture();
    let original = state(&engine);
    for shape in [false, true] {
        for owner in [None, Some(Panel::Timeline)] {
            for case in 0..4 {
                let mut panels = Panels {
                    focused: Some(Panel::Timeline),
                    ..Default::default()
                };
                panels.select_only(Some(7));
                panels
                    .animation_controls
                    .choose_track_in(&original, 7, Property::Position, None);
                property_focus::settle(&mut panels);
                if shape {
                    choose_shape(&mut panels, &original)
                } else {
                    choose_mask(&mut panels, &original)
                }
                let mut changed = original.clone();
                match case {
                    0 => changed["root"] = "reused-object-in-other-project".into(),
                    1 => changed["composition"] = "other-composition".into(),
                    2 => {
                        changed["project"]["layers"][0]["masks"] = serde_json::json!([]);
                        changed["project"]["layers"][0]["content"] = serde_json::to_value(
                            Layer::solid(9, "Other", [100.; 2], [0.; 3], [1.; 4]).content,
                        )
                        .unwrap();
                        changed["revision"] = 2.into();
                    }
                    _ => changed["project"]["layers"] = serde_json::json!([]),
                }
                for pass in 0..2 {
                    for key in [Key::J, Key::K, Key::Delete] {
                        assert!(
                            route(&mut panels, &changed, key, owner).is_empty(),
                            "shape={shape} owner={owner:?} case={case} pass={pass} key={key:?}"
                        );
                    }
                    // Rendering can clear a no-longer-valid typed selection; its explicit
                    // property ownership must still prevent a layer/transform fallback.
                    panels.path_controls.clear_mask_selection();
                }
            }
        }
    }
}

#[test]
fn old_graph_box_cannot_delete_keys_after_another_property_takes_ownership() {
    let engine = fixture();
    let state = state(&engine);
    for shape in [false, true] {
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        panels.select_only(Some(7));
        panels.timeline.graph_mode = true;
        panels
            .animation_controls
            .choose_track_in(&state, 7, Property::Position, None);
        property_focus::settle(&mut panels);
        assert!(crate::panels::graph_box::select_all(&mut panels, &state));
        assert!(panels.timeline.graph_box.has_selection());
        if shape {
            choose_shape(&mut panels, &state)
        } else {
            choose_mask(&mut panels, &state)
        }
        assert!(route(&mut panels, &state, Key::Delete, None).is_empty());
        assert!(!panels.timeline.graph_box.has_selection());
        let mut out = vec![];
        keyboard::route(
            &Input {
                keys: vec![(
                    Key::A,
                    motion_ui::input::Modifiers {
                        control: true,
                        ..Default::default()
                    },
                )],
                ..Default::default()
            },
            &mut panels,
            &state,
            false,
            None,
            &mut out,
        );
        assert!(out.is_empty());
        assert!(!panels.timeline.graph_box.has_selection());
    }
}

#[test]
fn property_navigation_follows_the_chosen_channel_across_panel_focus_and_defers_to_ime() {
    let engine = fixture();
    let state = state(&engine);
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            for focus in [
                Panel::Timeline,
                Panel::Curves,
                Panel::Properties,
                Panel::EffectControls,
                Panel::Masks,
                Panel::Vector,
                Panel::Audio,
                Panel::Composition,
            ] {
                let mut panels = Panels {
                    locale,
                    focused: Some(focus),
                    ..Default::default()
                };
                panels.select_only(Some(7));
                let owner = floating.then_some(focus);
                for (kind, expected) in [(0, 90.), (1, 80.), (2, 70.)] {
                    match kind {
                        0 => {
                            panels.animation_controls.choose_track_in(
                                &state,
                                7,
                                Property::Position,
                                None,
                            );
                            property_focus::settle(&mut panels);
                        }
                        1 => choose_mask(&mut panels, &state),
                        _ => choose_shape(&mut panels, &state),
                    }
                    assert!(
                        matches!(route(&mut panels,&state,Key::K,owner).as_slice(),
                    [Action::Seek(frame)] if *frame == expected),
                        "{focus:?} kind={kind}"
                    );
                    for composing in [false, true] {
                        let mut out = vec![];
                        keyboard::route(
                            &Input {
                                keys: vec![(Key::K, Default::default())],
                                composing,
                                ime_consumed: !composing,
                                ..Default::default()
                            },
                            &mut panels,
                            &state,
                            false,
                            owner,
                            &mut out,
                        );
                        assert!(out.is_empty());
                    }
                }
            }
        }
    }
}
