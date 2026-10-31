use super::*;
use crate::{
    engine::Engine, panels::viewer_guides::Controls as Guides, viewer_navigation::Navigation,
};

fn setup() -> (tempfile::TempDir, Engine, Value) {
    let (folder, engine, _, _) = super::super::tests::fixture(false);
    let other = motion_core::Layer::solid(
        8,
        "Reference",
        [100.; 2],
        [1125.25, 950., 0.],
        [0.4, 0.5, 0.6, 1.],
    );
    let state = engine
        .edit(serde_json::to_string(&Command::Add { layer: other }).unwrap())
        .unwrap();
    (folder, engine, state)
}

#[test]
fn move_and_scale_snap_to_other_layer_geometry_with_one_undo_and_alt_bypasses_targets() {
    let (_folder, engine, state) = setup();
    for zoom in [0.25, 1., 4.] {
        let view = Rect::new(30., 70., 1800., 1300.);
        let mut nav = Navigation::default();
        assert!(nav.zoom_at(view, [1920., 1080.], [930., 720.], zoom));
        let mapping = nav.mapping(view, [1920., 1080.]).unwrap();
        let pose = Pose::new(&state, 7, mapping.image).unwrap();
        let snap = Guides::default()
            .transform_snapping(
                &state,
                mapping,
                [1920., 1080.],
                Some(7),
                &PaintList::default().styles(),
            )
            .unwrap();
        for scale in [false, true] {
            let (handle, from, target) = if scale {
                (
                    Handle::Scale(true, true, Some(2)),
                    pose.corners[2],
                    mapping.screen([1175.25, 647.625]),
                )
            } else {
                (Handle::Move, pose.pivot, mapping.screen([1125.25, 950.]))
            };
            let point = if scale {
                [target[0] - 2., target[1] - 1.]
            } else {
                [target[0] - 2., target[1] + 3.]
            };
            let mut drag =
                Drag::new(Seed::new(from, &state, mapping.image), pose.clone(), handle).unwrap();
            let actions = drag.commands_with_guides(
                &state,
                &Input {
                    mouse: point,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                Some(&snap),
                &PaintList::default().styles(),
            );
            super::super::tests::apply(&engine, actions);
            let after = engine.state().unwrap();
            let actual = Pose::new(&after, 7, mapping.image).unwrap();
            let reached = if scale {
                actual.corners[2]
            } else {
                actual.pivot
            };
            assert!(
                distance(reached, target) < 0.08,
                "zoom={zoom} scale={scale} actual={reached:?} target={target:?}"
            );
            assert!(drag.snap_feedback.is_some());
            let mut paint = PaintList::default();
            drag.paint_snap_feedback(&mut paint, view);
            assert!(!paint.shapes.is_empty());
            engine.history(3).unwrap();
            assert_eq!(engine.history(0).unwrap()["project"], state["project"]);
            let mut drag =
                Drag::new(Seed::new(from, &state, mapping.image), pose.clone(), handle).unwrap();
            let actions = drag.commands_with_guides(
                &state,
                &Input {
                    mouse: point,
                    held: Some(MouseButton::Left),
                    modifiers: motion_ui::input::Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Some(&snap),
                &PaintList::default().styles(),
            );
            super::super::tests::apply(&engine, actions);
            assert!(drag.snap_feedback.is_none());
            let actual = Pose::new(&engine.state().unwrap(), 7, mapping.image).unwrap();
            let reached = if scale {
                actual.corners[2]
            } else {
                actual.pivot
            };
            assert!(distance(reached, target) > 0.2);
            engine.history(4).unwrap();
            assert_eq!(engine.state().unwrap()["project"], state["project"]);
        }
    }
}

#[test]
fn feedback_uses_window_tokens_and_only_reached_axes() {
    let (_folder, _engine, state) = setup();
    let pose = Pose::new(&state, 7, Rect::new(0., 0., 1920., 1080.)).unwrap();
    let mut drag = Drag::new(
        Seed::new(pose.pivot, &state, pose.projection.image),
        pose,
        Handle::Move,
    )
    .unwrap();
    drag.snap_feedback = Some(([300., 400.], [Some(300.), None]));
    let clip = Rect::new(20., 30., 800., 700.);
    let styles = PaintList::default().styles().with_overrides(br##"{"tokens":{"colors":{"viewer.guide":"#123456"},"sizes":{"viewer.overlay.line.width":2,"viewer.overlay.snap.marker.radius":5}}}"##).unwrap();
    let expected_color = styles.color("viewer.guide");
    let mut paint = PaintList::with_styles(std::sync::Arc::new(styles));
    drag.paint_snap_feedback(&mut paint, clip);
    assert!(paint.shapes.is_empty());
    drag.begun = true;
    drag.paint_snap_feedback(&mut paint, clip);
    assert_eq!(paint.shapes.len(), 2);
    assert!(paint.colors.iter().all(|color| *color == expected_color));
    assert!(
        matches!(&paint.shapes[0],paint::Shape::Stroke{points,width,..} if points == &vec![[300.,30.],[300.,730.]] && *width == 2.)
    );
    assert!(
        matches!(&paint.shapes[1],paint::Shape::Procedural{area,kind:paint::Pattern::Diamond,..} if area.width() == 10. && area.height() == 10.)
    );
}
