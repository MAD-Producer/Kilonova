use super::*;
use motion_ui::input::Key;

fn prepared(
    locale: Locale,
    floating: bool,
    zoom: f32,
    offset: f32,
) -> (Engine, Value, Panels, TextAtlas) {
    let (engine, catalogue) = fixture();
    let snapshot = state(&engine);
    let mut panels = setup(&catalogue, locale);
    let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
    choose(
        &mut panels,
        &snapshot,
        &Target::new(&snapshot, 7, 1, "speed"),
        Some(20),
    );
    panels.timeline.set_graph_mode(true);
    panels.timeline.graph_y_zoom = zoom;
    panels.timeline.graph_y_offset = offset;
    for _ in 0..2 {
        assert!(draw(
            &mut panels,
            &snapshot,
            Input::default(),
            floating,
            &mut atlas
        )
        .is_empty());
    }
    (engine, snapshot, panels, atlas)
}

fn key_points(panels: &Panels) -> [[f32; 2]; 2] {
    [20, 80].map(|frame| point(panels.effect_timeline.rects[&(1, "speed".into(), frame)]))
}

fn wheel(mouse: [f32; 2], delta: f32) -> Input {
    Input {
        mouse,
        wheel: [0., delta],
        modifiers: Modifiers {
            control: true,
            alt: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "actual={actual} expected={expected}"
    );
}

#[test]
fn effect_value_zoom_changes_key_spacing_in_the_wheel_frame_without_editing() {
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let (engine, snapshot, mut panels, mut atlas) = prepared(locale, floating, 1., 0.);
            let before = key_points(&panels);
            let time_view = (panels.timeline.zoom, panels.timeline.offset);
            let mouse = [before[0][0] + 2., before[0][1]];
            assert!(draw(
                &mut panels,
                &snapshot,
                wheel(mouse, -80.),
                floating,
                &mut atlas
            )
            .is_empty());
            let after = key_points(&panels);
            let factor = panels.timeline.graph_y_zoom;
            assert!(factor > 1.);
            near(
                after[1][1] - after[0][1],
                (before[1][1] - before[0][1]) * factor,
            );
            near(after[0][1], before[0][1]);
            assert_eq!((panels.timeline.zoom, panels.timeline.offset), time_view);
            assert!(draw(
                &mut panels,
                &snapshot,
                Input::default(),
                floating,
                &mut atlas
            )
            .is_empty());
            for (painted, confirmed) in after.into_iter().zip(key_points(&panels)) {
                near(painted[1], confirmed[1]);
            }
            assert_eq!(
                engine.project(),
                &serde_json::from_value::<Project>(snapshot["project"].clone()).unwrap()
            );
            assert!(!engine.can_undo());
        }
    }
}

#[test]
fn effect_value_zoom_keeps_mouse_value_after_repaint_including_both_limits() {
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            for (zoom, offset, delta) in [
                (1., 0., -80.),
                (1., 0., 80.),
                (3., -0.25, -120.),
                (1., 0., -f32::MAX),
                (1., 0., f32::MAX),
            ] {
                let (engine, snapshot, mut panels, mut atlas) =
                    prepared(locale, floating, zoom, offset);
                let anchor = key_points(&panels)[0];
                let mouse = [anchor[0] + 2., anchor[1]];
                let before = engine.project().clone();
                assert!(draw(
                    &mut panels,
                    &snapshot,
                    wheel(mouse, delta),
                    floating,
                    &mut atlas
                )
                .is_empty());
                assert!(draw(
                    &mut panels,
                    &snapshot,
                    Input::default(),
                    floating,
                    &mut atlas
                )
                .is_empty());
                near(key_points(&panels)[0][1], anchor[1]);
                if delta.abs() == f32::MAX {
                    assert_eq!(
                        panels.timeline.graph_y_zoom,
                        if delta < 0. { 64. } else { 0.25 }
                    );
                    for _ in 0..8 {
                        assert!(draw(
                            &mut panels,
                            &snapshot,
                            wheel(mouse, delta),
                            floating,
                            &mut atlas
                        )
                        .is_empty());
                        near(key_points(&panels)[0][1], anchor[1]);
                    }
                }
                assert_eq!(engine.project(), &before);
                assert!(!engine.can_undo());
            }
        }
    }
}

#[test]
fn effect_value_zoom_preserves_invalid_input_and_pending_drag_cancel_boundaries() {
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let (mut engine, snapshot, mut panels, mut atlas) = prepared(locale, floating, 1., 0.);
            let anchor = key_points(&panels)[0];
            let mouse = [anchor[0] + 2., anchor[1]];
            let mut no_control = wheel(mouse, -80.);
            no_control.modifiers.control = false;
            let mut no_alt = wheel(mouse, -80.);
            no_alt.modifiers.alt = false;
            for input in [
                wheel(mouse, 0.),
                wheel(mouse, f32::NAN),
                wheel(mouse, f32::INFINITY),
                wheel(mouse, f32::NEG_INFINITY),
                wheel([9999.; 2], -80.),
                no_control,
                no_alt,
            ] {
                let current = (panels.timeline.graph_y_zoom, panels.timeline.graph_y_offset);
                assert!(draw(&mut panels, &snapshot, input, floating, &mut atlas).is_empty());
                assert_eq!(
                    (panels.timeline.graph_y_zoom, panels.timeline.graph_y_offset),
                    current
                );
            }
            // Start the actual key gesture, then keep the pointer still while wheeling.
            let anchor = point(panels.effect_timeline.rects[&(1, "speed".into(), 20)]);
            let actions = draw(&mut panels, &snapshot, press(anchor), floating, &mut atlas);
            run(&mut engine, actions);
            assert!(panels.effect_timeline.editing());
            let current = (panels.timeline.graph_y_zoom, panels.timeline.graph_y_offset);
            let mut input = wheel(anchor, -80.);
            input.held = Some(MouseButton::Left);
            assert!(draw(&mut panels, &snapshot, input, floating, &mut atlas).is_empty());
            assert_eq!(
                (panels.timeline.graph_y_zoom, panels.timeline.graph_y_offset),
                current
            );
            let actions = draw(
                &mut panels,
                &snapshot,
                Input {
                    keys: vec![(Key::Escape, Modifiers::default())],
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            run(&mut engine, actions);
            assert!(!panels.effect_timeline.editing());
            assert_eq!(
                engine.project(),
                &serde_json::from_value::<Project>(snapshot["project"].clone()).unwrap()
            );
            assert!(!engine.can_undo());
        }
    }
}
