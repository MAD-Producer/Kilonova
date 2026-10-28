use super::*;

fn draw(h: &mut Harness, input: Input) -> Vec<Action> {
    h.state = h.engine.state().unwrap();
    draw_state(h, input)
}
fn draw_state(h: &mut Harness, input: Input) -> Vec<Action> {
    let mut paint = PaintList::default();
    if h.floating {
        floating_chrome(
            &mut paint,
            &mut h.atlas,
            Panel::Audio,
            [320., 600.],
            &mut h.panels,
            &h.state,
            &input,
        )
    } else {
        chrome(
            &mut paint,
            &mut h.atlas,
            [900., 600.],
            &Node::dock(Panel::Audio.dock()),
            &mut h.panels,
            &h.state,
            &input,
        )
    }
}
#[test]
fn audio_monitor_balance_scrub_is_one_history_step_and_cancel_restores_in_both_hosts_and_languages()
{
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let mut h = Harness::new(floating, locale);
            assert!(draw(&mut h, Input::default()).is_empty());
            let field = h.panels.auxiliary.audio.balance_rect.unwrap();
            let start = [
                (field.min[0] + field.max[0]) * 0.5,
                (field.min[1] + field.max[1]) * 0.5,
            ];
            let before = h.state["project"].clone();
            let actions = draw(&mut h, pointer(start, true, false));
            h.run(actions);
            let end = [start[0] + 30., start[1]];
            let actions = draw(
                &mut h,
                Input {
                    mouse: end,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            );
            h.run(actions);
            let actions = draw(&mut h, pointer(end, false, true));
            h.run(actions);
            let changed = h.engine.state().unwrap()["project"].clone();
            assert_ne!(changed, before);
            assert_eq!(
                changed["layers"][0]["audio_levels"]["balance"]
                    .as_f64()
                    .unwrap() as f32,
                0.3f32
            );
            assert!(!h.panels.auxiliary.audio.editing());
            assert_eq!(h.engine.history(0).unwrap()["project"], before);
            assert_eq!(h.engine.history(1).unwrap()["project"], changed);
            h.engine.save().unwrap();
            assert_eq!(
                serde_json::to_value(
                    motion_core::storage::load(std::path::Path::new(
                        h.state["root"].as_str().unwrap()
                    ))
                    .unwrap()
                )
                .unwrap(),
                changed
            );
            let actions = draw(&mut h, pointer(start, true, false));
            h.run(actions);
            let actions = draw(
                &mut h,
                Input {
                    mouse: end,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            );
            h.run(actions);
            let actions = draw(
                &mut h,
                Input {
                    keys: vec![(Key::Escape, Default::default())],
                    ..Default::default()
                },
            );
            h.run(actions);
            assert_eq!(h.engine.state().unwrap()["project"], changed);
            assert!(!h.panels.auxiliary.audio.editing());
        }
    }
}

#[test]
fn audio_monitor_balance_typing_outside_commit_ime_invalid_value_and_focus_loss_preserve_history() {
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let mut h = Harness::new(floating, locale);
            draw(&mut h, Input::default());
            let field = h.panels.auxiliary.audio.balance_rect.unwrap();
            let p = [
                (field.min[0] + field.max[0]) * 0.5,
                (field.min[1] + field.max[1]) * 0.5,
            ];
            let original = h.engine.state().unwrap()["project"].clone();
            for input in [
                pointer(p, true, false),
                pointer(p, false, true),
                Input {
                    text: "-50".into(),
                    ..Default::default()
                },
                pointer([field.min[0] - 20., field.min[1] + 2.], true, false),
            ] {
                let actions = draw(&mut h, input);
                h.run(actions);
            }
            let changed = h.engine.state().unwrap()["project"].clone();
            assert_eq!(changed["layers"][0]["audio_levels"]["balance"], -0.5);
            assert!(!h.panels.auxiliary.audio.editing());
            for input in [
                pointer(p, true, false),
                pointer(p, false, true),
                Input {
                    text: "999".into(),
                    ..Default::default()
                },
                Input {
                    keys: vec![(Key::Enter, Default::default())],
                    composing: true,
                    ..Default::default()
                },
                Input {
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                },
            ] {
                let actions = draw(&mut h, input);
                h.run(actions);
            }
            assert_eq!(
                h.engine.state().unwrap()["project"],
                changed,
                "invalid or composing text must not edit"
            );
            let actions = draw(
                &mut h,
                Input {
                    focus_lost: true,
                    ..Default::default()
                },
            );
            h.run(actions);
            assert!(!h.panels.auxiliary.audio.editing());
            assert_eq!(h.engine.history(0).unwrap()["project"], original);
            assert_eq!(h.engine.history(1).unwrap()["project"], changed);
        }
    }
}
#[test]
fn audio_monitor_balance_unsupported_and_locked_do_not_write_history_or_consume_text() {
    for floating in [false, true] {
        let mut h = Harness::new(floating, Locale::Zh);
        draw(&mut h, Input::default());
        let field = h.panels.auxiliary.audio.balance_rect.unwrap();
        let point = [field.min[0] + 2., field.min[1] + 2.];
        let before = h.state["project"].clone();
        h.state["capabilities"]["audio"]["balance"]["supported"] = false.into();
        assert!(draw_state(&mut h, pointer(point, true, false)).is_empty());
        assert!(!h.panels.auxiliary.audio.editing());
        h.state["capabilities"]["audio"]["balance"]["supported"] = true.into();
        h.state["project"]["layers"][0]["locked"] = true.into();
        assert!(draw_state(&mut h, pointer(point, true, false)).is_empty());
        assert!(!h.panels.auxiliary.audio.editing());
        assert_eq!(h.engine.state().unwrap()["project"], before);
    }
}
