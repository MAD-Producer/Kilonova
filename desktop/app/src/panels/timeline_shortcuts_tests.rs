use super::*;
use motion_core::{
    Axis, Curve, CurveShape, CurveSpace, Ease, Easing, Engine, Keyframe, Layer, Project,
};
use motion_ui::input::Modifiers;

fn key<T>(frame: i32, value: T) -> Keyframe<T> {
    Keyframe {
        frame,
        value,
        ease: Ease::Linear,
        curve: None,
        spatial: None,
    }
}
fn engine() -> Engine {
    let mut project = Project::new(640, 480, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Animated", [100., 100.], [0., 0., 0.], [1.; 4]);
    layer.three_d = true;
    layer.transform.position.keys = vec![
        key(0, [0., 0., 0.]),
        key(20, [100., 50., 20.]),
        key(40, [200., 100., 40.]),
    ];
    layer.transform.opacity.keys = vec![key(0, 0.2), key(60, 0.8)];
    project.layers.push(layer);
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(Command::MoveLayerClip {
            object: 7,
            in_frame: 12,
        })
        .unwrap();
    engine
}
fn state(engine: &Engine, frame: f64) -> Value {
    serde_json::json!({"frame":frame,"project":engine.project(),"timeline_layers":engine.project().timeline_layers(frame)})
}
fn panels() -> Panels {
    Panels {
        selected: Some(7),
        focused: Some(Panel::Timeline),
        ..Default::default()
    }
}
fn invoke(panels: &mut Panels, state: &Value, key: Key, modifiers: Modifiers) -> Vec<Action> {
    let mut out = vec![];
    keyboard::route(
        &Input {
            keys: vec![(key, modifiers)],
            ..Default::default()
        },
        panels,
        state,
        false,
        None,
        &mut out,
    );
    out
}
fn apply(engine: &mut Engine, out: Vec<Action>) {
    for action in out {
        match action {
            Action::Edit(command) => engine.apply(command).unwrap(),
            Action::EditBatch(commands) => {
                engine.apply_batch(commands).unwrap();
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn property_keys_filter_and_append_without_editing_and_u_omits_static_channels() {
    let engine = engine();
    let state = state(&engine, 32.);
    let mut panels = panels();
    assert!(invoke(&mut panels, &state, Key::P, Modifiers::default()).is_empty());
    assert_eq!(
        timeline::filtered_properties(&state, 7, panels.timeline.filters[&7]).len(),
        1
    );
    assert!(timeline::property_visible(
        &panels,
        &state,
        7,
        Property::Position
    ));
    assert!(!timeline::property_visible(
        &panels,
        &state,
        7,
        Property::Scale
    ));
    invoke(
        &mut panels,
        &state,
        Key::S,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert!(timeline::property_visible(
        &panels,
        &state,
        7,
        Property::Scale
    ));
    assert!(timeline::property_visible(
        &panels,
        &state,
        7,
        Property::Position
    ));
    invoke(&mut panels, &state, Key::U, Modifiers::default());
    let rows = timeline::filtered_properties(&state, 7, panels.timeline.filters[&7]);
    assert_eq!(
        rows.iter().map(|r| r.property).collect::<Vec<_>>(),
        [Property::Position, Property::Opacity]
    );
    invoke(&mut panels, &state, Key::U, Modifiers::default());
    assert!(!panels.timeline.expanded.contains(&7));
    assert!(invoke(
        &mut panels,
        &state,
        Key::P,
        Modifiers {
            alt: true,
            ..Default::default()
        }
    )
    .is_empty());
    assert!(!panels.timeline.expanded.contains(&7));
}

#[test]
fn repeated_p_can_be_reopened_manually_in_both_property_views() {
    let engine = engine();
    let mut state = state(&engine, 32.);
    state["sampledLayers"] = serde_json::json!([{"id":7,"position":[10,20,30],"rotation":[0,0,0],"scale":[100,100,100],"opacity":1}]);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for focused in [Panel::Timeline, Panel::Properties] {
        let mut panels = panels();
        panels.focused = Some(focused);
        invoke(&mut panels, &state, Key::P, Modifiers::default());
        invoke(&mut panels, &state, Key::P, Modifiers::default());
        assert_eq!(
            panels.timeline.filters[&7],
            types::PropertyFilter::Properties(0)
        );
        let point = if focused == Panel::Timeline {
            [81., 76.]
        } else {
            [60., 143.]
        };
        let input = Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        };
        let mut actions = vec![];
        if focused == Panel::Timeline {
            timeline::render(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 0., 1200., 600.),
                &mut panels,
                &state,
                &input,
                &mut actions,
            );
            assert!(panels.timeline.expanded.contains(&7));
        } else {
            properties::render(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 0., 320., 600.),
                &mut panels,
                &state,
                &input,
                &mut actions,
            );
            assert!(!panels.properties.transform_collapsed);
        }
        assert!(timeline::property_visible(
            &panels,
            &state,
            7,
            Property::Position
        ));
        assert!(timeline::property_visible(
            &panels,
            &state,
            7,
            Property::Scale
        ));
        assert!(actions.is_empty());
    }
}

#[test]
fn timeline_transform_header_collapses_and_reopens_while_layer_stays_expanded() {
    let engine = engine();
    let state = state(&engine, 32.);
    let mut panels = panels();
    panels.timeline.expanded.insert(7);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let point = [120., 100.];
    let input = Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    };
    let mut actions = vec![];
    for collapsed in [true, false] {
        timeline::render(
            &mut PaintList::default(),
            &mut atlas,
            Rect::new(0., 0., 1200., 600.),
            &mut panels,
            &state,
            &input,
            &mut actions,
        );
        assert!(panels.timeline.expanded.contains(&7));
        assert_eq!(panels.timeline.collapsed_transforms.contains(&7), collapsed);
    }
    assert!(actions.is_empty());
    invoke(&mut panels, &state, Key::P, Modifiers::default());
    assert!(timeline::property_visible(
        &panels,
        &state,
        7,
        Property::Position
    ));
    assert!(!timeline::property_visible(
        &panels,
        &state,
        7,
        Property::Scale
    ));
}

#[test]
fn navigation_uses_visible_property_times_and_accumulates_repeated_keys() {
    let engine = engine();
    let state = state(&engine, 12.);
    let mut panels = panels();
    invoke(&mut panels, &state, Key::P, Modifiers::default());
    panels.timeline.zoom = 10.;
    let input = Input {
        keys: vec![
            (Key::K, Modifiers::default()),
            (Key::K, Modifiers::default()),
            (Key::J, Modifiers::default()),
        ],
        key_repeats: vec![false, true, false],
        ..Default::default()
    };
    let mut out = vec![];
    keyboard::handle_shortcuts(&input, &mut panels, &state, &mut out);
    assert!(matches!(
        out.as_slice(),
        [Action::Seek(32.), Action::Seek(52.), Action::Seek(32.)]
    ));
    assert!(panels.timeline.offset > 0.);
    assert!(panels.animation_controls.user_key().is_none());
}

#[test]
fn easy_ease_changes_the_selected_keys_two_sides_as_one_undoable_edit() {
    for (modifiers, incoming, outgoing) in [
        (Modifiers::default(), true, true),
        (
            Modifiers {
                shift: true,
                ..Default::default()
            },
            true,
            false,
        ),
        (
            Modifiers {
                control: true,
                shift: true,
                alt: false,
            },
            false,
            true,
        ),
    ] {
        let mut engine = engine();
        let before = engine.snapshot();
        let state = state(&engine, 32.);
        let mut panels = panels();
        panels
            .animation_controls
            .select_key(7, Property::Position, None, 32);
        let out = invoke(&mut panels, &state, Key::F9, modifiers);
        assert!(matches!(out.as_slice(), [Action::EditBatch(_)]));
        let revision = engine.revision();
        apply(&mut engine, out);
        assert_eq!(engine.revision(), revision + 1);
        let keys = &engine.project().layers[0].transform.position.keys;
        assert_eq!(
            keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
            [0, 20, 40]
        );
        assert_eq!(
            keys[0].value,
            before.layers[0].transform.position.keys[0].value
        );
        if incoming {
            assert!(keys[0].curve.unwrap().sample(1.).velocity.abs() < 1e-6);
        } else {
            assert!(keys[0].curve.is_none());
        }
        if outgoing {
            assert!(keys[1].curve.unwrap().sample(0.).velocity.abs() < 1e-6);
        } else {
            assert!(keys[1].curve.is_none());
        }
        assert!(keys[2].curve.is_none());
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }
}

#[test]
fn ease_does_not_edit_without_explicit_selection_or_a_locked_or_foreign_layer() {
    let mut engine = engine();
    let state0 = state(&engine, 32.);
    let mut panels = panels();
    assert!(invoke(&mut panels, &state0, Key::F9, Modifiers::default()).is_empty());
    panels
        .animation_controls
        .select_key(7, Property::Position, None, 32);
    panels.selected = None;
    assert!(invoke(&mut panels, &state0, Key::F9, Modifiers::default()).is_empty());
    panels.selected = Some(7);
    panels
        .animation_controls
        .select_key(7, Property::Position, None, 32);
    engine
        .apply(Command::Flags {
            object: 7,
            visible: true,
            locked: true,
        })
        .unwrap();
    assert!(invoke(
        &mut panels,
        &state(&engine, 32.),
        Key::F9,
        Modifiers::default()
    )
    .is_empty());
    assert!(invoke(
        &mut panels,
        &state(&engine, 32.),
        Key::Delete,
        Modifiers::default()
    )
    .is_empty());
}

#[test]
fn one_sided_easing_preserves_the_opposite_bezier_handle() {
    let mut engine = engine();
    let curve = Curve {
        space: CurveSpace::Progress,
        shape: CurveShape::Cubic {
            control1: [0.25, 0.3],
            control2: [0.6, 0.8],
            start: 0.,
            end: 1.,
        },
    };
    engine
        .apply(Command::Curve {
            object: 7,
            property: Property::Position,
            axis: None,
            frame: 12,
            easing: Easing {
                ease: Ease::Linear,
                curve: Some(curve),
            },
        })
        .unwrap();
    let mut panels = panels();
    panels
        .animation_controls
        .select_key(7, Property::Position, None, 32);
    let out = invoke(
        &mut panels,
        &state(&engine, 32.),
        Key::F9,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    apply(&mut engine, out);
    let result = engine.project().layers[0].transform.position.keys[0]
        .curve
        .unwrap();
    assert!(matches!(
        result.shape,
        CurveShape::Cubic {
            control1: [0.25, 0.3],
            control2: [0.6, 1.],
            ..
        }
    ));
    assert!((result.sample(0.).velocity - curve.sample(0.).velocity).abs() < 1e-6);
    assert!(result.sample(1.).velocity.abs() < 1e-6);
}

#[test]
fn toggle_and_delete_keys_preserve_other_frames_and_do_not_disable_the_track() {
    let mut engine = engine();
    let before = engine.snapshot();
    let mut panels = panels();
    let mods = Modifiers {
        alt: true,
        shift: true,
        control: false,
    };
    let out = invoke(&mut panels, &state(&engine, 32.), Key::P, mods);
    apply(&mut engine, out);
    assert_eq!(
        engine.project().layers[0]
            .transform
            .position
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [0, 40]
    );
    let out = invoke(&mut panels, &state(&engine, 32.), Key::P, mods);
    apply(&mut engine, out);
    assert_eq!(
        engine.project().layers[0].transform.position.keys[1].value,
        [100., 50., 20.]
    );
    assert_eq!(engine.project().layers[0].transform.position.keys.len(), 3);
    panels
        .animation_controls
        .select_key(7, Property::Position, None, 32);
    let out = invoke(
        &mut panels,
        &state(&engine, 32.),
        Key::Delete,
        Modifiers::default(),
    );
    assert!(matches!(
        out.as_slice(),
        [Action::Edit(Command::DeleteKey {
            object: 7,
            frame: 32,
            ..
        })]
    ));
    apply(&mut engine, out);
    assert_eq!(engine.project().layers.len(), before.layers.len());
    assert!(panels.animation_controls.user_key().is_none());
    panels.timeline.graph_mode = true;
    assert!(invoke(
        &mut panels,
        &state(&engine, 32.),
        Key::Delete,
        Modifiers::default()
    )
    .is_empty());
}

#[test]
fn separated_axis_easing_and_value_sampling_use_composition_offsets() {
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
    assert_eq!(
        animation_data::sample(&state, 7, Property::Position, Some(Axis::Y), 22.)
            .unwrap()
            .as_f64(),
        Some(25.)
    );
    panels
        .animation_controls
        .select_key(7, Property::Position, Some(Axis::X), 32);
    apply(
        &mut engine,
        invoke(&mut panels, &state, Key::F9, Modifiers::default()),
    );
    let axes = engine.project().layers[0]
        .transform
        .position
        .axes
        .as_ref()
        .unwrap();
    let old = before.layers[0].transform.position.axes.as_ref().unwrap();
    assert_eq!(axes.y, old.y);
    assert_eq!(axes.z, old.z);
    assert!(axes.x.keys[1].curve.is_some());
}

#[test]
fn graph_toggle_preserves_layer_property_playhead_and_has_no_vertical_grid() {
    let engine = engine();
    let state = state(&engine, 32.);
    let mut panels = panels();
    panels.timeline.expanded.insert(7);
    panels
        .animation_controls
        .select_key(7, Property::Position, None, 32);
    let selected = panels.animation_controls.user_key();
    assert!(invoke(
        &mut panels,
        &state,
        Key::F3,
        Modifiers {
            shift: true,
            ..Default::default()
        }
    )
    .is_empty());
    assert!(panels.timeline.graph_mode);
    assert_eq!(panels.selected, Some(7));
    assert_eq!(panels.animation_controls.user_key(), selected);
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let mut paint = PaintList::default();
    let mut out = vec![];
    let area = Rect::new(0., 0., 1200., 600.);
    timeline_graph::render(
        &mut paint,
        &mut atlas,
        area,
        &mut panels,
        &state,
        &Input::default(),
        0.,
        10.,
        &mut out,
    );
    assert!(out.is_empty());
    assert!(panels.timeline.expanded.contains(&7));
    assert!(paint
        .shapes
        .iter()
        .any(|s| matches!(s,motion_ui::paint::Shape::Line {points} if points.len()>100)));
    assert!(!paint.shapes.iter().any(|s|matches!(s,motion_ui::paint::Shape::Line {points} if points.len()==2 && points[0][0]==points[1][0] && points[0][1]!=points[1][1])));
    invoke(
        &mut panels,
        &state,
        Key::F3,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert!(!panels.timeline.graph_mode);
    assert_eq!(panels.animation_controls.user_key(), selected);
}

#[test]
fn clicking_a_value_graph_key_selects_its_channel_without_editing_the_layer() {
    let engine = engine();
    let state = state(&engine, 12.);
    let mut panels = panels();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let mut out = vec![];
    let point = [320., 302.];
    timeline_graph::render(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 0., 1200., 600.),
        &mut panels,
        &state,
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
        0.,
        10.,
        &mut out,
    );
    assert!(matches!(out.as_slice(), [Action::Seek(32.)]));
    assert_eq!(
        panels.animation_controls.user_key(),
        Some(types::KeySelection {
            object: 7,
            property: Property::Position,
            axis: None,
            frame: 32
        })
    );
    out.clear();
    let point = [900., 100.];
    timeline_graph::render(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 0., 1200., 600.),
        &mut panels,
        &state,
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
        0.,
        10.,
        &mut out,
    );
    assert!(out.is_empty());
    assert!(panels.animation_controls.user_key().is_none());
}

#[test]
fn fields_search_ime_and_drag_own_animation_shortcuts() {
    let engine = engine();
    let state = state(&engine, 32.);
    for input in [
        Input {
            composing: true,
            ..Default::default()
        },
        Input {
            ime_consumed: true,
            ..Default::default()
        },
    ] {
        let mut panels = panels();
        let mut input = input;
        input.keys = vec![(Key::P, Modifiers::default())];
        let mut out = vec![];
        keyboard::route(&input, &mut panels, &state, false, None, &mut out);
        assert!(panels.timeline.filters.is_empty());
        assert!(out.is_empty());
    }
    let mut panels = panels();
    panels.timeline.timeline_search = true;
    invoke(&mut panels, &state, Key::P, Modifiers::default());
    assert!(panels.timeline.filters.is_empty());
    panels.timeline.timeline_search = false;
    panels.timeline.scrubbing = true;
    invoke(
        &mut panels,
        &state,
        Key::F3,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert!(!panels.timeline.graph_mode);
}
