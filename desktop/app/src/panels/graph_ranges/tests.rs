use super::*;
use motion_core::{Engine, Keyframe, Layer, Project};
use motion_ui::input::Modifiers;
mod effect;

fn fixture() -> (Engine, Value, Panels) {
    let mut project = Project::new(640, 480, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Animated", [100., 100.], [0.; 3], [1.; 4]);
    layer.transform.opacity.keys = [(0, 0.2), (20, 0.5), (40, 0.8)]
        .into_iter()
        .map(|(frame, value)| Keyframe {
            frame,
            value,
            ease: motion_core::Ease::Linear,
            curve: None,
            spatial: None,
        })
        .collect();
    project.layers.push(layer);
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(Command::MoveLayerClip {
            object: 7,
            in_frame: 12,
        })
        .unwrap();
    let state = serde_json::json!({"root":"graph-ranges","composition":motion_core::MAIN_COMPOSITION,
        "document_generation":1,"revision":engine.revision(),"frame":32.,"project":engine.project(),
        "timeline_layers":engine.project().timeline_layers(32.).unwrap()});
    let mut panels = Panels {
        selected: Some(7),
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    panels.timeline.graph_mode = true;
    panels
        .animation_controls
        .select_key_in(&state, 7, Property::Opacity, None, 32);
    (engine, state, panels)
}
fn id() -> Identity {
    Identity {
        surface: Surface::Timeline,
        target: Some(KeyframeTarget::Property {
            object: 7,
            property: Property::Opacity,
            axis: None,
        }),
        component: Some(0),
        speed: false,
        unit: "%".into(),
    }
}
fn bounds(x: [f64; 2], y: [f64; 2]) -> Bounds {
    Bounds {
        x: Range::new(x[0], x[1]).unwrap(),
        y: Range::new(y[0], y[1]).unwrap(),
    }
}
fn options() -> Options {
    Options {
        displayed: bounds([0., 120.], [0., 100.]),
        all: Some(bounds([10., 60.], [10., 90.])),
        selection: Some(bounds([30., 34.], [45., 55.])),
    }
}
fn prepare(p: &mut Panels, state: &Value, window: Option<Panel>, input: &Input) -> bool {
    p.timeline.graph_ranges.prepare(
        window,
        state,
        input,
        &p.selected_keys,
        p.animation_controls.user_key(),
        p.effect_timeline.range_key(),
        p.selected,
        p.focused,
    )
}
fn open(p: &mut Panels, state: &Value, window: Option<Panel>, options: Options) {
    prepare(p, state, window, &Input::default());
    p.timeline
        .graph_ranges
        .displayed(state, &id(), options.displayed);
    p.timeline.graph_ranges.open(
        [100., 150.],
        id(),
        options,
        state,
        &p.selected_keys,
        p.animation_controls.user_key(),
        p.effect_timeline.range_key(),
        p.selected,
        p.focused,
    );
}
fn draw(p: &mut Panels, state: &Value, input: &Input, atlas: &mut TextAtlas) {
    overlay(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., 900., 600.),
        p,
        state,
        input,
    );
}
fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn choose(p: &mut Panels, state: &Value, item: usize, atlas: &mut TextAtlas) {
    p.timeline.graph_ranges.menu.as_mut().unwrap().focus = item;
    draw(p, state, &key(Key::Space), atlas);
}

#[test]
fn native_checkbox_locks_capture_bounds_and_fit_only_the_unlocked_axis_without_edits() {
    let (engine, state, mut panels) = fixture();
    let before = serde_json::to_vec(engine.project()).unwrap();
    let history = (engine.revision(), engine.can_undo(), engine.can_redo());
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Timeline)] {
            panels.locale = locale;
            panels.timeline.graph_ranges = Controls::default();
            panels.timeline.zoom = 2.;
            panels.timeline.offset = 20.;
            open(&mut panels, &state, window, options());
            draw(&mut panels, &state, &Input::default(), &mut atlas);
            choose(&mut panels, &state, 3, &mut atlas);
            assert!(panels.timeline.graph_ranges.locked_x());
            choose(&mut panels, &state, 1, &mut atlas);
            assert!(!panels.timeline.graph_ranges.editing());
            assert_eq!((panels.timeline.zoom, panels.timeline.offset), (2., 20.));
            assert_eq!(
                panels
                    .timeline
                    .graph_ranges
                    .displayed(&state, &id(), options().displayed)
                    .y,
                options().selection.unwrap().y
            );
            open(&mut panels, &state, window, options());
            draw(&mut panels, &state, &Input::default(), &mut atlas);
            choose(&mut panels, &state, 4, &mut atlas);
            assert!(panels.timeline.graph_ranges.locked_y(&id()));
            assert!(!panels.timeline.graph_ranges.navigate_y(
                &id(),
                options().displayed.y,
                2.,
                50.
            ));
            let mut later = state.clone();
            later["frame"] = 90.into();
            let changed = bounds([20., 80.], [-1000., 2000.]);
            assert_eq!(
                panels
                    .timeline
                    .graph_ranges
                    .displayed(&later, &id(), changed)
                    .y,
                options().selection.unwrap().y
            );
            draw(&mut panels, &later, &key(Key::Escape), &mut atlas);
            assert!(!panels.timeline.graph_ranges.editing());
        }
    }
    assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
    assert_eq!(
        (engine.revision(), engine.can_undo(), engine.can_redo()),
        history
    );
}

#[test]
fn menu_consumes_closing_frames_and_cancels_foreign_input_source_selection_and_track_changes() {
    let (_, state, mut p) = fixture();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for window in [None, Some(Panel::Timeline)] {
        open(&mut p, &state, window, options());
        draw(&mut p, &state, &Input::default(), &mut atlas);
        let foreign = if window.is_none() {
            Some(Panel::Timeline)
        } else {
            None
        };
        assert!(!prepare(&mut p, &state, foreign, &Input::default()));
        assert!(p.timeline.graph_ranges.editing());
        assert!(prepare(&mut p, &state, foreign, &key(Key::Space)));
        assert!(!p.timeline.graph_ranges.editing());
        for case in 0..6 {
            open(&mut p, &state, window, options());
            draw(&mut p, &state, &Input::default(), &mut atlas);
            let mut next = state.clone();
            let mut input = Input::default();
            match case {
                0 => next["revision"] = 999.into(),
                1 => next["document_generation"] = 2.into(),
                2 => {
                    p.selected_keys.clear(&state);
                }
                3 => input.focus_lost = true,
                4 => next["project"]["layers"][0]["transform"]["opacity"]["value"] = 0.7.into(),
                _ => p.focused = Some(Panel::Properties),
            }
            assert!(prepare(&mut p, &next, window, &input), "case={case}");
            assert!(!p.timeline.graph_ranges.editing(), "case={case}");
            p.selected_keys.release();
            p.focused = Some(Panel::Timeline);
        }
        open(&mut p, &state, window, options());
        draw(&mut p, &state, &Input::default(), &mut atlas);
        let outside = Input {
            pressed: Some((MouseButton::Left, [10., 10.])),
            mouse: [10., 10.],
            ..Default::default()
        };
        assert!(prepare(&mut p, &state, window, &outside));
        draw(&mut p, &state, &outside, &mut atlas);
        assert!(!p.timeline.graph_ranges.editing());
    }
}

#[test]
fn keyboard_skips_disabled_rows_and_ime_repeat_and_opening_frame_cannot_activate() {
    let (_, state, mut p) = fixture();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    open(
        &mut p,
        &state,
        None,
        Options {
            all: None,
            selection: None,
            ..options()
        },
    );
    draw(&mut p, &state, &key(Key::Enter), &mut atlas);
    assert!(p.timeline.graph_ranges.editing());
    assert_eq!(p.timeline.graph_ranges.menu.as_ref().unwrap().focus, 2);
    draw(&mut p, &state, &key(Key::Down), &mut atlas);
    assert_eq!(p.timeline.graph_ranges.menu.as_ref().unwrap().focus, 3);
    for input in [
        Input {
            composing: true,
            ..key(Key::Space)
        },
        Input {
            ime_consumed: true,
            ..key(Key::Enter)
        },
        Input {
            key_repeats: vec![true],
            ..key(Key::Space)
        },
    ] {
        draw(&mut p, &state, &input, &mut atlas);
        assert!(!p.timeline.graph_ranges.locked_x());
    }
    draw(&mut p, &state, &key(Key::Space), &mut atlas);
    assert!(p.timeline.graph_ranges.locked_x());
    draw(&mut p, &state, &key(Key::Tab), &mut atlas);
    assert_eq!(p.timeline.graph_ranges.menu.as_ref().unwrap().focus, 4);
    draw(&mut p, &state, &key(Key::Enter), &mut atlas);
    assert!(p.timeline.graph_ranges.locked_y(&id()));
    draw(&mut p, &state, &key(Key::Up), &mut atlas);
    assert_eq!(p.timeline.graph_ranges.menu.as_ref().unwrap().focus, 3);
}

#[test]
fn actual_property_graph_menu_fits_composition_time_once_and_owned_empty_does_not_fall_back() {
    let (engine, state, mut p) = fixture();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let before = serde_json::to_vec(engine.project()).unwrap();
    let history = (engine.can_undo(), engine.can_redo(), engine.revision());
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Timeline)] {
            p.locale = locale;
            prepare(&mut p, &state, window, &Input::default());
            let mut actions = vec![];
            super::super::timeline_graph::render(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 100., 800., 400.),
                &mut p,
                &state,
                &Input {
                    pressed: Some((MouseButton::Right, [350., 103.])),
                    mouse: [350., 103.],
                    ..Default::default()
                },
                0.,
                800. / 120.,
                &mut actions,
            );
            assert!(actions.is_empty());
            let menu = p
                .timeline
                .graph_ranges
                .menu
                .as_ref()
                .expect("real graph header must open the menu");
            assert_eq!(
                menu.options.all.unwrap().x,
                Range::fitted(12., 52., 0.05, 1.).unwrap()
            );
            assert_eq!(
                menu.options.selection.unwrap().x,
                Range::fitted(32., 32., 0.05, 1.).unwrap()
            );
            let y = menu.options.selection.unwrap().y;
            assert!(
                y.low < 50. && y.high > 50. && y.span() < 10.,
                "actual percent units: {y:?}"
            );
            draw(&mut p, &state, &Input::default(), &mut atlas);
            choose(&mut p, &state, 1, &mut atlas);
            assert_eq!(p.timeline.zoom, 120.);
            assert!((p.timeline.offset - 31.5).abs() < 0.001);
            p.selected_keys.clear(&state);
            super::super::timeline_graph::render(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 100., 800., 400.),
                &mut p,
                &state,
                &Input {
                    pressed: Some((MouseButton::Right, [350., 103.])),
                    ..Default::default()
                },
                0.,
                800. / 120.,
                &mut actions,
            );
            assert!(p
                .timeline
                .graph_ranges
                .menu
                .as_ref()
                .unwrap()
                .options
                .selection
                .is_none());
            p.timeline.graph_ranges.cancel();
            p.selected_keys.release();
        }
    }
    assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
    assert_eq!(
        (engine.can_undo(), engine.can_redo(), engine.revision()),
        history
    );
}

#[test]
fn time_lock_blocks_navigation_and_channel_views_are_bounded_and_reset_on_document_change() {
    let (_, state, mut p) = fixture();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    open(&mut p, &state, None, options());
    draw(&mut p, &state, &Input::default(), &mut atlas);
    choose(&mut p, &state, 3, &mut atlas);
    p.timeline.graph_ranges.cancel();
    p.timeline.zoom = 4.;
    p.timeline.offset = 20.;
    super::super::timeline_zoom::zoom_at(&mut p.timeline, 120., 8., 0.5);
    assert_eq!((p.timeline.zoom, p.timeline.offset), (4., 20.));
    let mut actions = vec![];
    for modifiers in [
        Modifiers {
            control: true,
            ..Default::default()
        },
        Modifiers {
            shift: true,
            ..Default::default()
        },
    ] {
        super::super::timeline::render(
            &mut PaintList::default(),
            &mut atlas,
            Rect::new(0., 0., 1200., 600.),
            &mut p,
            &state,
            &Input {
                mouse: [900., 200.],
                wheel: [0., -20.],
                modifiers,
                ..Default::default()
            },
            &mut actions,
        );
        assert_eq!((p.timeline.zoom, p.timeline.offset), (4., 20.));
    }
    assert!(actions.is_empty());
    for index in 0..MAX_VIEWS * 2 {
        let mut next = id();
        next.unit = format!("unit-{index}");
        p.timeline
            .graph_ranges
            .displayed(&state, &next, options().displayed);
    }
    assert_eq!(p.timeline.graph_ranges.entries.len(), MAX_VIEWS);
    let mut next = state.clone();
    next["document_generation"] = 2.into();
    p.timeline.graph_ranges.sync(&next);
    assert!(p.timeline.graph_ranges.entries.is_empty());
    assert!(!p.timeline.graph_ranges.locked_x());
}

#[test]
fn automatic_fit_resets_old_vertical_navigation_and_preserves_a_locked_axis() {
    let (_, state, mut p) = fixture();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    p.timeline.graph_y_zoom = 8.;
    p.timeline.graph_y_offset = 100.;
    open(&mut p, &state, None, options());
    draw(&mut p, &state, &Input::default(), &mut atlas);
    choose(&mut p, &state, 2, &mut atlas);
    assert_eq!(
        (p.timeline.graph_y_zoom, p.timeline.graph_y_offset),
        (1., 0.)
    );
    open(&mut p, &state, None, options());
    draw(&mut p, &state, &Input::default(), &mut atlas);
    choose(&mut p, &state, 4, &mut atlas);
    p.timeline.graph_y_zoom = 4.;
    p.timeline.graph_y_offset = 25.;
    choose(&mut p, &state, 2, &mut atlas);
    assert_eq!(
        (p.timeline.graph_y_zoom, p.timeline.graph_y_offset),
        (4., 25.)
    );
    assert_eq!(
        p.timeline
            .graph_ranges
            .displayed(&state, &id(), bounds([0., 120.], [-1000., 1000.]))
            .y,
        options().displayed.y
    );
}

#[test]
fn actual_main_and_floating_chrome_keep_menu_keys_from_editing_the_selected_key() {
    let (engine, state, mut p) = fixture();
    let before = serde_json::to_vec(engine.project()).unwrap();
    let history = (engine.revision(), engine.can_undo(), engine.can_redo());
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Timeline)] {
            p.locale = locale;
            open(&mut p, &state, window, options());
            let render = |p: &mut Panels, input: &Input, atlas: &mut TextAtlas| {
                let mut list = PaintList::default();
                if window.is_some() {
                    super::super::floating_chrome(
                        &mut list,
                        atlas,
                        Panel::Timeline,
                        [720., 520.],
                        p,
                        &state,
                        input,
                    )
                } else {
                    super::super::chrome(
                        &mut list,
                        atlas,
                        [900., 600.],
                        &Node::dock(Panel::Timeline.dock()),
                        p,
                        &state,
                        input,
                    )
                }
            };
            assert!(render(&mut p, &Input::default(), &mut atlas).is_empty());
            assert!(p.timeline.graph_ranges.editing());
            assert!(render(&mut p, &key(Key::Delete), &mut atlas).is_empty());
            assert!(p.timeline.graph_ranges.editing());
            assert!(render(&mut p, &key(Key::Escape), &mut atlas).is_empty());
            assert!(!p.timeline.graph_ranges.editing());
        }
    }
    assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
    assert_eq!(
        (engine.revision(), engine.can_undo(), engine.can_redo()),
        history
    );
}
