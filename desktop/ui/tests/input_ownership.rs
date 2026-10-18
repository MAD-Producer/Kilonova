use motion_ui::{
    dock::DockId,
    input::{Event, Modifiers},
    interaction::{HitRegion, Router, Target},
    paint::PaintList,
    ui::accumulate,
    widgets::{Button, NumberEditor, NumberEvent, NumberOptions},
    Input, Key, MouseButton, Rect, TextAtlas,
};

fn region(window: u64, widget: u64) -> HitRegion {
    let rect = Rect::new((widget - 1) as f32 * 110., 0., 100., 30.);
    HitRegion {
        target: Target {
            window,
            panel: DockId(2),
            widget,
        },
        rect,
        clip: rect,
        layer: 0,
        keyboard: true,
    }
}

fn number_frame(
    router: &mut Router,
    field: &mut NumberEditor,
    input: &mut Input,
    event: Event,
    area: HitRegion,
) -> Vec<NumberEvent> {
    accumulate(input, event);
    router.begin_frame(area.target.window, input, &[area]);
    let result = field.update(
        25.,
        NumberOptions::default(),
        &router.input_for(area.target, input),
        area.rect,
    );
    router.end_frame(input);
    input.begin_frame();
    result
}

#[test]
fn numeric_drag_keeps_capture_through_secondary_buttons_and_commits_once() {
    for window in [0, 2] {
        for secondary in [MouseButton::Right, MouseButton::Middle] {
            let area = region(window, 1);
            let (mut router, mut field, mut input) =
                (Router::default(), NumberEditor::default(), Input::default());
            assert!(number_frame(
                &mut router,
                &mut field,
                &mut input,
                Event::MousePressed {
                    position: [10., 10.],
                    button: MouseButton::Left
                },
                area
            )
            .is_empty());
            assert_eq!(
                number_frame(
                    &mut router,
                    &mut field,
                    &mut input,
                    Event::MouseMoved {
                        position: [30., 10.]
                    },
                    area
                ),
                vec![
                    NumberEvent::Begin { value: 25. },
                    NumberEvent::Change { value: 45. }
                ]
            );
            for event in [
                Event::MousePressed {
                    position: [30., 10.],
                    button: secondary,
                },
                Event::MouseReleased {
                    position: [30., 10.],
                    button: secondary,
                },
            ] {
                assert!(number_frame(&mut router, &mut field, &mut input, event, area).is_empty());
                assert!(field.dragging());
                assert_eq!(router.captured(), Some(area.target));
                assert_eq!(input.held, Some(MouseButton::Left));
            }
            // Capture must retain the original field after the pointer leaves it.
            assert_eq!(
                number_frame(
                    &mut router,
                    &mut field,
                    &mut input,
                    Event::MouseMoved {
                        position: [120., 10.]
                    },
                    area
                ),
                vec![NumberEvent::Change { value: 135. }]
            );
            assert_eq!(
                number_frame(
                    &mut router,
                    &mut field,
                    &mut input,
                    Event::MouseReleased {
                        position: [120., 10.],
                        button: MouseButton::Left
                    },
                    area
                ),
                vec![NumberEvent::Commit { value: 135. }]
            );
            assert!(!field.interacting());
            assert_eq!(router.captured(), None);
            assert!(number_frame(
                &mut router,
                &mut field,
                &mut input,
                Event::MouseMoved {
                    position: [10., 10.]
                },
                area
            )
            .is_empty());
        }
    }
}

#[test]
fn unrelated_release_neither_starts_nor_finishes_a_numeric_gesture() {
    for secondary in [MouseButton::Right, MouseButton::Middle] {
        let mut field = NumberEditor::default();
        let area = region(0, 1).rect;
        field.update(
            25.,
            NumberOptions::default(),
            &Input {
                pressed: Some((MouseButton::Left, [10., 10.])),
                mouse: [10., 10.],
                ..Default::default()
            },
            area,
        );
        assert!(field
            .update(
                25.,
                NumberOptions::default(),
                &Input {
                    released: Some((secondary, [30., 10.])),
                    mouse: [30., 10.],
                    ..Default::default()
                },
                area
            )
            .is_empty());
        assert!(!field.dragging());
        assert!(field.interacting());
        assert!(!field.text.active());
        assert!(field
            .update(
                25.,
                NumberOptions::default(),
                &Input {
                    released: Some((MouseButton::Left, [10., 10.])),
                    mouse: [10., 10.],
                    ..Default::default()
                },
                area
            )
            .is_empty());
        assert!(field.text.active());
    }
}

#[test]
fn secondary_release_preserves_numeric_cancel_and_original_value() {
    for cancel in [
        Event::FocusLost,
        Event::KeyPressed {
            key: Key::Escape,
            modifiers: Modifiers::default(),
        },
    ] {
        let area = region(0, 1);
        let (mut router, mut field, mut input) =
            (Router::default(), NumberEditor::default(), Input::default());
        number_frame(
            &mut router,
            &mut field,
            &mut input,
            Event::MousePressed {
                position: [10., 10.],
                button: MouseButton::Left,
            },
            area,
        );
        number_frame(
            &mut router,
            &mut field,
            &mut input,
            Event::MouseMoved {
                position: [30., 10.],
            },
            area,
        );
        assert!(number_frame(
            &mut router,
            &mut field,
            &mut input,
            Event::MouseReleased {
                position: [30., 10.],
                button: MouseButton::Middle
            },
            area
        )
        .is_empty());
        assert_eq!(
            number_frame(&mut router, &mut field, &mut input, cancel, area),
            vec![NumberEvent::Cancel { value: 25. }]
        );
        assert!(!field.interacting());
    }
}

#[test]
fn button_keeps_its_primary_press_until_matching_release_or_cancellation() {
    let area = region(0, 1).rect;
    let mut paint = PaintList::default();
    let mut atlas = TextAtlas::from_system(1.).expect("system text atlas for button input test");
    for secondary in [MouseButton::Right, MouseButton::Middle] {
        let mut button = Button::default();
        let press = Input {
            mouse: [10., 10.],
            pressed: Some((MouseButton::Left, [10., 10.])),
            held: Some(MouseButton::Left),
            ..Default::default()
        };
        assert!(
            !button
                .draw(&mut paint, &mut atlas, area, "Run", &press, true)
                .clicked
        );
        let secondary_release = Input {
            released: Some((secondary, [10., 10.])),
            pressed: None,
            ..press.clone()
        };
        let response = button.draw(
            &mut paint,
            &mut atlas,
            area,
            "Run",
            &secondary_release,
            true,
        );
        assert!(!response.clicked);
        assert!(response.active);
        let release = Input {
            mouse: [10., 10.],
            released: Some((MouseButton::Left, [10., 10.])),
            ..Default::default()
        };
        assert!(
            button
                .draw(&mut paint, &mut atlas, area, "Run", &release, true)
                .clicked
        );
        assert!(
            !button
                .draw(&mut paint, &mut atlas, area, "Run", &release, true)
                .clicked
        );
        button.draw(&mut paint, &mut atlas, area, "Run", &press, true);
        button.draw(
            &mut paint,
            &mut atlas,
            area,
            "Run",
            &Input {
                focus_lost: true,
                ..Default::default()
            },
            true,
        );
        assert!(
            !button
                .draw(&mut paint, &mut atlas, area, "Run", &release, true)
                .clicked
        );
    }
}

#[test]
fn tab_visits_unique_visible_targets_in_order_in_both_directions_and_windows() {
    let a = region(0, 1);
    let b = region(0, 2);
    let c = region(0, 3);
    let mut clipped = region(0, 4);
    clipped.clip = Rect::new(0., 100., 10., 10.);
    let foreign = region(2, 1);
    let regions = [a, b, a, clipped, foreign, c, b];
    let mut router = Router::default();
    let tab = Input {
        keys: vec![(Key::Tab, Modifiers::default())],
        ..Default::default()
    };
    for expected in [a.target, b.target, c.target, a.target] {
        router.begin_frame(0, &tab, &regions);
        assert_eq!(router.focus(), Some(expected));
    }
    let reverse = Input {
        keys: vec![(
            Key::Tab,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        )],
        ..Default::default()
    };
    for expected in [c.target, b.target, a.target] {
        router.begin_frame(0, &reverse, &regions);
        assert_eq!(router.focus(), Some(expected));
    }
    router.begin_frame(2, &tab, &regions);
    assert_eq!(router.focus(), Some(foreign.target));
    router.set_modal(Some((0, a.target.panel)));
    router.begin_frame(2, &tab, &regions);
    assert_eq!(router.focus(), None);
}

#[test]
fn tab_defers_to_ime_repeats_and_other_modifier_chords() {
    let a = region(0, 1);
    let b = region(0, 2);
    let mut router = Router::default();
    let tab = Input {
        keys: vec![(Key::Tab, Modifiers::default())],
        ..Default::default()
    };
    router.begin_frame(0, &tab, &[a, b]);
    for input in [
        Input {
            composing: true,
            ..tab.clone()
        },
        Input {
            ime_consumed: true,
            ..tab.clone()
        },
        Input {
            key_repeats: vec![true],
            ..tab.clone()
        },
        Input {
            keys: vec![(
                Key::Tab,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        },
        Input {
            keys: vec![(
                Key::Tab,
                Modifiers {
                    alt: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        },
    ] {
        router.begin_frame(0, &input, &[a, b]);
        assert_eq!(router.focus(), Some(a.target));
    }
}

#[test]
fn a_different_windows_release_does_not_drop_primary_capture() {
    let a = region(0, 1);
    let foreign = region(2, 1);
    let mut router = Router::default();
    let click = Input {
        mouse: [10., 10.],
        pressed: Some((MouseButton::Left, [10., 10.])),
        ..Default::default()
    };
    router.begin_frame(0, &click, &[a, foreign]);
    for input in [
        Input {
            released: Some((MouseButton::Left, [10., 10.])),
            ..Default::default()
        },
        Input {
            focus_lost: true,
            ..Default::default()
        },
    ] {
        router.begin_frame(2, &input, &[a, foreign]);
        router.end_frame(&input);
        assert_eq!(router.captured(), Some(a.target));
    }
    router.begin_frame(0, &Input::default(), &[a]);
    router.end_frame(&Input {
        released: Some((MouseButton::Left, [10., 10.])),
        ..Default::default()
    });
    assert_eq!(router.captured(), None);
}

#[test]
fn event_accumulation_preserves_the_first_held_button_and_its_drag_origin() {
    for owner in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        for secondary in [MouseButton::Left, MouseButton::Right, MouseButton::Middle]
            .into_iter()
            .filter(|b| *b != owner)
        {
            let mut input = Input::default();
            accumulate(
                &mut input,
                Event::MousePressed {
                    position: [10., 10.],
                    button: owner,
                },
            );
            input.begin_frame();
            accumulate(
                &mut input,
                Event::MouseMoved {
                    position: [30., 10.],
                },
            );
            for event in [
                Event::MousePressed {
                    position: [30., 10.],
                    button: secondary,
                },
                Event::MouseReleased {
                    position: [30., 10.],
                    button: secondary,
                },
            ] {
                accumulate(&mut input, event);
                assert_eq!(input.held, Some(owner));
                assert_eq!(input.pointer_origin, Some([10., 10.]));
                assert_eq!(input.pointer_travel, 20.);
                input.begin_frame();
            }
            accumulate(
                &mut input,
                Event::MouseReleased {
                    position: [30., 10.],
                    button: owner,
                },
            );
            assert_eq!(input.held, None);
            assert_eq!(input.pointer_origin, None);
        }
    }
}
