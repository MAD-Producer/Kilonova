use super::*;
use std::sync::Arc;

fn styles(threshold: f32) -> Arc<motion_ui::style::StyleSheet> {
    Arc::new(
        PaintList::default()
            .styles()
            .with_overrides(
                &serde_json::to_vec(
                    &serde_json::json!({"tokens":{"sizes":{"motion.drag_threshold":threshold}}}),
                )
                .unwrap(),
            )
            .unwrap(),
    )
}

fn frame(
    controls: &mut Controls,
    state: &Value,
    view: Rect,
    styles: &Arc<motion_ui::style::StyleSheet>,
    input: Input,
) -> Vec<Action> {
    let mut paint = PaintList::default();
    paint.stylesheet = Some(styles.clone());
    let mut actions = Vec::new();
    controls.draw_in_viewport(&mut paint, view, view, state, Some(7), &input, &mut actions);
    actions
}

fn start(state: &Value, view: Rect, tool: Tool) -> [f32; 2] {
    let pose = Pose::new(state, 7, view).unwrap();
    match tool {
        Tool::Move | Tool::Anchor => pose.pivot,
        Tool::Rotate => pose.rotate,
        Tool::Scale => pose.corners[0],
    }
}

fn press(
    controls: &mut Controls,
    state: &Value,
    view: Rect,
    styles: &Arc<motion_ui::style::StyleSheet>,
    point: [f32; 2],
) {
    assert!(frame(
        controls,
        state,
        view,
        styles,
        Input {
            mouse: point,
            held: Some(MouseButton::Left),
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        }
    )
    .is_empty());
}

#[test]
fn window_threshold_keeps_short_gizmo_motion_and_release_out_of_history() {
    let styles = styles(12.);
    for three_d in [false, true] {
        let (_folder, engine, state, view) = super::tests::fixture(three_d);
        for (tool_index, tool) in [Tool::Move, Tool::Rotate, Tool::Scale, Tool::Anchor]
            .into_iter()
            .enumerate()
        {
            let mut controls = Controls::default();
            controls.tool = tool;
            let start = start(&state, view, tool);
            press(&mut controls, &state, view, &styles, start);
            let end = [start[0] + 5., start[1]];
            let actions = frame(
                &mut controls,
                &state,
                view,
                &styles,
                Input {
                    mouse: end,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            );
            assert!(
                actions.is_empty(),
                "three_d={three_d} tool={tool_index}: short motion must not begin history"
            );
            let actions = frame(
                &mut controls,
                &state,
                view,
                &styles,
                Input {
                    mouse: end,
                    released: Some((MouseButton::Left, end)),
                    ..Default::default()
                },
            );
            assert!(
                actions.is_empty(),
                "three_d={three_d} tool={tool_index}: a short release must not commit history"
            );
            assert!(controls.drag.is_none() && controls.pick.is_none());
            let after = engine.state().unwrap();
            assert_eq!(after["project"], state["project"]);
            assert_eq!(after["revision"], state["revision"]);
        }
    }
}

#[test]
fn small_window_threshold_starts_real_gizmo_edits_with_one_undo() {
    let styles = styles(1.);
    for three_d in [false, true] {
        let (_folder, engine, _, view) = super::tests::fixture(three_d);
        for (tool_index, tool) in [Tool::Move, Tool::Rotate, Tool::Scale, Tool::Anchor]
            .into_iter()
            .enumerate()
        {
            let before = engine.state().unwrap();
            let mut controls = Controls::default();
            controls.tool = tool;
            let start = start(&before, view, tool);
            press(&mut controls, &before, view, &styles, start);
            let end = [start[0] + 2., start[1]];
            let actions = frame(
                &mut controls,
                &before,
                view,
                &styles,
                Input {
                    mouse: end,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::History(2)))
                    .count(),
                1,
                "three_d={three_d} tool={tool_index}"
            );
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a, Action::PreviewDrag { .. } | Action::EditBatch(_))),
                "three_d={three_d} tool={tool_index}"
            );
            super::tests::apply(&engine, actions);
            let after = engine.state().unwrap();
            assert_ne!(after["project"], before["project"]);
            let released = frame(
                &mut controls,
                &after,
                view,
                &styles,
                Input {
                    mouse: end,
                    released: Some((MouseButton::Left, end)),
                    ..Default::default()
                },
            );
            assert_eq!(
                released
                    .iter()
                    .filter(|a| matches!(a, Action::History(3)))
                    .count(),
                1
            );
            assert!(!released.iter().any(|a| matches!(a, Action::History(2))));
            super::tests::apply(&engine, released);
            assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        }
    }
}

#[test]
fn default_boundary_pointer_travel_and_started_gestures_keep_their_contract() {
    for (threshold, travel) in [(3., false), (12., false), (12., true)] {
        let (_folder, engine, before, view) = super::tests::fixture(false);
        let styles = styles(threshold);
        let mut controls = Controls::default();
        controls.tool = Tool::Scale;
        let start = start(&before, view, Tool::Scale);
        press(&mut controls, &before, view, &styles, start);
        let displacement = if travel { 1. } else { threshold };
        let point = [start[0] + displacement, start[1]];
        let actions = frame(
            &mut controls,
            &before,
            view,
            &styles,
            Input {
                mouse: point,
                held: Some(MouseButton::Left),
                pointer_travel: threshold,
                ..Default::default()
            },
        );
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::History(2)))
                .count(),
            1
        );
        super::tests::apply(&engine, actions);
        let state = engine.state().unwrap();
        let nearer = [start[0] + 0.5, start[1]];
        let actions = frame(
            &mut controls,
            &state,
            view,
            &styles,
            Input {
                mouse: nearer,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::History(2))));
        assert!(actions.iter().any(|a| matches!(a, Action::EditBatch(_))));
        super::tests::apply(&engine, actions);
        let state = engine.state().unwrap();
        let actions = frame(
            &mut controls,
            &state,
            view,
            &styles,
            Input {
                mouse: nearer,
                released: Some((MouseButton::Left, nearer)),
                ..Default::default()
            },
        );
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::History(3)))
                .count(),
            1
        );
        super::tests::apply(&engine, actions);
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    }
}
