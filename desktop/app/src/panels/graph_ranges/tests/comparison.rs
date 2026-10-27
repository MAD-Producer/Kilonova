use super::*;
use crate::panels::curve_comparison::tests::{
    fixture as comparison_fixture, setup as comparison_panels, state as comparison_state,
};

fn render(
    p: &mut Panels,
    state: &Value,
    input: &Input,
    window: Option<Panel>,
    atlas: &mut TextAtlas,
) -> Vec<Action> {
    let mut list = PaintList::default();
    if window.is_some() {
        crate::panels::floating_chrome(
            &mut list,
            atlas,
            Panel::Timeline,
            [1200., 620.],
            p,
            state,
            input,
        )
    } else {
        crate::panels::chrome(
            &mut list,
            atlas,
            [1200., 850.],
            &Node::dock(Panel::Timeline.dock()),
            p,
            state,
            input,
        )
    }
}
fn launch(p: &mut Panels, state: &Value, unit: &str, window: Option<Panel>, atlas: &mut TextAtlas) {
    assert!(render(p, state, &Input::default(), window, atlas).is_empty());
    let header = p.timeline.comparison.test_range_header(unit);
    let point = [
        header.min[0] + header.width() * 0.5,
        header.min[1] + header.height() * 0.5,
    ];
    assert!(render(
        p,
        state,
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Right, point)),
            ..Default::default()
        },
        window,
        atlas
    )
    .is_empty());
    assert!(p.timeline.graph_ranges.editing(),"window={window:?} unit={unit} point={point:?} focused={:?} layer_popup={} range_trace={:?}",
        p.focused,p.layer_actions.editing(),p.timeline.comparison.test_range_debug());
    assert!(render(p, state, &Input::default(), window, atlas).is_empty());
    assert!(
        p.timeline.graph_ranges.editing(),
        "painting another group must retain the owning menu"
    );
}
fn choose_item(
    p: &mut Panels,
    state: &Value,
    item: usize,
    window: Option<Panel>,
    atlas: &mut TextAtlas,
) {
    p.timeline.graph_ranges.menu.as_mut().unwrap().focus = item;
    assert!(render(p, state, &key(Key::Space), window, atlas).is_empty());
}

#[test]
fn comparison_fit_selection_uses_only_its_unit_group_and_composition_time_without_history() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Timeline)] {
            let engine = comparison_fixture();
            let state = comparison_state(&engine);
            let mut p = comparison_panels(&state, locale);
            let before = serde_json::to_vec(engine.project()).unwrap();
            let history = engine.history_diagnostics();
            let target = KeyframeTarget::Property {
                object: 7,
                property: Property::Opacity,
                axis: None,
            };
            p.selected_keys
                .apply(
                    &state,
                    [key_selection::Key { target, frame: 45 }],
                    key_selection::Mode::Replace,
                )
                .unwrap();
            launch(&mut p, &state, "px", window, &mut atlas);
            assert!(p
                .timeline
                .graph_ranges
                .menu
                .as_ref()
                .unwrap()
                .options
                .selection
                .is_none());
            assert!(render(&mut p, &state, &key(Key::Escape), window, &mut atlas).is_empty());
            launch(&mut p, &state, "%", window, &mut atlas);
            let menu = p.timeline.graph_ranges.menu.as_ref().unwrap();
            assert_eq!(menu.id.surface, Surface::Comparison);
            assert_eq!(
                menu.options.all.unwrap().x,
                Range::fitted(15., 75., 0.05, 1.).unwrap()
            );
            let selected = menu.options.selection.unwrap();
            assert_eq!(selected.x, Range::fitted(45., 45., 0.05, 1.).unwrap());
            assert!((selected.y.at(0.5) - 50.).abs() < 1e-5);
            choose_item(&mut p, &state, 1, window, &mut atlas);
            assert!(!p.timeline.graph_ranges.editing());
            assert_eq!(p.timeline.zoom, 120.);
            assert_eq!(p.timeline.offset, 44.5);
            assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
            assert_eq!(engine.history_diagnostics(), history);
        }
    }
}

#[test]
fn comparison_axis_locks_are_per_unit_and_automatic_preserves_other_unit_and_legacy_y() {
    let engine = comparison_fixture();
    let state = comparison_state(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Timeline)] {
            let mut p = comparison_panels(&state, locale);
            p.timeline.graph_y_zoom = 4.;
            p.timeline.graph_y_offset = 5.;
            launch(&mut p, &state, "px", window, &mut atlas);
            let pixel = p.timeline.graph_ranges.menu.as_ref().unwrap().id.clone();
            let pixel_range = p
                .timeline
                .graph_ranges
                .menu
                .as_ref()
                .unwrap()
                .options
                .displayed;
            choose_item(&mut p, &state, 4, window, &mut atlas);
            assert!(render(&mut p, &state, &key(Key::Escape), window, &mut atlas).is_empty());
            launch(&mut p, &state, "%", window, &mut atlas);
            let percent = p.timeline.graph_ranges.menu.as_ref().unwrap().id.clone();
            let percent_range = p
                .timeline
                .graph_ranges
                .menu
                .as_ref()
                .unwrap()
                .options
                .displayed;
            choose_item(&mut p, &state, 4, window, &mut atlas);
            choose_item(&mut p, &state, 3, window, &mut atlas);
            assert!(render(&mut p, &state, &key(Key::Escape), window, &mut atlas).is_empty());
            p.timeline.zoom = 4.;
            p.timeline.offset = 20.;
            let header = p.timeline.comparison.test_range_header("%");
            let mouse = [header.min[0] + header.width() * 0.75, header.max[1] + 50.];
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
                    &state,
                    &Input {
                        mouse,
                        wheel: [0., -80.],
                        modifiers,
                        ..Default::default()
                    },
                    window,
                    &mut atlas
                )
                .is_empty());
                assert_eq!((p.timeline.zoom, p.timeline.offset), (4., 20.));
            }
            assert_eq!(
                p.timeline
                    .graph_ranges
                    .displayed(&state, &pixel, bounds([20., 50.], [-1000., 1000.]))
                    .y,
                pixel_range.y
            );
            assert_eq!(
                p.timeline
                    .graph_ranges
                    .displayed(&state, &percent, bounds([20., 50.], [-1000., 1000.]))
                    .y,
                percent_range.y
            );
            launch(&mut p, &state, "%", window, &mut atlas);
            choose_item(&mut p, &state, 4, window, &mut atlas);
            choose_item(&mut p, &state, 2, window, &mut atlas);
            assert_eq!(
                (p.timeline.graph_y_zoom, p.timeline.graph_y_offset),
                (4., 5.)
            );
            assert!(p.timeline.graph_ranges.locked_y(&pixel));
            assert!(!p.timeline.graph_ranges.locked_y(&percent));
            assert_eq!(
                p.timeline
                    .graph_ranges
                    .displayed(&state, &pixel, bounds([20., 50.], [-1000., 1000.]))
                    .y,
                pixel_range.y
            );
            assert_eq!(
                engine.history_diagnostics(),
                comparison_fixture().history_diagnostics()
            );
        }
    }
}

#[test]
fn comparison_menu_cancels_mode_and_collection_changes_and_consumes_delete_escape() {
    let engine = comparison_fixture();
    let state = comparison_state(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for window in [None, Some(Panel::Timeline)] {
        let mut p = comparison_panels(&state, Locale::En);
        launch(&mut p, &state, "px", window, &mut atlas);
        assert!(render(&mut p, &state, &key(Key::Delete), window, &mut atlas).is_empty());
        assert!(p.timeline.graph_ranges.editing());
        p.timeline.speed_graph = true;
        assert!(render(&mut p, &state, &key(Key::Delete), window, &mut atlas).is_empty());
        assert!(!p.timeline.graph_ranges.editing());
        launch(&mut p, &state, "px", window, &mut atlas);
        p.timeline.comparison = crate::panels::curve_comparison::Controls::default();
        assert!(render(&mut p, &state, &key(Key::Delete), window, &mut atlas).is_empty());
        assert!(!p.timeline.graph_ranges.editing());
        p = comparison_panels(&state, Locale::Zh);
        launch(&mut p, &state, "%", window, &mut atlas);
        assert!(render(&mut p, &state, &key(Key::Escape), window, &mut atlas).is_empty());
        assert!(!p.timeline.graph_ranges.editing());
    }
    assert_eq!(
        engine.history_diagnostics(),
        comparison_fixture().history_diagnostics()
    );
}
