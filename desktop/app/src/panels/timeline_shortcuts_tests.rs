use super::*;
use motion_core::{
    Axis, Curve, CurveShape, CurveSpace, Ease, Easing, Engine, Keyframe, Layer, Project,
};
use motion_ui::input::Modifiers;
mod property_context_tests;
mod reveal_axis_tests;
mod stale_feedback_tests;

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
    serde_json::json!({"root":"shortcuts-fixture","composition":motion_core::MAIN_COMPOSITION,"document_generation":1,"revision":engine.revision(),"frame":frame,"project":engine.project(),"timeline_layers":engine.project().timeline_layers(frame).unwrap()})
}
fn panels() -> Panels {
    Panels {
        selected: Some(7),
        focused: Some(Panel::Timeline),
        ..Default::default()
    }
}

#[test]
fn custom_property_reveal_and_navigation_use_new_chords_and_keep_text_owner_priority() {
    use crate::shortcuts::{bindings::Bindings,Command as Shortcut,Scope};
    let e=engine();let state=state(&e,32.);
    for locale in [Locale::En,Locale::Zh] {
        for host in [Panel::Timeline,Panel::Properties] {
            let mut p=panels();p.locale=locale;p.focused=Some(host);
            let mut bindings=Bindings::default();
            bindings.bind(Scope::Animation,Shortcut::Reveal(Property::Position),Some(motion_ui::input::KeyChord::new(Key::Q,false,false,false))).unwrap();
            bindings.bind(Scope::Animation,Shortcut::NextKey,Some(motion_ui::input::KeyChord::new(Key::E,true,false,false))).unwrap();
            p.shortcut_bindings=std::sync::Arc::new(bindings);
            assert!(invoke(&mut p,&state,Key::P,Modifiers::default()).is_empty());
            assert!(!p.timeline.filters.contains_key(&7));
            assert!(invoke(&mut p,&state,Key::Q,Modifiers::default()).is_empty());
            let rows=timeline::filtered_properties(&state,7,p.timeline.filters[&7]);
            assert_eq!(rows.len(),1);assert_eq!(rows[0].property,Property::Position);
            assert!(invoke(&mut p,&state,Key::K,Modifiers::default()).is_empty());
            assert!(matches!(invoke(&mut p,&state,Key::E,Modifiers {control:true,..Default::default()}).as_slice(),[Action::Seek(frame)] if *frame==52.));
            let before=p.timeline.filters.clone();p.timeline.timeline_search=true;
            assert!(invoke(&mut p,&state,Key::Q,Modifiers::default()).is_empty());assert_eq!(p.timeline.filters,before);
        }
    }
}

#[test]
fn custom_animated_property_chords_preserve_double_press_and_input_ownership() {
    use crate::shortcuts::{bindings::Bindings, Command as Shortcut, Scope};
    use motion_ui::input::KeyChord;
    let engine = engine();
    let history = (engine.can_undo(), engine.can_redo());
    let state = state(&engine, 32.);
    for locale in [Locale::En, Locale::Zh] {
        for host in [Panel::Timeline, Panel::Properties, Panel::Curves, Panel::Composition] {
            for chord in [KeyChord::new(Key::Q, false, false, false),
                KeyChord::new(Key::F4, true, false, false)] {
                let mut panels = panels();
                panels.locale = locale;
                panels.focused = Some(host);
                let mut bindings = Bindings::default();
                bindings.bind(Scope::Animation, Shortcut::AnimatedProperties, Some(chord)).unwrap();
                panels.shortcut_bindings = std::sync::Arc::new(bindings);
                assert!(invoke(&mut panels, &state, Key::U, Modifiers::default()).is_empty());
                assert!(!panels.timeline.filters.contains_key(&7));
                assert!(invoke(&mut panels, &state, chord.key, chord.modifiers).is_empty());
                assert_eq!(panels.timeline.filters[&7], types::PropertyFilter::Animated);
                assert!(invoke(&mut panels, &state, chord.key, chord.modifiers).is_empty());
                assert_eq!(panels.timeline.filters[&7], types::PropertyFilter::Modified,
                    "the effective animated-property chord must retain UU semantics");

                // An autorepeat interrupts a pair, as it does for the default U.
                let mut out = Vec::new();
                keyboard::route(&Input {
                    keys: vec![(chord.key, chord.modifiers)],
                    key_repeats: vec![true],
                    ..Default::default()
                }, &mut panels, &state, false, None, &mut out);
                assert!(out.is_empty());
                assert!(invoke(&mut panels, &state, chord.key, chord.modifiers).is_empty());
                assert_eq!(panels.timeline.filters[&7], types::PropertyFilter::Animated);

                // A text owner consumes this chord and cancels the pending pair.
                panels.project.search = true;
                assert!(invoke(&mut panels, &state, chord.key, chord.modifiers).is_empty());
                assert_eq!(panels.timeline.filters[&7], types::PropertyFilter::Animated);
                panels.project.search = false;
                assert!(invoke(&mut panels, &state, chord.key, chord.modifiers).is_empty());
                assert_eq!(panels.timeline.filters[&7], types::PropertyFilter::Properties(0),
                    "a fresh single press toggles the existing animated filter closed");
                assert!(invoke(&mut panels, &state, chord.key, chord.modifiers).is_empty());
                assert_eq!(panels.timeline.filters[&7], types::PropertyFilter::Modified);
                assert_eq!((engine.can_undo(), engine.can_redo()), history);
            }
        }
    }
}

#[test]
fn custom_select_all_routes_the_graph_selection_and_disables_its_old_chord() {
    use crate::shortcuts::{bindings::Bindings,Command as Shortcut,Scope};
    let engine=engine();let history=(engine.can_undo(),engine.can_redo());let state=state(&engine,32.);let mut panels=panels();
    panels.timeline.set_graph_mode(true);
    let mut bindings=Bindings::default();bindings.bind(Scope::Application,Shortcut::SelectAll,
        Some(motion_ui::input::KeyChord::new(Key::B,true,false,false))).unwrap();
    panels.shortcut_bindings=std::sync::Arc::new(bindings);
    let control=Modifiers {control:true,..Default::default()};
    assert!(invoke(&mut panels,&state,Key::A,control).is_empty());
    assert!(!panels.timeline.graph_box.has_selection());
    assert!(invoke(&mut panels,&state,Key::B,control).is_empty());
    assert!(panels.timeline.graph_box.has_selection());
    assert_eq!((engine.can_undo(),engine.can_redo()),history,"selection must retain existing project history");
}
#[test]
fn a_reveals_the_real_layer_anchor_in_both_locales_and_property_hosts() {
    let mut e=engine();
    e.apply(Command::Animate{object:7,property:Property::Anchor,axis:None,frame:12,enabled:true}).unwrap();
    e.apply(Command::SetVector{object:7,property:Property::Anchor,frame:52,value:[0.1,0.9,0.]}).unwrap();
    for locale in [Locale::Zh,Locale::En] {
        for host in [Panel::Timeline,Panel::Properties] {
            let mut p=panels();p.locale=locale;p.focused=Some(host);
            let s=state(&e,32.);
            assert!(invoke(&mut p,&s,Key::A,Modifiers::default()).is_empty());
            let rows=timeline::filtered_properties(&s,7,p.timeline.filters[&7]);
            assert_eq!(rows.len(),1);assert_eq!(rows[0].property,Property::Anchor);
            assert_eq!(rows[0].keys,vec![12.,52.]);
            assert_eq!(p.animation_controls.property,Some(Property::Anchor));
            let sampled=animation_data::sample(&s,7,Property::Anchor,None,32.).unwrap();
            assert!((sampled[0].as_f64().unwrap()-0.3).abs()<1e-6);
            assert_eq!(animation_data::display_scale(&s,7,Property::Anchor,0),100.);
            assert!(invoke(&mut p,&s,Key::A,Modifiers::default()).is_empty());
            assert!(!p.timeline.expanded.contains(&7));
            let camera=serde_json::json!({"project":{"layers":[motion_core::Layer::camera(7,"Camera",640,480)]}});
            assert!(timeline::property_rows(&camera,7).iter().all(|r|r.property!=Property::Anchor));
        }
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
    // This existing scenario covers a later independent single U, rather than
    // the newly supported immediate UU pair. Keep its collapse assertion.
    panels.property_reveal.interrupt();
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
            [81., timeline::header_height(&PaintList::default().styles()) + 12.]
        } else {
            // The independent Masks header precedes Transform in the inspector.
            [60., 63.]
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
    // The width-aware helper now retains a centerline until GPU expansion,
    // rather than eagerly storing six raw triangle vertices per segment.
    // Check visible samples and the actual style width, not that old storage.
    assert!(paint
        .shapes
        .iter()
        .zip(&paint.colors)
        .any(|(shape, color)| *color == paint.styles().color("curve")
            && matches!(shape, motion_ui::paint::Shape::Stroke {points, width, clip: Some(clip)}
                if *width == paint.styles().metric("graph.curve_width", 2.)
                    && area.contains_rect(*clip)
                    && points.iter().all(|p| p.iter().all(|v| v.is_finite()))
                    && points.iter().filter(|p| clip.contains(**p)).count() > 100)));
    assert!(!paint.shapes.iter().any(|s| matches!(s,
        motion_ui::paint::Shape::Line {points, ..}
        | motion_ui::paint::Shape::Stroke {points, ..}
        if points.len()==2 && points[0][0]==points[1][0] && points[0][1]!=points[1][1])));
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
    let point = [320., 281.];
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
    assert!(matches!(out.as_slice(), [Action::Seek(32.)]), "{out:?}");
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

fn apply_graph_actions(engine: &mut Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => engine.apply(command).unwrap(),
            Action::EditBatch(commands) => { engine.apply_batch(commands).unwrap(); },
            Action::History(2) => engine.begin_gesture().unwrap(),
            Action::History(3) => engine.end_gesture(true).unwrap(),
            Action::History(4) => engine.end_gesture(false).unwrap(),
            Action::Seek(_) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}
#[test]
fn graph_axis_drag_preserves_other_axes_and_moves_one_undoable_key() {
    let mut engine = engine();
    let before = engine.project().clone();
    let snapshot = state(&engine, 32.);
    let mut panels = panels();
    let source = animation_data::channel(&snapshot, 7, Property::Position, None).unwrap();
    let press = Input {
        mouse: [320., 100.],
        pressed: Some((MouseButton::Left, [320., 100.])),
        ..Default::default()
    };
    let mut actions = vec![];
    graph_keys::begin(
        &mut panels,
        &snapshot,
        7,
        Property::Position,
        None,
        1,
        source,
        32,
        [320., 100.],
        50.,
        [0., 200.],
        &press,
        &mut actions,
    );
    assert!(actions.is_empty());
    let held = Input {
        mouse: [340., 80.],
        held: Some(MouseButton::Left),
        ..Default::default()
    };
    graph_keys::update(
        &mut panels,
        &snapshot,
        Rect::new(0., 0., 600., 200.),
        10.,
        1.,
        source,
        &held,
        &mut actions,
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::History(2)))
            .count(),
        1
    );
    apply_graph_actions(&mut engine, std::mem::take(&mut actions));
    let snapshot = state(&engine, 34.);
    let source = animation_data::channel(&snapshot, 7, Property::Position, None).unwrap();
    graph_keys::update(
        &mut panels,
        &snapshot,
        Rect::new(0., 0., 600., 200.),
        10.,
        1.,
        source,
        &held,
        &mut actions,
    );
    assert!(
        actions.is_empty(),
        "holding still must leave room for presentation"
    );
    let release = Input {
        mouse: held.mouse,
        released: Some((MouseButton::Left, held.mouse)),
        ..Default::default()
    };
    graph_keys::update(
        &mut panels,
        &snapshot,
        Rect::new(0., 0., 600., 200.),
        10.,
        1.,
        source,
        &release,
        &mut actions,
    );
    apply_graph_actions(&mut engine, actions);
    let track = &engine.project().layers[0].transform.position;
    assert_eq!(track.keys[1].frame, 22); // composition frame 34, layer offset 12
    assert_eq!(track.keys[1].value, [100., 70., 20.]);
    assert!(track.axes.is_none());
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}
#[test]
fn graph_drag_return_to_origin_then_cancel_restores_engine_bytes() {
    let mut engine = engine();
    let before = serde_json::to_vec(engine.project()).unwrap();
    let snapshot = state(&engine, 32.);
    let mut panels = panels();
    let source = animation_data::channel(&snapshot, 7, Property::Position, None).unwrap();
    let mut actions = vec![];
    let press = Input {
        mouse: [320., 100.],
        pressed: Some((MouseButton::Left, [320., 100.])),
        ..Default::default()
    };
    graph_keys::begin(
        &mut panels,
        &snapshot,
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
    for mouse in [[320., 80.], [320., 100.]] {
        let snapshot = state(&engine, 32.);
        let source = animation_data::channel(&snapshot, 7, Property::Position, None).unwrap();
        graph_keys::update(
            &mut panels,
            &snapshot,
            Rect::new(0., 0., 600., 200.),
            10.,
            1.,
            source,
            &Input {
                mouse,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            &mut actions,
        );
        apply_graph_actions(&mut engine, std::mem::take(&mut actions));
    }
    assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
    let snapshot = state(&engine, 32.);
    let source = animation_data::channel(&snapshot, 7, Property::Position, None).unwrap();
    graph_keys::update(
        &mut panels,
        &snapshot,
        Rect::new(0., 0., 600., 200.),
        10.,
        1.,
        source,
        &Input {
            keys: vec![(Key::Escape, Modifiers::default())],
            ..Default::default()
        },
        &mut actions,
    );
    apply_graph_actions(&mut engine, actions);
    assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
    assert!(panels.timeline.key_drag.is_none());
}

#[test]
fn graph_property_tree_selects_y_without_separating_or_editing_the_track() {
    let engine = engine();
    let snapshot = state(&engine, 32.);
    let before = engine.snapshot();
    let mut panels = panels();
    panels.timeline.expanded.insert(7);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut actions = vec![];
    let point = [405., timeline::header_height(&PaintList::default().styles()) + 77.];
    graph_workspace::render(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 0., 1200., 600.),
        &mut panels,
        &snapshot,
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
        &mut actions,
    );
    assert!(actions.is_empty());
    assert_eq!(panels.selected, Some(7));
    assert_eq!(panels.timeline.graph_component, 1);
    assert_eq!(panels.animation_controls.property, Some(Property::Position));
    assert_eq!(panels.animation_controls.axis, None);
    assert_eq!(engine.snapshot(), before);
}

#[test]
fn visible_bezier_handles_edit_the_curve_without_moving_keyframes() {
    let mut engine = engine();
    let before = engine.project().clone();
    let snapshot = state(&engine, 32.);
    let mut panels = panels();
    panels
        .animation_controls
        .select_key(7, Property::Position, None, 32);
    let source = animation_data::channel(&snapshot, 7, Property::Position, None).unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut list = PaintList::default();
    let mut actions = vec![];
    let plot = graph_handles::Plot {
        area: Rect::new(0., 0., 600., 200.),
        offset: 12.,
        ppf: 10.,
        low: 0.,
        high: 200.,
        speed: false,
        fps: 30.,
        scale: 1.,
    };
    graph_handles::draw(
        &mut list,
        &mut atlas,
        &mut panels,
        source,
        7,
        Property::Position,
        None,
        0,
        plot,
        &Input::default(),
        false,
        &mut actions,
    );
    let circles=list.shapes.iter().filter(|shape|matches!(shape,motion_ui::paint::Shape::Procedural{kind:motion_ui::paint::Pattern::Box{radius,..},..}if *radius==4.)).count();
    assert_eq!(
        circles, 4,
        "both handles of each adjacent segment must be visible"
    );
    assert!(actions.is_empty());
    let point = [200. / 3. * 2., 200. - 100. / 3. * 2.];
    graph_handles::draw(
        &mut PaintList::default(),
        &mut atlas,
        &mut panels,
        source,
        7,
        Property::Position,
        None,
        0,
        plot,
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
        false,
        &mut actions,
    );
    assert!(actions.is_empty());
    assert!(panels.timeline.graph_drag.is_some());
    let held = Input {
        mouse: [160., 120.],
        held: Some(MouseButton::Left),
        modifiers: Modifiers {
            alt: true,
            ..Default::default()
        },
        ..Default::default()
    };
    graph_handles::draw(
        &mut PaintList::default(),
        &mut atlas,
        &mut panels,
        source,
        7,
        Property::Position,
        None,
        0,
        plot,
        &held,
        false,
        &mut actions,
    );
    apply_graph_actions(&mut engine, std::mem::take(&mut actions));
    let snapshot = state(&engine, 32.);
    let source = animation_data::channel(&snapshot, 7, Property::Position, None).unwrap();
    graph_handles::draw(
        &mut PaintList::default(),
        &mut atlas,
        &mut panels,
        source,
        7,
        Property::Position,
        None,
        0,
        plot,
        &held,
        false,
        &mut actions,
    );
    assert!(
        actions.is_empty(),
        "stationary handle must not resubmit an edit"
    );
    graph_handles::draw(
        &mut PaintList::default(),
        &mut atlas,
        &mut panels,
        source,
        7,
        Property::Position,
        None,
        0,
        plot,
        &Input {
            mouse: held.mouse,
            released: Some((MouseButton::Left, held.mouse)),
            ..Default::default()
        },
        false,
        &mut actions,
    );
    apply_graph_actions(&mut engine, actions);
    let actual = &engine.project().layers[0].transform.position.keys;
    let original = &before.layers[0].transform.position.keys;
    assert_eq!(
        actual
            .iter()
            .map(|k| (k.frame, k.value))
            .collect::<Vec<_>>(),
        original
            .iter()
            .map(|k| (k.frame, k.value))
            .collect::<Vec<_>>()
    );
    assert_ne!(actual[0].curve, original[0].curve);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn linked_graph_button_and_shortcut_restore_both_views_and_share_key_selection() {
    let engine = engine();
    let snapshot = state(&engine, 32.);
    let before = engine.snapshot();
    let mut panels = panels();
    panels.timeline.expanded.insert(7);
    panels.timeline.zoom = 4.;
    panels.timeline.offset = 18.;
    panels.timeline.scroll = 11.;
    panels
        .animation_controls
        .select_key(7, Property::Position, None, 32);
    let selected = panels.animation_controls.user_key();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut actions = vec![];
    let enter = [570., 17.];
    timeline::render(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 0., 1200., 180.),
        &mut panels,
        &snapshot,
        &Input {
            mouse: enter,
            pressed: Some((MouseButton::Left, enter)),
            ..Default::default()
        },
        &mut actions,
    );
    assert!(panels.timeline.graph_mode);
    assert!(actions.is_empty());
    panels.timeline.zoom = 7.;
    panels.timeline.offset = 8.;
    panels.timeline.scroll = 5.;
    let exit = enter;
    timeline::render(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 0., 1200., 180.),
        &mut panels,
        &snapshot,
        &Input {
            mouse: exit,
            pressed: Some((MouseButton::Left, exit)),
            ..Default::default()
        },
        &mut actions,
    );
    assert!(!panels.timeline.graph_mode);
    assert!(actions.is_empty());
    assert_eq!(
        (
            panels.timeline.zoom,
            panels.timeline.offset,
            panels.timeline.scroll
        ),
        (4., 18., 11.)
    );
    assert!(invoke(
        &mut panels,
        &snapshot,
        Key::F3,
        Modifiers {
            shift: true,
            ..Default::default()
        }
    )
    .is_empty());
    assert!(panels.timeline.graph_mode);
    assert_eq!(
        (
            panels.timeline.zoom,
            panels.timeline.offset,
            panels.timeline.scroll
        ),
        (7., 8., 5.)
    );
    assert_eq!(panels.animation_controls.user_key(), selected);
    assert_eq!(panels.selected, Some(7));
    assert!(panels.timeline.expanded.contains(&7));
    assert_eq!(engine.snapshot(), before);
}

fn curve_box_draw(panels: &mut Panels, engine: &Engine, atlas: &mut TextAtlas, input: Input) -> Vec<Action> {
    let mut actions=vec![];
    timeline_graph::render(&mut PaintList::default(),atlas,Rect::new(0.,0.,1200.,600.),
        panels,&state(engine,32.),&input,0.,10.,&mut actions);
    actions
}
fn curve_pointer(point:[f32;2],pressed:bool,released:bool) -> Input {
    Input { mouse:point,pressed:pressed.then_some((MouseButton::Left,point)),
        held:(!released).then_some(MouseButton::Left),released:released.then_some((MouseButton::Left,point)),
        ..Default::default() }
}
#[test]
fn three_dimensional_layer_anchor_graph_has_only_two_pixel_components() {
    let mut project = engine().snapshot();
    project.layers[0].size = [200., 100.];
    let mut engine = Engine::new(project).unwrap();
    engine.apply(Command::Animate {
        object: 7, property: Property::Anchor, axis: None, frame: 12, enabled: true,
    }).unwrap();
    engine.apply(Command::SetVector {
        object: 7, property: Property::Anchor, frame: 52, value: [0.2, 0.8, 0.],
    }).unwrap();
    let before = engine.snapshot();
    let mut panels = panels();
    panels.timeline.graph_mode = true;
    panels.timeline.graph_component = 2;
    panels.animation_controls.choose_track(7, Property::Anchor, None);
    let snapshot = state(&engine, 32.);
    assert_eq!(animation_data::display_scale(&snapshot, 7, Property::Anchor, 0), 200.);
    assert_eq!(animation_data::display_scale(&snapshot, 7, Property::Anchor, 1), 100.);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    assert!(curve_box_draw(&mut panels, &engine, &mut atlas, Input::default()).is_empty());
    assert_eq!(panels.timeline.graph_component, 0);
    panels.timeline.graph_component = 1;
    assert!(curve_box_draw(&mut panels, &engine, &mut atlas, Input::default()).is_empty());
    assert_eq!(panels.timeline.graph_component, 1);
    assert_eq!(engine.project(), &before);
}
#[test]
fn curve_selection_box_translates_and_scales_as_one_undo_without_changing_other_axes() {
    for resize in [false,true] {
        let mut engine=engine();
        let before=engine.project().clone();
        let mut panels=panels();
        panels.timeline.graph_mode=true;
        let mut atlas=TextAtlas::from_system(12.).unwrap();
        let select=[220.,384.75];
        let actions=curve_box_draw(&mut panels,&engine,&mut atlas,curve_pointer(select,true,false));
        assert!(actions.is_empty(),"{actions:?}");
        assert!(panels.timeline.graph_box.has_selection());
        assert_eq!(engine.project(),&before);
        let start=if resize { [520.,70.5] } else { select };
        let finish=if resize { [620.,29.] } else { [320.,364.75] };
        let actions=curve_box_draw(&mut panels,&engine,&mut atlas,curve_pointer(start,true,false));
        assert!(actions.is_empty(),"{actions:?}");
        let actions=curve_box_draw(&mut panels,&engine,&mut atlas,curve_pointer(finish,false,true));
        assert!(matches!(actions.as_slice(),[Action::History(2),Action::EditBatch(_),Action::History(3)]),"{actions:?}");
        apply_graph_actions(&mut engine,actions);
        let keys=&engine.project().layers[0].transform.position.keys;
        let times=keys.iter().map(|key|key.frame).collect::<Vec<_>>();
        assert_eq!(times,if resize { vec![0,25,50] } else { vec![10,30,50] });
        for (i,key) in keys.iter().enumerate() {
            assert_eq!(key.value[1..],before.layers[0].transform.position.keys[i].value[1..]);
            assert_eq!(key.ease,before.layers[0].transform.position.keys[i].ease);
        }
        assert!((keys[1].value[0]-(if resize {110.} else {109.63855})).abs()<0.0001);
        engine.undo().unwrap();
        assert_eq!(engine.project(),&before);
    }
}
#[test]
fn curve_selection_box_keeps_its_mapping_during_drag_and_cancels_on_escape_or_focus_loss() {
    for focus_lost in [false,true] {
        let mut engine=engine();
        let before=engine.project().clone();
        let mut panels=panels();
        panels.timeline.graph_mode=true;
        let mut atlas=TextAtlas::from_system(12.).unwrap();
        let start=[220.,384.75];
        for input in [curve_pointer(start,true,false),curve_pointer(start,true,false),
            curve_pointer([320.,364.75],false,false)] {
            let actions=curve_box_draw(&mut panels,&engine,&mut atlas,input);
            apply_graph_actions(&mut engine,actions);
        }
        assert_ne!(engine.project(),&before);
        let stationary=curve_box_draw(&mut panels,&engine,&mut atlas,curve_pointer([320.,364.75],false,false));
        assert!(stationary.is_empty(),"{stationary:?}");
        let actions=curve_box_draw(&mut panels,&engine,&mut atlas,Input {
            focus_lost, keys:if focus_lost {vec![]} else {vec![(Key::Escape,Modifiers::default())]},
            ..Default::default()
        });
        assert!(matches!(actions.as_slice(),[Action::History(4)]),"{actions:?}");
        apply_graph_actions(&mut engine,actions);
        assert_eq!(engine.project(),&before);
        assert!(!panels.timeline.graph_box.editing());
    }
}
#[test]
fn curve_selection_box_delete_owns_keys_only_in_the_focused_timeline() {
    let mut engine=engine();
    let before=engine.project().clone();
    let mut panels=panels();
    panels.timeline.graph_mode=true;
    let mut atlas=TextAtlas::from_system(12.).unwrap();
    assert!(curve_box_draw(&mut panels,&engine,&mut atlas,curve_pointer([220.,384.75],true,false)).is_empty());
    panels.focused=Some(Panel::Properties);
    assert!(!graph_box::delete(&mut panels,&state(&engine,32.),&mut vec![]));
    panels.focused=Some(Panel::Timeline);
    let mut actions=vec![];
    assert!(graph_box::delete(&mut panels,&state(&engine,32.),&mut actions));
    apply_graph_actions(&mut engine,actions);
    assert_eq!(engine.project().layers.len(),1);
    assert!(engine.project().layers[0].transform.position.keys.is_empty());
    engine.undo().unwrap();
    assert_eq!(engine.project(),&before);
}
#[test]
fn active_numeric_text_can_start_a_new_drag_in_main_and_floating_timeline_and_graph() {
    for floating in [false,true] {
        for graph in [false,true] {
            let mut engine=engine();
            let before=engine.project().clone();
            let mut panels=panels();
            panels.timeline.expanded.insert(7);
            panels.timeline.graph_mode=graph;
            let mut atlas=TextAtlas::from_system(12.).unwrap();
            let size=[1000.,700.];
            let layout=Node::dock(Panel::Timeline.dock());
            let area=if floating {Rect::new(0.,30.,1000.,648.)} else {frames(&layout,size)[0].content};
            let gutter=Rect::new(area.min[0],area.min[1],GUTTER.min(area.width()*0.52),area.height());
            let styles=PaintList::default().styles();
            let field=timeline::property_layout(&styles,gutter,area.min[1]+timeline::header_height(&styles)+66.,22.,0,3).number;
            let point=[field.min[0]+8.,field.min[1]+10.];
            let mut draw=|panels:&mut Panels,engine:&Engine,input:Input| {
                let mut state=state(engine,32.);
                let layer=&engine.project().layers[0];
                state["sampledLayers"]=serde_json::json!([{"id":7,
                    "position":layer.transform.position.sample(20.).unwrap(),
                    "scale":layer.transform.scale.sample(20.).unwrap(),
                    "rotation":layer.transform.rotation.sample(20.).unwrap(),
                    "opacity":layer.transform.opacity.sample(20.).unwrap()}]);
                if floating {floating_chrome(&mut PaintList::default(),&mut atlas,Panel::Timeline,size,panels,&state,&input)}
                else {chrome(&mut PaintList::default(),&mut atlas,size,&layout,panels,&state,&input)}
            };
            for input in [curve_pointer(point,true,false),curve_pointer(point,false,true)] {
                let actions=draw(&mut panels,&engine,input);
                assert!(actions.is_empty(),"{actions:?}");
            }
            assert!(panels.properties.field.as_ref().unwrap().scrubbing.is_none());
            let actions=draw(&mut panels,&engine,curve_pointer(point,true,false));
            assert!(actions.is_empty());
            let actions=draw(&mut panels,&engine,curve_pointer([point[0]+40.,point[1]],false,false));
            apply_graph_actions(&mut engine,actions);
            let actions=draw(&mut panels,&engine,curve_pointer([point[0]+60.,point[1]],false,true));
            apply_graph_actions(&mut engine,actions);
            let value=engine.project().layers[0].transform.position.sample(20.).unwrap();
            assert_eq!(value,[130.,50.,20.]);
            assert!(panels.properties.field.is_none());
            engine.undo().unwrap();
            assert_eq!(engine.project(),&before);
        }
    }
}

#[test]
fn graph_select_all_uses_keys_and_never_selects_layers_or_changes_project_history() {
    let engine=engine();
    let mut panels=panels();
    panels.timeline.graph_mode=true;
    let before=engine.project().clone();
    let actions=invoke(&mut panels,&state(&engine,32.),Key::A,Modifiers {control:true,..Default::default()});
    assert!(actions.is_empty(),"{actions:?}");
    assert!(panels.timeline.graph_box.has_selection());
    assert_eq!(panels.selected,Some(7));
    assert_eq!(engine.project(),&before);
}

#[test]
fn shared_graph_wheel_keeps_mouse_time_and_separates_time_and_value_scaling() {
    let engine=engine();
    let snapshot=state(&engine,32.);
    let mut panels=panels();
    panels.timeline.graph_mode=true;
    panels.timeline.expanded.insert(7);
    panels.timeline.zoom=4.;
    panels.timeline.offset=18.;
    let initial_y=panels.timeline.graph_y_zoom;
    let mut atlas=TextAtlas::from_system(12.).unwrap();
    let area=Rect::new(0.,0.,1200.,600.);
    let mouse=[900.,200.];
    let gutter=GUTTER.min(area.width()*0.52);
    let width=area.width()-gutter-8.;
    let anchor=panels.timeline.offset+(mouse[0]-gutter)/width*120./panels.timeline.zoom;
    let mut actions=vec![];
    timeline::render(&mut PaintList::default(),&mut atlas,area,&mut panels,&snapshot,
        &Input {mouse,wheel:[0.,-20.],modifiers:Modifiers {control:true,..Default::default()},..Default::default()},&mut actions);
    let after=panels.timeline.offset+(mouse[0]-gutter)/width*120./panels.timeline.zoom;
    assert!((after-anchor).abs()<0.0001);
    assert_eq!(panels.timeline.graph_y_zoom,initial_y);
    assert!(actions.is_empty());
    let time=(panels.timeline.zoom,panels.timeline.offset);
    timeline::render(&mut PaintList::default(),&mut atlas,area,&mut panels,&snapshot,
        &Input {mouse,wheel:[0.,-20.],modifiers:Modifiers {control:true,alt:true,..Default::default()},..Default::default()},&mut actions);
    assert_eq!((panels.timeline.zoom,panels.timeline.offset),time);
    assert!(panels.timeline.graph_y_zoom>1.);
    let vertical=panels.timeline.graph_y_zoom;
    timeline::render(&mut PaintList::default(),&mut atlas,area,&mut panels,&snapshot,
        &Input {mouse,wheel:[0.,f32::NAN],modifiers:Modifiers {control:true,alt:true,..Default::default()},..Default::default()},&mut actions);
    assert_eq!((panels.timeline.zoom,panels.timeline.offset),time);
    assert_eq!(panels.timeline.graph_y_zoom,vertical);
    assert!(actions.is_empty());
}

#[test]
fn in_out_shortcuts_seek_to_the_selected_clip_without_editing() {
    let mut engine = engine();
    engine
        .apply(Command::SetLayerRange {
            object: 7,
            in_frame: 12.,
            out_frame: 60.,
        })
        .unwrap();
    let before = engine.project().clone();
    let mut panels = panels();
    let base = state(&engine, 30.);
    let out = invoke(&mut panels, &base, Key::I, Modifiers::default());
    assert!(matches!(out.as_slice(), [Action::Seek(t)] if (t - 12.).abs() < 1e-9));
    let out = invoke(&mut panels, &base, Key::O, Modifiers::default());
    assert!(matches!(out.as_slice(), [Action::Seek(t)] if (t - 59.).abs() < 1e-9));
    assert_eq!(engine.project(), &before);
}

#[test]
fn bracket_shortcuts_move_and_trim_clips_as_single_undoable_edits() {
    let mut engine = engine();
    engine
        .apply(Command::SetLayerRange {
            object: 7,
            in_frame: 12.,
            out_frame: 60.,
        })
        .unwrap();
    let before = engine.project().clone();
    let mut panels = panels();

    let out = invoke(
        &mut panels,
        &state(&engine, 30.),
        Key::BracketLeft,
        Modifiers::default(),
    );
    assert!(matches!(out.as_slice(), [Action::EditBatch(v)] if v.len() == 1));
    apply(&mut engine, out);
    let clip = engine.project().layers[0].clip(120);
    assert_eq!((clip.in_frame, clip.out_frame), (30, 78));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);

    let out = invoke(
        &mut panels,
        &state(&engine, 30.),
        Key::BracketLeft,
        Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    apply(&mut engine, out);
    let clip = engine.project().layers[0].clip(120);
    assert_eq!((clip.in_frame, clip.out_frame), (30, 60));
    engine.undo().unwrap();

    let out = invoke(
        &mut panels,
        &state(&engine, 30.),
        Key::BracketRight,
        Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    apply(&mut engine, out);
    let clip = engine.project().layers[0].clip(120);
    assert_eq!((clip.in_frame, clip.out_frame), (12, 31));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn in_out_edits_skip_locked_or_empty_selection_and_batch_all_selected_layers() {
    let mut locked_engine = engine();
    locked_engine
        .apply(Command::Flags {
            object: 7,
            visible: true,
            locked: true,
        })
        .unwrap();
    let mut panels = panels();
    assert!(invoke(
        &mut panels,
        &state(&locked_engine, 30.),
        Key::BracketLeft,
        Modifiers::default(),
    )
    .is_empty());
    let observing = invoke(
        &mut Panels {
            selected: Some(7),
            focused: Some(Panel::Timeline),
            ..Default::default()
        },
        &serde_json::json!({
            "frame": 30.,
            "project": locked_engine.project(),
            "timeline_layers": locked_engine.project().timeline_layers(30.).unwrap(),
            "observing": true,
        }),
        Key::BracketLeft,
        Modifiers::default(),
    );
    assert!(observing.is_empty());

    let mut doubled = engine();
    doubled.apply(Command::Duplicate { object: 7 }).unwrap();
    let ids: Vec<u64> = doubled.project().layers.iter().map(|l| l.id).collect();
    assert_eq!(ids.len(), 2);
    panels.selected = ids.last().copied();
    panels.selected_layers = ids.clone();
    let out = invoke(
        &mut panels,
        &state(&doubled, 30.),
        Key::BracketLeft,
        Modifiers::default(),
    );
    assert!(matches!(out.as_slice(), [Action::EditBatch(v)] if v.len() == 2));
    apply(&mut doubled, out);
    for layer in &doubled.project().layers {
        let clip = layer.clip(120);
        assert_eq!((clip.in_frame, clip.out_frame), (30, 150));
    }
}

#[test]
fn f4_toggles_composite_columns_and_shift_f4_toggles_parent() {
    let engine = engine();
    let mut panels = panels();
    let base = state(&engine, 30.);
    let out = invoke(&mut panels, &base, Key::F4, Modifiers::default());
    assert!(matches!(
        out.as_slice(),
        [Action::TimelineColumnVisibility(v)] if !v.blend && !v.matte && v.parent
    ));
    let out = invoke(
        &mut panels,
        &base,
        Key::F4,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert!(matches!(
        out.as_slice(),
        [Action::TimelineColumnVisibility(v)] if v.blend && v.matte && !v.parent
    ));
    assert_eq!(panels.timeline.column_visibility, ColumnVisibility::default());
}
