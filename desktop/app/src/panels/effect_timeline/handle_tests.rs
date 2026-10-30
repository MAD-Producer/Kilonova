use super::*;

#[test]
fn automatic_handle_input_preserves_neighbours_and_cancels_in_both_owners_and_languages() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            for incoming in [false, true] {
                // Commit, Escape, focus loss, and an invalidated document.
                for finish in 0..4 {
                    let (e, catalogue) = fixture();
                    let mut project = e.snapshot();
                    let track = &mut project.layers[0].effects[0]
                        .params
                        .get_mut("speed")
                        .unwrap()
                        .track;
                    for frame in [10, 70] {
                        track.set_auto_bezier(frame, true).unwrap();
                    }
                    let mut e = Engine::new(project).unwrap();
                    let before = e.snapshot();
                    let mut p = setup(&catalogue, locale);
                    let s = state(&e);
                    choose(&mut p, &s, &Target::new(&s, 7, 1, "speed"), Some(20));
                    p.timeline.set_graph_mode(true);
                    draw(&mut p, &s, Input::default(), floating, &mut atlas);
                    let start = point(
                        p.effect_timeline.rects[&(1, "speed".into(), -100 - i32::from(incoming))],
                    );
                    run(&mut e, draw(&mut p, &s, press(start), floating, &mut atlas));
                    assert!(p.effect_timeline.handle.is_some());
                    let end = [start[0] + 18., start[1] - 12.];
                    let s = state(&e);
                    run(
                        &mut e,
                        draw(
                            &mut p,
                            &s,
                            Input {
                                mouse: end,
                                held: Some(MouseButton::Left),
                                ..Default::default()
                            },
                            floating,
                            &mut atlas,
                        ),
                    );
                    let keys = &e.project().layers[0].effects[0].params["speed"].track.keys;
                    assert!(
                        !keys[usize::from(incoming)].auto_bezier,
                        "drag did not convert its owner"
                    );
                    assert!(
                        keys[usize::from(!incoming)].auto_bezier,
                        "drag converted its unselected neighbour"
                    );
                    let mut s = state(&e);
                    let input = match finish {
                        0 => Input {
                            mouse: end,
                            released: Some((MouseButton::Left, end)),
                            ..Default::default()
                        },
                        1 => Input {
                            keys: vec![(Key::Escape, Default::default())],
                            ..Default::default()
                        },
                        2 => Input {
                            focus_lost: true,
                            ..Default::default()
                        },
                        _ => {
                            s["document_generation"] =
                                (s["document_generation"].as_u64().unwrap() + 1).into();
                            Input::default()
                        }
                    };
                    run(&mut e, draw(&mut p, &s, input, floating, &mut atlas));
                    assert!(
                        p.effect_timeline.handle.is_none(),
                        "finish={finish} incoming={incoming} floating={floating} locale={locale:?}"
                    );
                    let settled = draw(&mut p, &s, Input::default(), floating, &mut atlas);
                    assert!(!settled.iter().any(|a| matches!(a, Action::History(4))));
                    assert!(p.effect_timeline.handle.is_none());
                    if finish == 0 {
                        assert_ne!(e.project(), &before);
                        e.undo().unwrap();
                    }
                    assert_eq!(
                        e.project(),
                        &before,
                        "finish={finish} incoming={incoming} floating={floating} locale={locale:?}"
                    );
                    assert!(!e.can_undo());
                }
            }
        }
    }
}
