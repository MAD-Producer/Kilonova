use super::*;
use motion_ui::input::{Key, Modifiers};
#[test]
fn zoom_preserves_pointer_time_and_rejects_nonfinite_values() {
    let mut view = types::TimelineState {
        zoom: 4.,
        offset: 100.,
        ..Default::default()
    };
    for fraction in [0., 0.1, 0.5, 0.9, 1.] {
        view.zoom = 4.;
        view.offset = 100.;
        let before = view.offset + 1200. / view.zoom * fraction;
        zoom_at(&mut view, 1200., 5., fraction);
        assert!((view.offset + 1200. / view.zoom * fraction - before).abs() < 0.001);
    }
    let old = (view.zoom, view.offset);
    for (total, next, fraction) in [
        (f32::NAN, 1., 0.5),
        (0., 1., 0.5),
        (1200., f32::INFINITY, 0.5),
        (1200., 1., f32::NAN),
    ] {
        zoom_at(&mut view, total, next, fraction);
        assert_eq!((view.zoom, view.offset), old);
    }
    zoom_at(&mut view, 1200., 1e30, 0.5);
    assert!(view.zoom <= 1200. && view.offset.is_finite());
}
#[test]
fn single_frame_toggle_and_independent_clip_curve_ranges_do_not_edit_project() {
    let state = serde_json::json!({"frame":237.5,"project":{"frames":1200,"layers":[]}});
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        selected: Some(7),
        ..Default::default()
    };
    panels.timeline.zoom = 4.;
    panels.timeline.offset = 100.;
    let input = Input::default();
    assert!(shortcut(
        Shortcut::ToggleFrameView,
        &input,
        &mut panels,
        &state
    ));
    assert_eq!(
        (panels.timeline.zoom, panels.timeline.offset),
        (1200., 237.)
    );
    panels.timeline.set_graph_mode(true);
    shortcut(Shortcut::ToggleFrameView, &input, &mut panels, &state);
    assert_eq!(panels.timeline.zoom, 1.);
    panels.timeline.set_graph_mode(false);
    assert_eq!(
        (panels.timeline.zoom, panels.timeline.offset),
        (1200., 237.)
    );
    shortcut(Shortcut::ToggleFrameView, &input, &mut panels, &state);
    assert_eq!((panels.timeline.zoom, panels.timeline.offset), (1., 0.));
    assert_eq!(panels.selected, Some(7));
}
#[test]
fn real_window_shortcuts_obey_text_ime_repeat_and_focus() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let state = serde_json::json!({"frame":100,"project":{"frames":1200,"fps":30,"layers":[]}});
    for floating in [false, true] {
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        let mut render = |panels: &mut Panels, input: &Input| {
            let mut list = PaintList::default();
            if floating {
                floating_chrome(
                    &mut list,
                    &mut atlas,
                    Panel::Timeline,
                    [1280., 800.],
                    panels,
                    &state,
                    input,
                )
            } else {
                chrome(
                    &mut list,
                    &mut atlas,
                    [1280., 800.],
                    &dock::Node::dock(Panel::Timeline.dock()),
                    panels,
                    &state,
                    input,
                )
            }
        };
        render(&mut panels, &Input::default());
        let input = Input {
            keys: vec![(Key::Equal, Modifiers::default())],
            ..Default::default()
        };
        assert!(render(&mut panels, &input).is_empty());
        assert_eq!(panels.timeline.zoom, 1.25);
        let zoom = panels.timeline.zoom;
        for (composing, ime_consumed, repeat) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            assert!(render(
                &mut panels,
                &Input {
                    composing,
                    ime_consumed,
                    key_repeats: vec![repeat],
                    ..input.clone()
                }
            )
            .is_empty());
            assert_eq!(panels.timeline.zoom, zoom);
        }
        panels.timeline.timeline_search = true;
        render(&mut panels, &input);
        assert_eq!(panels.timeline.zoom, zoom);
        panels.timeline.timeline_search = false;
        render(
            &mut panels,
            &Input {
                keys: vec![(Key::Minus, Modifiers::default())],
                ..Default::default()
            },
        );
        assert_eq!(panels.timeline.zoom, 1.);
        render(
            &mut panels,
            &Input {
                keys: vec![(Key::Semicolon, Modifiers::default())],
                ..Default::default()
            },
        );
        assert_eq!(
            (panels.timeline.zoom, panels.timeline.offset),
            (1200., 100.)
        );
        panels.timeline.zoom = 4.;
        panels.timeline.offset = 100.;
        let track = panels.timeline.zoom_track.unwrap();
        let fraction = 0.3;
        let mouse = [
            track.min[0] + track.width() * fraction,
            track.min[1] + track.height() * 0.5,
        ];
        let before = panels.timeline.offset + 1200. / panels.timeline.zoom * fraction;
        assert!(render(
            &mut panels,
            &Input {
                mouse,
                wheel: [0., -12.],
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
                ..Default::default()
            }
        )
        .is_empty());
        assert!(
            (panels.timeline.offset + 1200. / panels.timeline.zoom * fraction - before).abs()
                < 0.001
        );
        assert_eq!(state["frame"], 100);
    }
}
