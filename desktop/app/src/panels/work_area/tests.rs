use super::*;
use motion_core::{Engine, Project};
use motion_ui::input::Modifiers;

#[cfg(target_os = "windows")]
mod native;

fn fixture() -> Value {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    project.work_area = WorkArea {
        in_frame: 20,
        out_frame: 80,
    };
    serde_json::json!({"root":"range-test", "composition":"comp-main", "revision":0, "frame":40., "project":project})
}
fn pointer(point: [f32; 2], release: bool) -> Input {
    Input {
        mouse: point,
        held: (!release).then_some(MouseButton::Left),
        released: release.then_some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn draw(panels: &mut Panels, state: &Value, input: &Input) -> Vec<Action> {
    let mut out = vec![];
    render(
        &mut PaintList::default(),
        Rect::new(0., 0., 240., 14.),
        panels,
        state,
        input,
        2.,
        0.,
        &mut out,
    );
    out
}
#[test]
fn range_edges_and_translation_commit_once_without_seek_or_early_mutation() {
    for (press, point, expected) in [
        (
            40.,
            60.,
            WorkArea {
                in_frame: 30,
                out_frame: 80,
            },
        ),
        (
            160.,
            260.,
            WorkArea {
                in_frame: 20,
                out_frame: 120,
            },
        ),
        (
            100.,
            -100.,
            WorkArea {
                in_frame: 0,
                out_frame: 60,
            },
        ),
        (
            40.,
            200.,
            WorkArea {
                in_frame: 79,
                out_frame: 80,
            },
        ),
    ] {
        let state = fixture();
        let project: Project = serde_json::from_value(state["project"].clone()).unwrap();
        let mut engine = Engine::new(project.clone()).unwrap();
        let mut p = Panels::default();
        let mut down = pointer([press, 7.], false);
        down.pressed = Some((MouseButton::Left, down.mouse));
        assert!(draw(&mut p, &state, &down).is_empty());
        assert!(draw(&mut p, &state, &pointer([point, 7.], false)).is_empty());
        assert_eq!(engine.project(), &project);
        let actions = draw(&mut p, &state, &pointer([point, 7.], true));
        assert!(matches!(
            actions.as_slice(),
            [Action::EditChecked {
                command: Command::SetWorkArea { .. },
                ..
            }]
        ));
        for action in actions {
            if let Action::EditChecked { command, revision } = action {
                assert_eq!(revision, engine.revision());
                engine.apply(command).unwrap();
            }
        }
        assert_eq!(engine.project().work_area, expected);
        assert!(p.timeline.work_area_drag.is_none());
        engine.undo().unwrap();
        assert_eq!(engine.project(), &project);
        assert!(draw(&mut p, &state, &pointer([point + 10., 7.], false)).is_empty());
    }
}
#[test]
fn cancellation_and_stale_snapshot_never_commit_a_delayed_release() {
    for reason in 0..6 {
        let mut state = fixture();
        let mut p = Panels::default();
        let mut down = pointer([40., 7.], false);
        down.pressed = Some((MouseButton::Left, down.mouse));
        draw(&mut p, &state, &down);
        draw(&mut p, &state, &pointer([60., 7.], false));
        let mut cancel = Input::default();
        match reason {
            0 => cancel.focus_lost = true,
            1 => cancel.keys.push((Key::Escape, Modifiers::default())),
            2 => state["root"] = "other".into(),
            3 => state["composition"] = "other".into(),
            4 => state["revision"] = 1.into(),
            _ => {
                assert!(super::super::cancel_interaction(&mut p).is_empty());
            }
        }
        assert!(draw(&mut p, &state, &cancel).is_empty());
        assert!(draw(&mut p, &state, &pointer([100., 7.], true)).is_empty());
        assert!(p.timeline.work_area_drag.is_none());
    }
}

#[test]
fn collapsed_time_lanes_keep_handles_inside_the_available_width_without_panicking() {
    for width in [1., 2., 4., 5., 12., 24., 240.] {
        let mut p = Panels::default();
        let mut out = vec![];
        let mut paint = PaintList::default();
        render(
            &mut paint,
            Rect::new(0., 0., width, 14.),
            &mut p,
            &fixture(),
            &Input::default(),
            width / 120.,
            0.,
            &mut out,
        );
        assert!(out.is_empty());
        let point = [width * 2. / 3., 7.];
        let down = Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        };
        assert!(render(
            &mut paint,
            Rect::new(0., 0., width, 14.),
            &mut p,
            &fixture(),
            &down,
            width / 120.,
            0.,
            &mut out
        ));
        assert!(out.is_empty());
    }
}
#[test]
fn range_shortcuts_work_without_layer_selection_and_respect_search_and_node_focus() {
    for locale in [Locale::En, Locale::Zh] {
        let state = fixture();
        let mut p = Panels::default();
        p.locale = locale;
        p.focused = Some(Panel::Timeline);
        for (key, shift, expected) in [
            (Key::B, false, 40),
            (Key::N, false, 41),
            (Key::Home, true, 20),
            (Key::End, true, 79),
        ] {
            let m = Modifiers {
                shift,
                ..Default::default()
            };
            let input = Input {
                keys: vec![(key, m)],
                ..Default::default()
            };
            let mut out = vec![];
            super::super::keyboard::route(&input, &mut p, &state, false, None, &mut out);
            match out.as_slice() {
                [Action::EditChecked {
                    command:
                        Command::SetWorkArea {
                            in_frame,
                            out_frame,
                        },
                    ..
                }] => assert_eq!(if key == Key::B { *in_frame } else { *out_frame }, expected),
                [Action::Seek(frame)] => assert_eq!(*frame, f64::from(expected)),
                _ => panic!("{out:?}"),
            }
        }
        p.timeline.timeline_search = true;
        let mut out = vec![];
        super::super::keyboard::route(
            &Input {
                keys: vec![(Key::B, Modifiers::default())],
                ..Default::default()
            },
            &mut p,
            &state,
            false,
            None,
            &mut out,
        );
        assert!(out.is_empty());
        p.timeline.timeline_search = false;
        p.focused = Some(Panel::Nodes);
        super::super::keyboard::route(
            &Input {
                keys: vec![(Key::N, Modifiers::default())],
                ..Default::default()
            },
            &mut p,
            &state,
            true,
            None,
            &mut out,
        );
        assert!(!out.iter().any(|action| matches!(
            action,
            Action::EditChecked {
                command: Command::SetWorkArea { .. },
                ..
            }
        )));
    }
}
