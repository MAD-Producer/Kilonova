//! Actual transform commands and host geometry, rather than a mirrored scale formula.
use super::tests::{apply, fixture};
use super::*;
use crate::panels::viewer_guides;
use crate::viewer_navigation::Navigation;
use motion_core::{Guide, GuideAxis};

fn pointer(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
fn guides(engine: &crate::engine::Engine, entries: Vec<Guide>) -> Value {
    engine
        .edit(serde_json::to_string(&Command::SetGuides { guides: entries }).unwrap())
        .unwrap()
}
fn mapping(zoom: f64) -> (Rect, crate::viewer_navigation::Mapping) {
    let view = Rect::new(30., 70., 1800., 1300.);
    let mut nav = Navigation::default();
    assert!(nav.zoom_at(view, [1920., 1080.], [930., 720.], zoom));
    assert!(nav.pan(view, [1920., 1080.], [13., -17.]));
    (view, nav.mapping(view, [1920., 1080.]).unwrap())
}
fn context(state: &Value, mapping: crate::viewer_navigation::Mapping) -> viewer_guides::Snapping {
    viewer_guides::Controls::default()
        .snapping(
            state,
            mapping,
            [1920., 1080.],
            &PaintList::default().styles(),
        )
        .unwrap()
}

#[test]
fn moving_anchor_snaps_through_the_actual_host_at_different_zoom_and_camera_modes_and_undoes_once()
{
    for three_d in [false, true] {
        let (_folder, engine, _, _) = fixture(three_d);
        for zoom in [0.25, 1., 4.] {
            let (viewport, mapping) = mapping(zoom);
            let state = guides(
                &engine,
                vec![
                    Guide {
                        id: 1,
                        axis: GuideAxis::X,
                        position: 1020.,
                    },
                    Guide {
                        id: 2,
                        axis: GuideAxis::Y,
                        position: 590.,
                    },
                ],
            );
            let before = state["project"].clone();
            let pose = Pose::new(&state, 7, mapping.image).unwrap();
            let start = pose.pivot;
            let target = mapping.screen([1020., 590.]);
            let end = [target[0] - 3., target[1] + 2.];
            let snap = context(&state, mapping);
            let mut controls = Controls::default();
            let mut out = vec![];
            controls.draw_with_guides(
                &mut PaintList::default(),
                viewport,
                mapping.image,
                &state,
                Some(7),
                &Input {
                    pressed: Some((MouseButton::Left, start)),
                    ..pointer(start)
                },
                Some(&snap),
                &mut out,
            );
            assert!(out.is_empty());
            controls.draw_with_guides(
                &mut PaintList::default(),
                viewport,
                mapping.image,
                &state,
                Some(7),
                &pointer(end),
                Some(&snap),
                &mut out,
            );
            apply(&engine, out);
            let current = engine.state().unwrap();
            let mut out = vec![];
            controls.draw_with_guides(
                &mut PaintList::default(),
                viewport,
                mapping.image,
                &current,
                Some(7),
                &Input {
                    mouse: end,
                    released: Some((MouseButton::Left, end)),
                    ..Default::default()
                },
                Some(&snap),
                &mut out,
            );
            apply(&engine, out);
            let after = engine.state().unwrap();
            let pose = Pose::new(&after, 7, mapping.image).unwrap();
            assert!(
                distance(pose.pivot, target) < 0.1,
                "three_d={three_d} zoom={zoom} actual={:?} target={target:?}",
                pose.pivot
            );
            assert_eq!(engine.history(0).unwrap()["project"], before);
        }
    }
}

#[test]
fn uniform_corner_scale_snaps_the_real_rotated_or_perspective_grip_and_preserves_ratio() {
    for three_d in [false, true] {
        let (_folder, engine, _, _) = fixture(three_d);
        engine
            .edit(
                serde_json::to_string(&Command::SetVector {
                    object: 7,
                    property: Property::Rotation,
                    frame: 0,
                    value: if three_d {
                        [12., -9., 25.]
                    } else {
                        [0., 0., 25.]
                    },
                })
                .unwrap(),
            )
            .unwrap();
        for zoom in [0.25, 1., 4.] {
            let (_, mapping) = mapping(zoom);
            let initial = engine.state().unwrap();
            let pose = Pose::new(&initial, 7, mapping.image).unwrap();
            let start = pose.corners[2];
            let probe = Drag::new(
                Seed::new(start, &initial, mapping.image),
                pose,
                Handle::Scale(true, true, Some(2)),
            )
            .unwrap();
            let candidate = if three_d {
                [160.; 3]
            } else {
                [160., 160., 100.]
            };
            let end = probe.scaled_grip(candidate).unwrap();
            let goal = end[0] + 3.;
            let position = mapping.local([goal, end[1]])[0];
            let state = guides(
                &engine,
                vec![Guide {
                    id: 1,
                    axis: GuideAxis::X,
                    position,
                }],
            );
            let before = state["project"].clone();
            let pose = Pose::new(&state, 7, mapping.image).unwrap();
            let mut drag = Drag::new(
                Seed::new(start, &state, mapping.image),
                pose,
                Handle::Scale(true, true, Some(2)),
            )
            .unwrap();
            apply(
                &engine,
                drag.commands_with_guides(&state, &pointer(end), Some(&context(&state, mapping)), &PaintList::default().styles()),
            );
            engine.history(3).unwrap();
            let after = engine.state().unwrap();
            let actual = Pose::new(&after, 7, mapping.image).unwrap();
            assert!(
                (actual.corners[2][0] - goal).abs() < 0.1,
                "three_d={three_d} zoom={zoom} corner={:?} goal={goal}",
                actual.corners[2]
            );
            assert!((actual.scale[0] - actual.scale[1]).abs() < 0.001);
            if three_d {
                assert!((actual.scale[0] - actual.scale[2]).abs() < 0.001);
            } else {
                assert_eq!(actual.scale[2], 100.);
            }
            assert_eq!(engine.history(0).unwrap()["project"], before);
        }
    }
}

#[test]
fn edge_snap_keeps_separated_uncontrolled_tracks_and_alt_bypasses_guide_targets() {
    let (_folder, engine, _, _) = fixture(false);
    engine
        .edit(
            serde_json::to_string(&Command::SeparateDimensions {
                object: 7,
                property: Property::Scale,
            })
            .unwrap(),
        )
        .unwrap();
    let (_, mapping) = mapping(1.);
    let state = guides(
        &engine,
        vec![Guide {
            id: 1,
            axis: GuideAxis::X,
            position: 1123.,
        }],
    );
    for alt in [false, true] {
        let before = engine.state().unwrap();
        let pose = Pose::new(&before, 7, mapping.image).unwrap();
        let start = mid(pose.corners[1], pose.corners[2]);
        let end = mapping.screen([1120., 540.]);
        let mut drag = Drag::new(
            Seed::new(start, &before, mapping.image),
            pose,
            Handle::Scale(true, false, Some(5)),
        )
        .unwrap();
        apply(
            &engine,
            drag.commands_with_guides(
                &before,
                &Input {
                    modifiers: motion_ui::input::Modifiers {
                        alt,
                        ..Default::default()
                    },
                    ..pointer(end)
                },
                Some(&context(&state, mapping)),
                &PaintList::default().styles(),
            ),
        );
        engine.history(3).unwrap();
        let after = engine.state().unwrap();
        let pose = Pose::new(&after, 7, mapping.image).unwrap();
        assert_eq!(pose.scale[1], 100.);
        assert_eq!(pose.scale[2], 100.);
        let edge = mid(pose.corners[1], pose.corners[2]);
        let expected = mapping.screen([if alt { 1120. } else { 1123. }, 540.]);
        assert!(distance(edge, expected) < 0.1);
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    }
}

#[test]
fn hidden_overlays_and_distant_guides_preserve_unsnapped_scale_and_unreachable_targets_are_normal()
{
    let (_folder, engine, _, _) = fixture(false);
    let (_, mapping) = mapping(1.);
    let state = guides(
        &engine,
        vec![Guide {
            id: 1,
            axis: GuideAxis::X,
            position: 1800.,
        }],
    );
    let mut controls = viewer_guides::Controls::default();
    controls.visibility.guides = false;
    assert!(controls
        .snapping(
            &state,
            mapping,
            [1920., 1080.],
            &PaintList::default().styles()
        )
        .is_none());
    let pose = Pose::new(&state, 7, mapping.image).unwrap();
    let start = pose.corners[2];
    let drag = Drag::new(
        Seed::new(start, &state, mapping.image),
        pose,
        Handle::Scale(true, true, Some(2)),
    )
    .unwrap();
    assert!(drag
        .snapped_scale(
            [150., 150., 100.],
            true,
            true,
            true,
            &context(&state, mapping)
        )
        .is_none());
    let pose = Pose::new(&state, 7, mapping.image).unwrap();
    let start = mid(pose.corners[0], pose.corners[1]);
    let mut drag = Drag::new(
        Seed::new(start, &state, mapping.image),
        pose,
        Handle::Scale(false, true, Some(4)),
    )
    .unwrap();
    drag.pose.scale[1] = 0.;
    assert!(drag
        .snapped_scale(
            [100., 150., 100.],
            false,
            false,
            true,
            &context(&state, mapping)
        )
        .is_none());
}
