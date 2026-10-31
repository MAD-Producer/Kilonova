use super::*;
use glam::{Mat4, Quat, Vec3};

fn pose(eye: Vec3, parent: Mat4, rotation: [f32; 3], scale: [f32; 3], anchor: [f32; 2]) -> Pose {
    let size = [200., 100.];
    let vp = Mat4::perspective_rh(60f32.to_radians(), 1920. / 1080., 1., 10000.)
        * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
    let rs = Mat4::from_scale_rotation_translation(
        Vec3::from_array(scale) / 100.,
        motion_core::rotation_quaternion(rotation, motion_core::RotationOrder::LocalXyz),
        Vec3::ZERO,
    );
    let offset = Mat4::from_translation(Vec3::new(
        (0.5 - anchor[0]) * size[0],
        (anchor[1] - 0.5) * size[1],
        0.,
    ));
    let projection = Projection::new(
        (vp * parent * rs * offset).to_cols_array().map(f64::from),
        Rect::new(0., 0., 1920., 1080.),
        [1920., 1080.],
    )
    .unwrap();
    let pivot = projection
        .screen([(anchor[0] - 0.5) * size[0], (anchor[1] - 0.5) * size[1]])
        .unwrap();
    Pose {
        object: 7,
        projection,
        size,
        anchor,
        scale,
        position: [960., 540., 0.],
        rotation,
        rotation_order: motion_core::RotationOrder::LocalXyz,
        three_d: true,
        locked: false,
        corners: [[0.; 2]; 4],
        pivot,
        rotate: [0.; 2],
    }
}
fn styles() -> std::sync::Arc<StyleSheet> {
    PaintList::default().styles()
}

#[test]
fn ray_constraints_preserve_parent_axes_and_layer_scale_axes_across_cameras_mirrors_and_anchors() {
    for eye in [
        Vec3::new(0., 0., 1800.),
        Vec3::new(900., 600., 1500.),
        Vec3::new(-1100., 400., 1500.),
    ] {
        for parent in [
            Mat4::IDENTITY,
            Mat4::from_scale_rotation_translation(
                Vec3::new(-1.4, 0.7, 1.8),
                Quat::from_rotation_y(0.4) * Quat::from_rotation_z(0.3),
                Vec3::new(40., -50., 10.),
            ),
        ] {
            let pose = pose(
                eye,
                parent,
                [20., -30., 18.],
                [-140., 70., 125.],
                [0.2, 0.7],
            );
            for tool in [Tool::Move, Tool::Scale] {
                let frame = Frame::new(&pose, tool, &styles()).unwrap();
                assert!(distance(frame.origin, pose.pivot) < 0.01);
                for i in 0..3 {
                    let Some(length) = frame.lengths[i] else {
                        continue;
                    };
                    let a = axis(i) * length * 0.7;
                    let b = a + axis(i) * 12.;
                    let from = frame
                        .point(frame.projection.screen3(a).unwrap(), Constraint::Axis(i))
                        .unwrap();
                    let to = frame
                        .point(frame.projection.screen3(b).unwrap(), Constraint::Axis(i))
                        .unwrap();
                    assert!(
                        (to - from - axis(i) * 12.).length() < 0.03,
                        "{eye:?} {tool:?} axis={i}"
                    );
                    let tip = frame.tips[i].unwrap();
                    assert!(
                        matches!(frame.hit(tip,&styles()),Some((_,Constraint::Axis(index))) if index==i)
                    );
                }
                for normal in 0..3 {
                    if frame.planes[normal].is_none() {
                        continue;
                    }
                    let a = (normal + 1) % 3;
                    let b = (normal + 2) % 3;
                    let start = axis(a) * frame.lengths[a].unwrap() * 0.3
                        + axis(b) * frame.lengths[b].unwrap() * 0.3;
                    let delta = axis(a) * 7. + axis(b) * (-11.);
                    let from = frame
                        .point(
                            frame.projection.screen3(start).unwrap(),
                            Constraint::Plane(normal),
                        )
                        .unwrap();
                    let to = frame
                        .point(
                            frame.projection.screen3(start + delta).unwrap(),
                            Constraint::Plane(normal),
                        )
                        .unwrap();
                    assert!(
                        (to - from - delta).length() < 0.03,
                        "{eye:?} {tool:?} plane={normal}"
                    );
                    let corners = frame.planes[normal].unwrap();
                    let center =
                        std::array::from_fn(|k| corners.iter().map(|p| p[k]).sum::<f32>() / 4.);
                    assert!(
                        matches!(frame.hit(center,&styles()),Some((_,Constraint::Plane(i))) if i==normal)
                    );
                }
            }
            // Position space is evaluated parent space, not the layer's rotated
            // or scaled local space. Compare it with an independent projection.
            let frame = Frame::new(&pose, Tool::Move, &styles()).unwrap();
            let vp = Mat4::perspective_rh(60f32.to_radians(), 1920. / 1080., 1., 10000.)
                * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
            let expected = Projection::new(
                (vp * parent).to_cols_array().map(f64::from),
                Rect::new(0., 0., 1920., 1080.),
                [1920., 1080.],
            )
            .unwrap();
            for delta in [DVec3::X * 23., DVec3::Y * (-17.), DVec3::Z * 31.] {
                assert!(
                    distance(
                        frame.projection.screen3(delta).unwrap(),
                        expected.screen3(delta).unwrap()
                    ) < 0.01
                );
            }
        }
    }
}

#[test]
fn end_on_axes_and_edge_on_planes_are_disabled_and_projection_keeps_2d_mapping_identical() {
    let pose = pose(
        Vec3::new(0., 0., 1800.),
        Mat4::IDENTITY,
        [0.; 3],
        [100.; 3],
        [0.5; 2],
    );
    let frame = Frame::new(&pose, Tool::Move, &styles()).unwrap();
    assert!(frame.lengths[2].is_none());
    assert!(frame.planes[0].is_none() && frame.planes[1].is_none());
    assert!(frame.planes[2].is_some());
    assert!(matches!(
        frame.point(frame.origin, Constraint::Axis(2)),
        Err(Problem::Direction)
    ));
    assert!(matches!(
        frame.point(frame.origin, Constraint::Plane(0)),
        Err(Problem::Direction)
    ));
    for point in [[0., 0.], [-20., 37.], [58., -42.]] {
        assert_eq!(
            pose.projection.screen(point),
            pose.projection
                .screen3(DVec3::new(f64::from(point[0]), f64::from(point[1]), 0.))
        );
    }
    assert!(pose.projection.ray3([f32::NAN, 0.]).is_none());
    let mut flat = pose.clone();
    flat.three_d = false;
    assert!(Frame::new(&flat, Tool::Move, &styles()).is_none());
    assert!(Frame::new(&pose, Tool::Rotate, &styles()).is_none());
    assert!(Frame::new(&pose, Tool::Anchor, &styles()).is_none());
}

#[test]
fn constrained_commands_only_change_requested_components_and_share_one_preview_transaction() {
    let pose = pose(
        Vec3::new(900., 600., 1500.),
        Mat4::IDENTITY,
        [20., -30., 18.],
        [140., 70., 125.],
        [0.2, 0.7],
    );
    for tool in [Tool::Move, Tool::Scale] {
        for constraint in [
            Constraint::Axis(0),
            Constraint::Axis(1),
            Constraint::Axis(2),
            Constraint::Plane(0),
            Constraint::Plane(1),
            Constraint::Plane(2),
        ] {
            let frame = Frame::new(&pose, tool, &styles()).unwrap();
            let start = match constraint {
                Constraint::Axis(i) => axis(i) * frame.lengths[i].unwrap() * 0.7,
                Constraint::Plane(i) => {
                    axis((i + 1) % 3) * frame.lengths[(i + 1) % 3].unwrap() * 0.3
                        + axis((i + 2) % 3) * frame.lengths[(i + 2) % 3].unwrap() * 0.3
                }
            };
            let delta = match constraint {
                Constraint::Axis(i) => axis(i) * frame.lengths[i].unwrap() * 0.2,
                Constraint::Plane(i) => {
                    axis((i + 1) % 3) * frame.lengths[(i + 1) % 3].unwrap() * 0.2
                        + axis((i + 2) % 3) * frame.lengths[(i + 2) % 3].unwrap() * (-0.15)
                }
            };
            let point = frame.projection.screen3(start).unwrap();
            let target = frame.projection.screen3(start + delta).unwrap();
            let state = serde_json::json!({"root":"a","document_generation":4,"composition":"comp-main","frame":0.});
            let mut drag = Drag::new(
                Seed::new(point, &state, Rect::new(0., 0., 1920., 1080.)),
                pose.clone(),
                Handle::Spatial(constraint, tool == Tool::Scale),
            )
            .unwrap();
            let actions = drag.commands(
                &state,
                &Input {
                    mouse: target,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            );
            let [Action::History(2), Action::EditBatch(commands)] = actions.as_slice() else {
                panic!("one begin plus one edit batch: {actions:?}");
            };
            let [Command::SetVector {
                object: 7,
                property,
                value,
                ..
            }] = commands.as_slice()
            else {
                panic!("one vector edit");
            };
            assert_eq!(
                *property,
                if tool == Tool::Move {
                    Property::Position
                } else {
                    Property::Scale
                }
            );
            let original = if tool == Tool::Move {
                pose.position
            } else {
                pose.scale
            };
            for i in 0..3 {
                let active = match constraint {
                    Constraint::Axis(a) => i == a,
                    Constraint::Plane(normal) => i != normal,
                };
                if !active {
                    assert_eq!(value[i], original[i]);
                } else if tool == Tool::Move {
                    assert!((value[i] - original[i] - delta[i] as f32).abs() < 0.03);
                } else {
                    assert!(
                        (value[i]
                            - original[i]
                                * (1. + delta[i] as f32 / frame.lengths[i].unwrap() as f32))
                            .abs()
                            < 0.03
                    );
                }
            }
            assert!(drag
                .commands(
                    &state,
                    &Input {
                        mouse: target,
                        released: Some((MouseButton::Left, target)),
                        ..Default::default()
                    }
                )
                .is_empty());
        }
    }
}
