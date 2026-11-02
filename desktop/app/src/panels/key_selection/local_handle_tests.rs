//! Main/floating chrome owns handles by exact local interval identity.
use super::*;
use motion_core::{CurveShape, Easing};
#[cfg(target_os = "windows")]
#[path = "local_handle_native_tests.rs"]
mod native;
#[path = "local_handle_observation_tests.rs"]
mod observation;

fn fixture(reverse: bool) -> Engine {
    let mut p = mapped(reverse).snapshot();
    let t = &mut p.layers[0].transform.position;
    t.keys.clear();
    for (frame, x) in [(-3, -3.), (3, 3.), (13, 13.), (25, 16.)] {
        t.upsert(frame, [x, 20., 0.], Ease::Linear).unwrap();
    }
    // Keep all negative local IDs visible at fractional composition times.
    let direction = if reverse {
        TimeDirection::Reverse
    } else {
        TimeDirection::Forward
    };
    p.layers[0].time_mapping.as_mut().unwrap().clock =
        LayerClock::new(TimeStretch::new(3, 2).unwrap(), direction, 40., 0.).unwrap();
    Engine::new(p).unwrap()
}
fn ready(s: &Value, locale: Locale, speed: bool) -> Panels {
    let mut p = setup(locale, true, false);
    p.timeline.speed_graph = speed;
    p.selected_keys
        .apply_addressed(s, [local(7, None, 3)], Mode::Replace)
        .unwrap();
    p
}
fn points(paint: &PaintList) -> Vec<[f32; 2]> {
    let radius = paint.styles().metric("graph.handle_radius", 4.);
    paint
        .shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Procedural {
                area,
                kind: Pattern::Box { radius: r, .. },
                ..
            } if *r == radius
                && (area.width() - radius * 2.).abs() < 0.001
                && (area.height() - radius * 2.).abs() < 0.001 =>
            {
                Some([
                    (area.min[0] + area.max[0]) * 0.5,
                    (area.min[1] + area.max[1]) * 0.5,
                ])
            }
            _ => None,
        })
        .collect()
}
fn worker(e: &mut Engine, actions: Vec<Action>) -> Vec<Command> {
    let mut edits = vec![];
    for action in actions {
        match action {
            Action::Edit(c) => {
                e.apply(c.clone()).unwrap();
                edits.push(c);
            }
            Action::History(2) => e.begin_gesture().unwrap(),
            Action::History(3) => e.end_gesture(true).unwrap(),
            Action::History(4) => e.end_gesture(false).unwrap(),
            other => panic!("handle must own press without seek/marquee: {other:?}"),
        }
    }
    edits
}
fn controls(e: Easing) -> [[f64; 2]; 2] {
    let CurveShape::Cubic {
        control1, control2, ..
    } = e.curve.unwrap().shape
    else {
        panic!("cubic required")
    };
    [control1, control2]
}

#[test]
fn mapped_handles_only_begin_in_visible_plot_and_keep_capture_outside_it() {
    let mut e = fixture(false);
    let before = e.snapshot();
    let s = view(&e);
    let mut p = ready(&s, Locale::En, false);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let source = crate::panels::animation_data::channel(&s, 7, Property::Position, None).unwrap();
    let plot = crate::panels::graph_handles::Plot {
        area: motion_ui::Rect::new(500., 100., 400., 300.),
        offset: 55.,
        ppf: 10.,
        low: -10.,
        high: 30.,
        speed: false,
        fps: 30.,
        scale: 1.,
    };
    let mut render = |p: &mut Panels, s: &Value, input: &Input| {
        let mut paint = PaintList::default();
        let mut actions = vec![];
        crate::panels::graph_handles::draw(
            &mut paint,
            &mut atlas,
            p,
            source,
            s,
            7,
            Property::Position,
            None,
            0,
            plot,
            input,
            false,
            &mut actions,
        );
        (paint, actions)
    };
    let (paint, actions) = render(&mut p, &s, &Input::default());
    assert!(actions.is_empty());
    let controls = points(&paint);
    assert_eq!(controls.len(), 4);
    let outside = controls[2];
    assert!(!plot.area.contains(outside));
    let modifiers = Modifiers {
        alt: true,
        ..Default::default()
    };
    let (_, actions) = render(&mut p, &s, &pointer(outside, true, false, modifiers));
    assert!(actions.is_empty());
    assert!(p.timeline.graph_drag.is_none());
    let inside = controls[3];
    assert!(plot.area.contains(inside));
    let (_, actions) = render(&mut p, &s, &pointer(inside, true, false, modifiers));
    assert!(p.timeline.graph_drag.is_some());
    worker(&mut e, actions);
    let next = [plot.area.min[0] - 12., inside[1] - 12.];
    let (_, actions) = render(&mut p, &s, &pointer(next, false, false, modifiers));
    assert_eq!(worker(&mut e, actions).len(), 1);
    assert!(p.timeline.graph_drag.is_some());
    let (_, actions) = render(&mut p, &view(&e), &pointer(next, false, true, modifiers));
    worker(&mut e, actions);
    assert!(p.timeline.graph_drag.is_none());
    assert_ne!(e.project(), &before);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
}

#[test]
fn handles_in_value_and_speed_graphs_edit_local_intervals_without_seek_and_commit_one_undo() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for floating in [false, true] {
            for speed in [false, true] {
                for alt in [false, true] {
                    for incoming in [false, true] {
                        let mut e = fixture(reverse);
                        let before = e.snapshot();
                        let s = view(&e);
                        let mut p =
                            ready(&s, if floating { Locale::En } else { Locale::Zh }, speed);
                        let (paint, actions) =
                            draw(&mut p, &s, Input::default(), floating, &mut atlas);
                        worker(&mut e, actions);
                        let handles = points(&paint);
                        assert_eq!(
                            handles.len(),
                            4,
                            "{reverse}/{floating}/{speed}: {handles:?}"
                        );
                        let at = handles[if incoming { 1 } else { 2 }];
                        let modifiers = Modifiers {
                            alt,
                            ..Default::default()
                        };
                        let (_, actions) = draw(
                            &mut p,
                            &s,
                            pointer(at, true, false, modifiers),
                            floating,
                            &mut atlas,
                        );
                        assert!(p.timeline.graph_drag.is_some());
                        worker(&mut e, actions);
                        let next = [at[0] + if reverse { -5. } else { 5. }, at[1] - 12.];
                        let (_, actions) = draw(
                            &mut p,
                            &s,
                            pointer(next, false, false, modifiers),
                            floating,
                            &mut atlas,
                        );
                        let edits = worker(&mut e, actions);
                        assert_eq!(edits.len(), if alt { 1 } else { 2 });
                        assert!(edits.iter().all(|c| matches!(
                            c,
                            Command::LocalKeyframes {
                                edit: LocalKeyEdit::Handle { .. },
                                ..
                            }
                        )));
                        assert!(matches!(
                            edits.last().unwrap(),
                            Command::LocalKeyframes {
                                edit: LocalKeyEdit::Handle {
                                    frame,
                                    incoming: side,
                                    ..
                                },
                                ..
                            } if *frame==if incoming {-3}else{3} && *side==incoming
                        ));
                        let (_, actions) = draw(
                            &mut p,
                            &view(&e),
                            pointer(next, false, true, modifiers),
                            floating,
                            &mut atlas,
                        );
                        worker(&mut e, actions);
                        assert!(p.timeline.graph_drag.is_none());
                        let after = e.snapshot();
                        assert_ne!(after, before);
                        assert_eq!(
                            p.selected_keys.local_selections(),
                            vec![(key(7, None, 0).target, vec![3])]
                        );
                        e.undo().unwrap();
                        assert_eq!(e.project(), &before);
                        assert!(!e.can_undo());
                        e.redo().unwrap();
                        assert_eq!(e.project(), &after);
                    }
                }
            }
        }
    }
}

#[test]
fn mapped_handle_cancels_on_input_or_clock_selection_and_window_geometry_changes() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reason in [
        "lost_release",
        "focus",
        "ime",
        "clock",
        "playhead",
        "selection",
        "lock",
        "zoom",
        "escape",
        "missing_layer",
    ] {
        let mut e = fixture(false);
        let before = e.snapshot();
        let s = view(&e);
        let mut p = ready(&s, Locale::En, false);
        let (paint, _) = draw(&mut p, &s, Input::default(), false, &mut atlas);
        let at = points(&paint)[2];
        let modifiers = Modifiers {
            alt: true,
            ..Default::default()
        };
        let (_, actions) = draw(
            &mut p,
            &s,
            pointer(at, true, false, modifiers),
            false,
            &mut atlas,
        );
        worker(&mut e, actions);
        let next = [at[0] + 5., at[1] - 12.];
        let (_, actions) = draw(
            &mut p,
            &s,
            pointer(next, false, false, modifiers),
            false,
            &mut atlas,
        );
        worker(&mut e, actions);
        assert_ne!(e.project(), &before, "{reason}");
        let mut s = view(&e);
        let mut input = pointer(next, false, false, modifiers);
        match reason {
            "lost_release" => input.held = None,
            "focus" => input.focus_lost = true,
            "ime" => input.composing = true,
            "clock" => s["project"]["layers"][0]["time_mapping"] = Value::Null,
            "playhead" => s["frame"] = json!(1.),
            "selection" => p
                .selected_keys
                .apply_addressed(&s, [local(7, None, 13)], Mode::Replace)
                .unwrap(),
            "lock" => s["project"]["layers"][0]["locked"] = json!(true),
            "zoom" => p.timeline.zoom = 2.,
            "escape" => input
                .keys
                .push((motion_ui::input::Key::Escape, Modifiers::default())),
            "missing_layer" => p.select_only(None),
            _ => unreachable!(),
        }
        let (_, actions) = draw(&mut p, &s, input, false, &mut atlas);
        assert!(p.timeline.graph_drag.is_none(), "{reason}");
        assert!(
            matches!(actions.as_slice(), [Action::History(4)]),
            "{reason}: {actions:?}"
        );
        worker(&mut e, actions);
        assert_eq!(e.project(), &before, "{reason}");
        assert!(!e.can_undo());
        let (_, actions) = draw(&mut p, &view(&e), Input::default(), false, &mut atlas);
        assert!(
            actions.is_empty(),
            "cancel only once: {reason}: {actions:?}"
        );
    }
}

#[test]
fn modifier_constraints_and_automatic_owner_keep_local_semantics_when_reversed() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for control in [false, true] {
            for shift in [false, true] {
                let mut project = fixture(reverse).snapshot();
                for f in [3, 13] {
                    project.layers[0]
                        .transform
                        .position
                        .set_auto_bezier(f, true)
                        .unwrap();
                }
                let mut e = Engine::new(project).unwrap();
                let before = e.snapshot();
                let s = view(&e);
                let mut p = ready(&s, Locale::En, false);
                let (paint, _) = draw(&mut p, &s, Input::default(), false, &mut atlas);
                let at = points(&paint)[2];
                let modifiers = Modifiers {
                    alt: true,
                    control,
                    shift,
                    ..Default::default()
                };
                let (_, actions) = draw(
                    &mut p,
                    &s,
                    pointer(at, true, false, modifiers),
                    false,
                    &mut atlas,
                );
                worker(&mut e, actions);
                let next = [at[0] + if reverse { -3. } else { 3. }, at[1] - 12.];
                let (_, actions) = draw(
                    &mut p,
                    &s,
                    pointer(next, false, true, modifiers),
                    false,
                    &mut atlas,
                );
                worker(&mut e, actions);
                let t = &e.project().layers[0].transform.position;
                assert!(!t.keys[1].auto_bezier);
                assert!(t.keys[2].auto_bezier);
                let previous = controls(
                    before.layers[0]
                        .transform
                        .position
                        .effective_easing(1)
                        .unwrap(),
                );
                let now = controls(t.effective_easing(1).unwrap());
                assert_eq!(now[1], previous[1]);
                if shift {
                    let angle = now[0][1].atan2(now[0][0]) / (std::f64::consts::PI / 12.);
                    assert!((angle - angle.round()).abs() < 0.000001);
                } else if control {
                    assert_eq!(now[0][0], previous[0][0]);
                }
                assert_eq!(
                    t.effective_easing(0).unwrap(),
                    before.layers[0]
                        .transform
                        .position
                        .effective_easing(0)
                        .unwrap()
                );
            }
        }
    }
}
