use super::*;
use motion_core::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};

fn mapped(reverse: bool) -> (Engine, Value) {
    let (e, catalogue) = fixture();
    let mut project = e.snapshot();
    let layer = &mut project.layers[0];
    layer.timeline.as_mut().unwrap().offset_frame = 2;
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            if reverse { 40. } else { 0. },
            0.,
        )
        .unwrap(),
        remap: None,
    });
    let track = &mut layer.effects[0].params.get_mut("speed").unwrap().track;
    track.keys[0].frame = 3;
    track.keys[1].frame = 13;
    (Engine::new(project).unwrap(), catalogue)
}
fn selected(s: &Value, catalogue: &Value, locale: Locale) -> Panels {
    let mut p = setup(catalogue, locale);
    p.effect_controls.choose_parameter(s, 7, 1, "speed");
    p.timeline.set_graph_mode(true);
    p.selected_keys
        .apply_addressed(
            s,
            [key_selection::LocalKey {
                target: motion_core::KeyframeTarget::Effect {
                    object: 7,
                    effect: 1,
                    param: "speed".into(),
                },
                frame: 3,
            }
            .into()],
            key_selection::Mode::Replace,
        )
        .unwrap();
    p
}

#[test]
fn fractional_effect_handles_edit_the_local_interval_and_undo_once_in_both_owners() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for floating in [false, true] {
            for locale in [Locale::Zh, Locale::En] {
                let (mut e, catalogue) = mapped(reverse);
                let before = e.snapshot();
                let s = state(&e);
                let mut p = selected(&s, &catalogue, locale);
                draw(&mut p, &s, Input::default(), floating, &mut atlas);
                let start = point(p.effect_timeline.rects[&(1, "speed".into(), -100)]);
                run(&mut e, draw(&mut p, &s, press(start), floating, &mut atlas));
                let drag = p
                    .effect_timeline
                    .handle
                    .as_ref()
                    .expect("actual visible handle hit");
                assert_eq!(drag.local.unwrap().0, 3);
                assert_eq!(drag.duration, if reverse { -15. } else { 15. });
                let end = [start[0] + if reverse { -5. } else { 5. }, start[1] - 12.];
                let actions = draw(
                    &mut p,
                    &s,
                    Input {
                        mouse: end,
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                );
                assert!(actions.iter().any(|a|matches!(a,Action::Edit(Command::LocalKeyframes {edit:motion_core::LocalKeyEdit::Curve {frames,..},..}) if frames==&[3])));
                run(&mut e, actions);
                let s = state(&e);
                run(
                    &mut e,
                    draw(
                        &mut p,
                        &s,
                        Input {
                            mouse: end,
                            released: Some((MouseButton::Left, end)),
                            ..Default::default()
                        },
                        floating,
                        &mut atlas,
                    ),
                );
                let track = &e.project().layers[0].effects[0].params["speed"].track;
                assert_ne!(
                    track.keys[0].curve,
                    before.layers[0].effects[0].params["speed"].track.keys[0].curve
                );
                assert_eq!(
                    track.keys[1],
                    before.layers[0].effects[0].params["speed"].track.keys[1]
                );
                assert!(e.undo().unwrap());
                assert_eq!(e.project(), &before);
                assert!(!e.can_undo());
            }
        }
    }
}

#[test]
fn clock_change_and_focus_loss_cancel_active_local_handle_without_committing() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for clock_change in [false, true] {
        let (mut e, catalogue) = mapped(true);
        let before = e.snapshot();
        let s = state(&e);
        let mut p = selected(&s, &catalogue, Locale::En);
        draw(&mut p, &s, Input::default(), false, &mut atlas);
        let start = point(p.effect_timeline.rects[&(1, "speed".into(), -100)]);
        run(&mut e, draw(&mut p, &s, press(start), false, &mut atlas));
        let next = [start[0] - 5., start[1] - 12.];
        run(
            &mut e,
            draw(
                &mut p,
                &s,
                Input {
                    mouse: next,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                false,
                &mut atlas,
            ),
        );
        let mut s = state(&e);
        if clock_change {
            s["project"]["layers"][0]["time_mapping"] = serde_json::Value::Null;
        }
        let ppf = p.timeline.zoom_track.unwrap().width() / 180.;
        let mut actions = vec![];
        graph::update_handle(
            &mut p,
            &s,
            &Input {
                focus_lost: !clock_change,
                ..Default::default()
            },
            ppf,
            0.,
            false,
            30.,
            &mut actions,
        );
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::History(4)));
        run(&mut e, actions);
        assert_eq!(e.project(), &before);
        assert!(p.effect_timeline.handle.is_none());
        assert!(!e.can_undo());
    }
}
