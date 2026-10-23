use super::*;
use crate::engine::Engine;
use motion_ui::input::Modifiers;

fn check_drag(
    engine: &Engine,
    state: &Value,
    view: Rect,
    handle: Handle,
    modifiers: Modifiers,
    expected: [f32; 3],
) {
    let pose = Pose::new(state, 7, view).unwrap();
    let (start, local_end) = match handle {
        Handle::Scale(true, true, Some(i)) => (
            pose.corners[i],
            [[-150., -100.], [150., -100.], [150., 100.], [-150., 100.]][i],
        ),
        Handle::Scale(true, false, Some(5)) => (mid(pose.corners[1], pose.corners[2]), [150., 0.]),
        Handle::Scale(true, false, Some(7)) => (mid(pose.corners[3], pose.corners[0]), [-150., 0.]),
        Handle::Scale(false, true, Some(6)) => (mid(pose.corners[2], pose.corners[3]), [0., 100.]),
        Handle::Scale(false, true, Some(4)) => (mid(pose.corners[0], pose.corners[1]), [0., -100.]),
        _ => panic!("scale fixture requires a corner or edge"),
    };
    let end = pose.projection.screen(local_end).unwrap();
    let mut drag = Drag::new(Seed::new(start, state, view), pose, handle).unwrap();
    super::tests::apply(
        engine,
        drag.commands(
            state,
            &Input {
                mouse: end,
                held: Some(MouseButton::Left),
                modifiers,
                ..Default::default()
            },
        ),
    );
    let after = engine.state().unwrap();
    let layer = after["sampledLayers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["id"] == 7)
        .unwrap();
    let actual = array::<3>(&layer["scale"]).unwrap();
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, b)| (*a - b).abs() < 0.01),
        "handle={handle:?} modifiers={modifiers:?} actual={actual:?} expected={expected:?}"
    );
    engine.history(3).unwrap();
    assert_eq!(engine.history(0).unwrap()["project"], state["project"]);
}

#[test]
fn ctrl_corner_scaling_keeps_the_default_proportion_constraint() {
    let (_folder, engine, state, view) = super::tests::fixture(false);
    check_drag(
        &engine,
        &state,
        view,
        Handle::Scale(true, true, Some(2)),
        Modifiers {
            control: true,
            ..Default::default()
        },
        [160., 160., 100.],
    );
}

#[test]
fn alt_corner_scaling_changes_axes_independently() {
    let (_folder, engine, state, view) = super::tests::fixture(false);
    check_drag(
        &engine,
        &state,
        view,
        Handle::Scale(true, true, Some(2)),
        Modifiers {
            alt: true,
            ..Default::default()
        },
        [150., 200., 100.],
    );
}

#[test]
fn shift_edge_scaling_preserves_the_other_axis_proportion() {
    let (_folder, engine, state, view) = super::tests::fixture(false);
    check_drag(
        &engine,
        &state,
        view,
        Handle::Scale(true, false, Some(5)),
        Modifiers {
            shift: true,
            ..Default::default()
        },
        [150., 150., 100.],
    );
}

#[test]
fn all_scale_grips_follow_the_modifier_table_in_2d_and_3d_with_one_undo() {
    const DEFAULT: [[f32; 3]; 3] = [[160., 160., 160.], [150., 100., 100.], [100., 200., 100.]];
    const SHIFT: [[f32; 3]; 3] = [[160., 160., 160.], [150., 150., 150.], [200., 200., 200.]];
    const ALT: [[f32; 3]; 3] = [[150., 200., 100.], [150., 100., 100.], [100., 200., 100.]];
    for three_d in [false, true] {
        let (_folder, engine, state, view) = super::tests::fixture(three_d);
        for (mask, expected) in [
            (0, DEFAULT),
            (1, DEFAULT),
            (2, SHIFT),
            (3, SHIFT),
            (4, ALT),
            (5, ALT),
            (6, ALT),
            (7, ALT),
        ] {
            let modifiers = Modifiers {
                control: mask & 1 != 0,
                shift: mask & 2 != 0,
                alt: mask & 4 != 0,
            };
            for (handle, expected_index) in [
                (Handle::Scale(true, true, Some(0)), 0),
                (Handle::Scale(true, true, Some(1)), 0),
                (Handle::Scale(true, true, Some(2)), 0),
                (Handle::Scale(true, true, Some(3)), 0),
                (Handle::Scale(true, false, Some(5)), 1),
                (Handle::Scale(true, false, Some(7)), 1),
                (Handle::Scale(false, true, Some(4)), 2),
                (Handle::Scale(false, true, Some(6)), 2),
            ] {
                let mut wanted = expected[expected_index];
                if !three_d {
                    wanted[2] = 100.;
                }
                check_drag(&engine, &state, view, handle, modifiers, wanted);
            }
        }
    }
}

fn input(point: [f32; 2], modifiers: Modifiers) -> Input {
    Input {
        mouse: point,
        held: Some(MouseButton::Left),
        modifiers,
        ..Default::default()
    }
}

#[test]
fn modifier_changes_use_the_original_seed_and_stay_in_one_gesture() {
    let (_folder, engine, state, view) = super::tests::fixture(false);
    let pose = Pose::new(&state, 7, view).unwrap();
    let start = pose.corners[2];
    let end = pose.projection.screen([150., 100.]).unwrap();
    let mut controls = Controls::default();
    let mut actions = Vec::new();
    let mut press = input(start, Modifiers::default());
    press.pressed = Some((MouseButton::Left, start));
    controls.draw_in_viewport(
        &mut PaintList::default(),
        view,
        view,
        &state,
        Some(7),
        &press,
        &mut actions,
    );
    assert!(actions.is_empty());
    for (modifiers, expected) in [
        (
            Modifiers {
                alt: true,
                ..Default::default()
            },
            [150., 200., 100.],
        ),
        (Modifiers::default(), [160., 160., 100.]),
        (
            Modifiers {
                alt: true,
                shift: true,
                ..Default::default()
            },
            [150., 200., 100.],
        ),
    ] {
        let current = engine.state().unwrap();
        controls.draw_in_viewport(
            &mut PaintList::default(),
            view,
            view,
            &current,
            Some(7),
            &input(end, modifiers),
            &mut actions,
        );
        super::tests::apply(&engine, std::mem::take(&mut actions));
        let current = engine.state().unwrap();
        let actual = array::<3>(&current["sampledLayers"][0]["scale"]).unwrap();
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(a, b)| (*a - b).abs() < 0.01),
            "{actual:?} != {expected:?}"
        );
    }
    let current = engine.state().unwrap();
    controls.draw_in_viewport(
        &mut PaintList::default(),
        view,
        view,
        &current,
        Some(7),
        &Input {
            mouse: end,
            released: Some((MouseButton::Left, end)),
            modifiers: Modifiers {
                alt: true,
                ..Default::default()
            },
            ..Default::default()
        },
        &mut actions,
    );
    super::tests::apply(&engine, actions);
    assert!(controls.drag.is_none());
    assert_eq!(engine.history(0).unwrap()["project"], state["project"]);
}

#[test]
fn escape_restores_the_original_project_after_a_free_scale_preview() {
    let (_folder, engine, state, view) = super::tests::fixture(false);
    let pose = Pose::new(&state, 7, view).unwrap();
    let start = pose.corners[2];
    let end = pose.projection.screen([150., 100.]).unwrap();
    let modifiers = Modifiers {
        alt: true,
        ..Default::default()
    };
    let mut controls = Controls::default();
    let mut actions = Vec::new();
    let mut press = input(start, modifiers);
    press.pressed = Some((MouseButton::Left, start));
    controls.draw_in_viewport(
        &mut PaintList::default(),
        view,
        view,
        &state,
        Some(7),
        &press,
        &mut actions,
    );
    assert!(actions.is_empty());
    controls.draw_in_viewport(
        &mut PaintList::default(),
        view,
        view,
        &state,
        Some(7),
        &input(end, modifiers),
        &mut actions,
    );
    super::tests::apply(&engine, std::mem::take(&mut actions));
    let preview = engine.state().unwrap();
    assert_ne!(preview["project"], state["project"]);
    controls.draw_in_viewport(
        &mut PaintList::default(),
        view,
        view,
        &preview,
        Some(7),
        &Input {
            keys: vec![(Key::Escape, Modifiers::default())],
            ..Default::default()
        },
        &mut actions,
    );
    assert!(matches!(actions.as_slice(), [Action::History(4)]));
    super::tests::apply(&engine, actions);
    assert_eq!(engine.state().unwrap()["project"], state["project"]);
    assert!(controls.drag.is_none());
}
