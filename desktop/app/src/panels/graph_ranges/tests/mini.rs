use super::*;

fn scene() -> (Engine, Value, Panels) {
    let (engine, mut state, mut p) = fixture();
    state["sampledLayers"] = serde_json::json!([{"id":7,"position":[0.,0.,0.],"opacity":0.5}]);
    p.focused = Some(Panel::Curves);
    (engine, state, p)
}
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
            Panel::Curves,
            [1000., 850.],
            p,
            state,
            input,
        )
    } else {
        crate::panels::chrome(
            &mut list,
            atlas,
            [1200., 850.],
            &Node::dock(Panel::Curves.dock()),
            p,
            state,
            input,
        )
    }
}
fn launch(p: &mut Panels, state: &Value, window: Option<Panel>, atlas: &mut TextAtlas) {
    assert!(render(p, state, &Input::default(), window, atlas).is_empty());
    let (header, _, _) = p
        .animation_controls
        .mini_test_geometry()
        .expect("Mini graph was actually drawn");
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
    assert!(p.timeline.graph_ranges.editing());
    assert!(render(p, state, &Input::default(), window, atlas).is_empty());
    assert!(p.timeline.graph_ranges.editing());
}
fn choose(
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
fn mini_fit_selection_uses_normalized_interval_without_moving_shared_time_or_history() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Curves)] {
            let (engine, state, mut p) = scene();
            p.locale = locale;
            let before = serde_json::to_vec(engine.project()).unwrap();
            let history = engine.history_diagnostics();
            p.timeline.zoom = 4.;
            p.timeline.offset = 20.;
            launch(&mut p, &state, window, &mut atlas);
            let menu = p.timeline.graph_ranges.menu.as_ref().unwrap();
            assert_eq!(menu.id.surface, Surface::Mini(Panel::Curves));
            assert_eq!(menu.options.all.unwrap().x, Range::new(0., 1.).unwrap());
            assert_eq!(
                menu.options.selection.unwrap().x,
                Range::new(-0.05, 0.05).unwrap()
            );
            let id = menu.id.clone();
            choose(&mut p, &state, 1, window, &mut atlas);
            assert!(!p.timeline.graph_ranges.editing());
            assert!(render(&mut p, &state, &Input::default(), window, &mut atlas).is_empty());
            let (_, _, actual) = p.animation_controls.mini_test_geometry().unwrap();
            assert_eq!(actual.x, Range::new(-0.05, 0.05).unwrap());
            assert_eq!((p.timeline.zoom, p.timeline.offset), (4., 20.));
            assert_eq!(
                p.timeline
                    .graph_ranges
                    .entries
                    .iter()
                    .find(|e| e.id == id)
                    .unwrap()
                    .x
                    .range(options().displayed.x),
                actual.x
            );
            assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
            assert_eq!(engine.history_diagnostics(), history);
        }
    }
}

#[test]
fn mini_locks_both_local_axes_during_playback_and_modifier_wheel_without_panel_scroll() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Curves)] {
            let (engine, mut state, mut p) = scene();
            p.locale = locale;
            launch(&mut p, &state, window, &mut atlas);
            let captured = p
                .timeline
                .graph_ranges
                .menu
                .as_ref()
                .unwrap()
                .options
                .displayed;
            choose(&mut p, &state, 3, window, &mut atlas);
            choose(&mut p, &state, 4, window, &mut atlas);
            assert!(
                !p.timeline.graph_ranges.locked_x(),
                "Mini X must not lock the shared composition axis"
            );
            assert!(render(&mut p, &state, &key(Key::Escape), window, &mut atlas).is_empty());
            let scroll = p
                .auxiliary
                .scroll
                .get(&(Panel::Curves as u32))
                .copied()
                .unwrap_or(0.);
            for modifiers in [
                Modifiers {
                    control: true,
                    ..Default::default()
                },
                Modifiers {
                    control: true,
                    alt: true,
                    ..Default::default()
                },
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            ] {
                state["frame"] = 80.into();
                let (_, graph, _) = p.animation_controls.mini_test_geometry().unwrap();
                let mouse = [
                    graph.min[0] + graph.width() * 0.5,
                    graph.min[1] + graph.height() * 0.5,
                ];
                assert!(render(
                    &mut p,
                    &state,
                    &Input {
                        mouse,
                        modifiers,
                        wheel: [0., 4.],
                        ..Default::default()
                    },
                    window,
                    &mut atlas
                )
                .is_empty());
                assert_eq!(
                    p.animation_controls.mini_test_geometry().unwrap().2,
                    captured
                );
                assert_eq!(
                    p.auxiliary
                        .scroll
                        .get(&(Panel::Curves as u32))
                        .copied()
                        .unwrap_or(0.),
                    scroll
                );
            }
            assert_eq!(
                engine.history_diagnostics(),
                fixture().0.history_diagnostics()
            );
        }
    }
}

#[test]
fn mini_menu_owns_ime_and_cancels_changed_key_or_other_window_without_deleting() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for window in [None, Some(Panel::Curves)] {
        let (_, state, mut p) = scene();
        launch(&mut p, &state, window, &mut atlas);
        let before = p.timeline.graph_ranges.entries[0].x;
        let mut ime = key(Key::Space);
        ime.composing = true;
        assert!(render(&mut p, &state, &ime, window, &mut atlas).is_empty());
        assert!(p.timeline.graph_ranges.editing());
        assert_eq!(p.timeline.graph_ranges.entries[0].x, before);
        p.animation_controls
            .select_key_in(&state, 7, Property::Opacity, None, 52);
        assert!(render(&mut p, &state, &key(Key::Delete), window, &mut atlas).is_empty());
        assert!(!p.timeline.graph_ranges.editing());
        p.animation_controls
            .select_key_in(&state, 7, Property::Opacity, None, 32);
        launch(&mut p, &state, window, &mut atlas);
        let other = if window.is_some() {
            None
        } else {
            Some(Panel::Curves)
        };
        assert!(render(&mut p, &state, &key(Key::Delete), other, &mut atlas).is_empty());
        assert!(!p.timeline.graph_ranges.editing());
    }
}

#[test]
fn mini_hover_uses_actual_composition_frame_and_percent_but_last_key_stays_normalized() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Curves)] {
            let (_, state, mut p) = scene();
            p.locale = locale;
            assert!(render(&mut p, &state, &Input::default(), window, &mut atlas).is_empty());
            let (_, graph, _) = p.animation_controls.mini_test_geometry().unwrap();
            let mouse = [
                graph.min[0] + graph.width() * 0.5,
                graph.min[1] + graph.height() * 0.5,
            ];
            assert!(render(
                &mut p,
                &state,
                &Input {
                    mouse,
                    ..Default::default()
                },
                window,
                &mut atlas
            )
            .is_empty());
            let (title, lines) = p.animation_controls.mini_test_readout().unwrap();
            assert!(
                title.contains("42.00 f"),
                "composition offset must occur exactly once: {title}"
            );
            assert_eq!(lines.len(), 2);
            assert!(lines[1].contains("65.0000 %"));
            p.animation_controls
                .select_key_in(&state, 7, Property::Opacity, None, 52);
            assert!(render(
                &mut p,
                &state,
                &Input {
                    mouse,
                    ..Default::default()
                },
                window,
                &mut atlas
            )
            .is_empty());
            let (title, lines) = p.animation_controls.mini_test_readout().unwrap();
            assert!(!title.contains(" f"));
            assert_eq!(lines.len(), 1);
            assert!(!lines[0].contains('%'));
        }
    }
}

#[test]
fn mini_control_drag_inverts_zoomed_axes_and_escape_cancels_the_existing_transaction() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for window in [None, Some(Panel::Curves)] {
        let (mut engine, mut state, mut p) = scene();
        engine
            .apply(Command::Curve {
                object: 7,
                property: Property::Opacity,
                axis: None,
                frame: 32,
                easing: motion_core::Easing {
                    ease: motion_core::Ease::Linear,
                    curve: Some(motion_core::Curve {
                        space: motion_core::CurveSpace::Progress,
                        shape: motion_core::CurveShape::Quadratic {
                            control: [0.5, 0.],
                            start: 0.,
                            end: 1.,
                        },
                    }),
                },
            })
            .unwrap();
        state["project"] = serde_json::to_value(engine.project()).unwrap();
        state["revision"] = engine.revision().into();
        state["timeline_layers"] =
            serde_json::to_value(engine.project().timeline_layers(32.).unwrap()).unwrap();
        launch(&mut p, &state, window, &mut atlas);
        let menu = p.timeline.graph_ranges.menu.as_ref().unwrap();
        let id = menu.id.clone();
        let automatic = menu.options.displayed;
        assert!(render(&mut p, &state, &key(Key::Escape), window, &mut atlas).is_empty());
        assert!(p
            .timeline
            .graph_ranges
            .navigate_mini(&id, automatic, 0, 2., 0.5, 0.));
        assert!(p
            .timeline
            .graph_ranges
            .navigate_mini(&id, automatic, 1, 2., 0., 0.));
        assert!(render(&mut p, &state, &Input::default(), window, &mut atlas).is_empty());
        let (_, graph, bounds) = p.animation_controls.mini_test_geometry().unwrap();
        let point = |value: [f64; 2]| {
            [
                graph.min[0] + bounds.x.fraction(value[0]) as f32 * graph.width(),
                graph.max[1] - bounds.y.fraction(value[1]) as f32 * graph.height(),
            ]
        };
        let mouse = point([0.5, 0.]);
        let actions = render(
            &mut p,
            &state,
            &Input {
                mouse,
                pressed: Some((MouseButton::Left, mouse)),
                ..Default::default()
            },
            window,
            &mut atlas,
        );
        assert!(actions.iter().any(|a| matches!(a, Action::History(2))));
        assert!(p.animation_controls.editing());
        let mouse = point([0.6, 0.4]);
        let actions = render(
            &mut p,
            &state,
            &Input {
                mouse,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            window,
            &mut atlas,
        );
        let actual = actions
            .iter()
            .find_map(|a| match a {
                Action::Edit(Command::Curve {
                    object: 7,
                    property: Property::Opacity,
                    frame: 32,
                    easing,
                    ..
                }) => easing.curve,
                _ => None,
            })
            .expect("drag must emit the original authored curve command");
        let motion_core::CurveShape::Quadratic { control, .. } = actual.shape else {
            panic!("shape changed");
        };
        assert!(
            (control[0] - 0.6).abs() < 1e-5 && (control[1] - 0.4).abs() < 1e-5,
            "{control:?}"
        );
        let actions = render(&mut p, &state, &key(Key::Escape), window, &mut atlas);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::History(4)))
                .count(),
            1
        );
        assert!(!p.animation_controls.editing());
        assert!(
            render(&mut p, &state, &key(Key::Escape), window, &mut atlas)
                .iter()
                .all(|a| !matches!(a, Action::History(4)))
        );
    }
}

#[test]
fn mini_modifier_zoom_and_pan_are_pointer_anchored_and_do_not_scroll_or_move_timeline() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Curves)] {
            let (_, state, mut p) = scene();
            p.locale = locale;
            p.timeline.zoom = 4.;
            p.timeline.offset = 20.;
            assert!(render(&mut p, &state, &Input::default(), window, &mut atlas).is_empty());
            let (_, graph, initial) = p.animation_controls.mini_test_geometry().unwrap();
            let mouse = [
                graph.min[0] + graph.width() * 0.5,
                graph.min[1] + graph.height() * 0.5,
            ];
            let input = |modifiers| Input {
                mouse,
                modifiers,
                wheel: [0., 4.],
                ..Default::default()
            };
            let scroll = p
                .auxiliary
                .scroll
                .get(&(Panel::Curves as u32))
                .copied()
                .unwrap_or(0.);
            assert!(render(
                &mut p,
                &state,
                &input(Modifiers {
                    control: true,
                    ..Default::default()
                }),
                window,
                &mut atlas
            )
            .is_empty());
            let x = p.animation_controls.mini_test_geometry().unwrap().2;
            let factor = 1.15f64.powf(4.);
            assert!((x.x.span() - initial.x.span() / factor).abs() < 1e-7);
            assert!((x.x.at(0.5) - initial.x.at(0.5)).abs() < 1e-7);
            assert_eq!(x.y, initial.y);
            assert!(render(
                &mut p,
                &state,
                &input(Modifiers {
                    control: true,
                    alt: true,
                    ..Default::default()
                }),
                window,
                &mut atlas
            )
            .is_empty());
            let y = p.animation_controls.mini_test_geometry().unwrap().2;
            assert_eq!(x.x, y.x);
            assert!((y.y.span() - x.y.span() / factor).abs() < 1e-7);
            assert!((y.y.at(0.5) - x.y.at(0.5)).abs() < 1e-7);
            assert!(render(
                &mut p,
                &state,
                &input(Modifiers {
                    shift: true,
                    ..Default::default()
                }),
                window,
                &mut atlas
            )
            .is_empty());
            let pan = p.animation_controls.mini_test_geometry().unwrap().2;
            assert!((pan.x.at(0.5) - (y.x.at(0.5) - y.x.span() * 0.4)).abs() < 1e-7);
            assert!((pan.x.span() - y.x.span()).abs() < 1e-7);
            assert_eq!(pan.y, y.y);
            assert_eq!((p.timeline.zoom, p.timeline.offset), (4., 20.));
            assert_eq!(
                p.auxiliary
                    .scroll
                    .get(&(Panel::Curves as u32))
                    .copied()
                    .unwrap_or(0.),
                scroll
            );
        }
    }
}
