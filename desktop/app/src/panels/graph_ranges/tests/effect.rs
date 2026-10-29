use super::*;
use crate::panels::effect_timeline;
use crate::panels::effect_timeline::tests::{
    fixture as effect_fixture, setup as effect_panels, state as effect_state,
};

fn prepared(locale: Locale) -> (Engine, Value, Panels) {
    let (engine, catalogue) = effect_fixture();
    let state = effect_state(&engine);
    let mut p = effect_panels(&catalogue, locale);
    p.timeline.set_graph_mode(true);
    effect_timeline::project_key(&mut p, &state, 7, 1, "speed", 20);
    (engine, state, p)
}
fn launch(p: &mut Panels, state: &Value, window: Option<Panel>, atlas: &mut TextAtlas) {
    prepare(p, state, window, &Input::default());
    let mut actions = vec![];
    assert!(effect_timeline::graph::render(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 100., 800., 400.),
        p,
        state,
        &Input {
            pressed: Some((MouseButton::Right, [350., 103.])),
            mouse: [350., 103.],
            ..Default::default()
        },
        0.,
        800. / 180.,
        &mut actions
    ));
    assert!(actions.is_empty());
    assert!(p.timeline.graph_ranges.editing());
    draw(p, state, &Input::default(), atlas);
}

#[test]
fn effect_range_menu_fits_local_keys_once_and_retains_native_values_and_per_second_speed() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Timeline)] {
            let (engine, state, mut p) = prepared(locale);
            let before = serde_json::to_vec(engine.project()).unwrap();
            let history = (engine.revision(), engine.can_undo(), engine.can_redo());
            for speed in [false, true] {
                p.timeline.speed_graph = speed;
                launch(&mut p, &state, window, &mut atlas);
                let menu = p.timeline.graph_ranges.menu.as_ref().unwrap();
                assert_eq!(
                    menu.options.all.unwrap().x,
                    Range::fitted(20., 80., 0.05, 1.).unwrap()
                );
                assert_eq!(
                    menu.options.selection.unwrap().x,
                    Range::fitted(20., 20., 0.05, 1.).unwrap()
                );
                assert!(menu
                    .id
                    .unit
                    .ends_with(if speed { ":value/s" } else { ":value" }));
                let y = menu.options.selection.unwrap().y;
                let expected = if speed { 0.375 } else { 0.25 };
                assert!(
                    (y.at(0.5) - expected).abs() < 1e-5,
                    "actual effect units {y:?}"
                );
                choose(&mut p, &state, 1, &mut atlas);
                assert_eq!(p.timeline.zoom, 180.);
                assert_eq!(p.timeline.offset, 19.5);
            }
            assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
            assert_eq!(
                (engine.revision(), engine.can_undo(), engine.can_redo()),
                history
            );
        }
    }
}

#[test]
fn effect_menu_cancels_projected_key_changes_and_owned_empty_never_uses_the_old_diamond() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for window in [None, Some(Panel::Timeline)] {
        let (_, state, mut p) = prepared(Locale::En);
        launch(&mut p, &state, window, &mut atlas);
        effect_timeline::project_key(&mut p, &state, 7, 1, "speed", 80);
        assert!(prepare(&mut p, &state, window, &key(Key::Delete)));
        assert!(!p.timeline.graph_ranges.editing());
        launch(&mut p, &state, window, &mut atlas);
        assert_eq!(
            p.timeline
                .graph_ranges
                .menu
                .as_ref()
                .unwrap()
                .options
                .selection
                .unwrap()
                .x,
            Range::fitted(80., 80., 0.05, 1.).unwrap()
        );
        p.timeline.graph_ranges.cancel();
        p.selected_keys.clear(&state);
        launch(&mut p, &state, window, &mut atlas);
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
        let target = KeyframeTarget::Effect {
            object: 7,
            effect: 1,
            param: "speed".into(),
        };
        p.selected_keys
            .apply(
                &state,
                [20, 80].map(|frame| key_selection::Key {
                    target: target.clone(),
                    frame,
                }),
                key_selection::Mode::Replace,
            )
            .unwrap();
        launch(&mut p, &state, window, &mut atlas);
        assert_eq!(
            p.timeline
                .graph_ranges
                .menu
                .as_ref()
                .unwrap()
                .options
                .selection
                .unwrap()
                .x,
            Range::fitted(20., 80., 0.05, 1.).unwrap()
        );
    }
}

#[test]
fn effect_locks_survive_playback_wheel_and_main_float_escape_consumes_the_key() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Timeline)] {
            let (engine, state, mut p) = prepared(locale);
            let before = serde_json::to_vec(engine.project()).unwrap();
            launch(&mut p, &state, window, &mut atlas);
            let id = p.timeline.graph_ranges.menu.as_ref().unwrap().id.clone();
            let displayed = p
                .timeline
                .graph_ranges
                .menu
                .as_ref()
                .unwrap()
                .options
                .displayed;
            choose(&mut p, &state, 3, &mut atlas);
            choose(&mut p, &state, 4, &mut atlas);
            let render = |p: &mut Panels, input: &Input, atlas: &mut TextAtlas| {
                if window.is_some() {
                    crate::panels::floating_chrome(
                        &mut PaintList::default(),
                        atlas,
                        Panel::Timeline,
                        [1200., 850.],
                        p,
                        &state,
                        input,
                    )
                } else {
                    crate::panels::chrome(
                        &mut PaintList::default(),
                        atlas,
                        [1200., 850.],
                        &Node::dock(Panel::Timeline.dock()),
                        p,
                        &state,
                        input,
                    )
                }
            };
            assert!(render(&mut p, &key(Key::Delete), &mut atlas).is_empty());
            assert!(p.timeline.graph_ranges.editing());
            assert!(render(&mut p, &key(Key::Escape), &mut atlas).is_empty());
            assert!(!p.timeline.graph_ranges.editing());
            p.timeline.zoom = 4.;
            p.timeline.offset = 20.;
            let legacy_y = (p.timeline.graph_y_zoom, p.timeline.graph_y_offset);
            for modifiers in [
                Modifiers {
                    control: true,
                    ..Default::default()
                },
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                Modifiers {
                    control: true,
                    alt: true,
                    ..Default::default()
                },
            ] {
                assert!(render(
                    &mut p,
                    &Input {
                        mouse: [900., 400.],
                        wheel: [0., -80.],
                        modifiers,
                        ..Default::default()
                    },
                    &mut atlas
                )
                .is_empty());
                assert_eq!((p.timeline.zoom, p.timeline.offset), (4., 20.));
                assert_eq!(
                    (p.timeline.graph_y_zoom, p.timeline.graph_y_offset),
                    legacy_y
                );
            }
            let mut playing = state.clone();
            playing["frame"] = 100.into();
            playing["timeline_layers"] =
                serde_json::to_value(engine.project().timeline_layers(100.).unwrap()).unwrap();
            let auto = bounds([20., 65.], [-100., 100.]);
            assert_eq!(
                p.timeline.graph_ranges.displayed(&playing, &id, auto).y,
                displayed.y
            );
            assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
            assert!(!engine.can_undo());
        }
    }
}
