use super::tests::{apply, draw, engine, press};
use super::*;

#[test]
fn mask_path_commands_preserve_parameter_updates_between_snapshot_and_apply() {
    let mut engine = engine();
    let mask = &engine.project().layers[0].masks[0];
    let action = mask_path::command(
        7,
        mask,
        12,
        MaskAction::Animate {
            mask: 1,
            property: MaskProperty::Expansion,
            frame: 21,
            enabled: true,
        },
    )
    .unwrap();
    // A parameter update acknowledged after this path UI snapshot must survive.
    engine
        .apply(Command::Mask {
            object: 7,
            action: MaskAction::Set {
                mask: 1,
                property: MaskProperty::Opacity,
                frame: 21,
                value: vec![80.],
                animated: Some(true),
            },
        })
        .unwrap();
    let before_path = engine.project().clone();
    apply(&mut engine, vec![action]);
    assert_eq!(
        engine.project().layers[0].masks[0].opacity,
        before_path.layers[0].masks[0].opacity
    );
    assert!(animated(
        &engine.project().layers[0].masks[0],
        mask_path::CHANNEL
    ));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before_path);
}

#[test]
fn canvas_anchor_and_handle_drags_record_full_path_poses_and_leave_static_masks_static() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for animated in [false, true] {
        let mut engine = engine();
        engine
            .apply(Command::Mask {
                object: 7,
                action: MaskAction::Set {
                    mask: 1,
                    property: MaskProperty::Node(1),
                    frame: 21,
                    value: vec![0., 0., 0., 0., 10., 5.],
                    animated: None,
                },
            })
            .unwrap();
        if animated {
            let mask = &engine.project().layers[0].masks[0];
            let action = mask_path::command(
                7,
                mask,
                12,
                MaskAction::Animate {
                    mask: 1,
                    property: MaskProperty::Expansion,
                    frame: 21,
                    enabled: true,
                },
            )
            .unwrap();
            apply(&mut engine, vec![action]);
        }
        let before = engine.project().clone();
        let mut controls = super::super::Controls::default();
        controls.tool = Tool::Mask;
        controls.mask = Some(1);
        let view = Rect::new(0., 0., 640., 360.);
        let matrix =
            glam::DMat4::from_scale(glam::DVec3::new(2. / 640., 2. / 360., 1.)).to_cols_array();
        let snapshot = |engine: &motion_core::Engine| {
            let mask = &engine.project().layers[0].masks[0];
            let nodes: Vec<_> = mask
                .path
                .nodes
                .iter()
                .map(|n| serde_json::json!({"id":n.id,"geometry":n.geometry.sample(48.)}))
                .collect();
            serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"project":engine.project(),"frame":60,"revision":engine.revision(),
                "source_transforms":[{"id":7,"mvp":matrix}],"mask_layers":[{"id":7,"masks":[{"id":1,"path":{"id":1,"closed":true,"nodes":nodes}}]}]})
        };
        let mut render =
            |controls: &mut super::super::Controls, engine: &motion_core::Engine, input: &Input| {
                let mut actions = vec![];
                controls.draw(
                    &mut PaintList::default(),
                    &mut atlas,
                    view,
                    &snapshot(engine),
                    Some(7),
                    input,
                    Locale::En,
                    &mut actions,
                );
                actions
            };
        // Source coordinates (0,0) project to (270,130), including source-center offset.
        let actions = render(
            &mut controls,
            &engine,
            &press(MouseButton::Left, 270., 130.),
        );
        apply(&mut engine, actions);
        assert!(controls.drag.is_some());
        for (point, release) in [([300., 150.], false), ([310., 160.], true)] {
            let actions = render(
                &mut controls,
                &engine,
                &Input {
                    mouse: point,
                    held: (!release).then_some(MouseButton::Left),
                    released: release.then_some((MouseButton::Left, point)),
                    ..Default::default()
                },
            );
            apply(&mut engine, actions);
        }
        let mask = &engine.project().layers[0].masks[0];
        let geometry = mask.path.nodes[0].geometry.sample(48.);
        assert!((geometry[0] - 40.).abs() < 0.001 && (geometry[1] - 30.).abs() < 0.001);
        if animated {
            assert!(mask.path.nodes.iter().all(|n| n
                .geometry
                .keys
                .iter()
                .map(|k| k.frame)
                .collect::<Vec<_>>()
                == [9, 48]));
        } else {
            assert!(mask.path.nodes.iter().all(|n| n.geometry.keys.is_empty()));
        }
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        engine.redo().unwrap();
        // A selected vertex exposes its existing outgoing handle.
        let before_handle = engine.project().clone();
        let actions = render(
            &mut controls,
            &engine,
            &press(MouseButton::Left, 320., 165.),
        );
        apply(&mut engine, actions);
        let actions = render(
            &mut controls,
            &engine,
            &Input {
                mouse: [330., 170.],
                released: Some((MouseButton::Left, [330., 170.])),
                ..Default::default()
            },
        );
        apply(&mut engine, actions);
        let g = engine.project().layers[0].masks[0].path.nodes[0]
            .geometry
            .sample(48.);
        assert!((g[4] - 20.).abs() < 0.001 && (g[5] - 10.).abs() < 0.001);
        assert!((g[2] + 20.).abs() < 0.001 && (g[3] + 10.).abs() < 0.001);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before_handle);
    }
}

#[test]
fn control_drag_copies_path_pose_once_then_moves_only_the_copy() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut engine = engine();
    let mut controls = Controls::default();
    let actions = draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &press(MouseButton::Left, 24., 12.),
        &mut atlas,
    );
    apply(&mut engine, actions);
    let before = engine.project().clone();
    let x = |f| 80. + f as f32 / 119. * 304.;
    let mut input = press(MouseButton::Left, x(21), 35.);
    input.modifiers.control = true;
    let actions = draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &input,
        &mut atlas,
    );
    apply(&mut engine, actions);
    for (f, release) in [(40, false), (50, true)] {
        let point = [x(f), 35.];
        let actions = draw(
            &mut controls,
            &engine,
            21,
            mask_path::CHANNEL,
            &Input {
                mouse: point,
                held: (!release).then_some(MouseButton::Left),
                released: release.then_some((MouseButton::Left, point)),
                ..Default::default()
            },
            &mut atlas,
        );
        apply(&mut engine, actions);
    }
    assert!(engine.project().layers[0].masks[0]
        .path
        .nodes
        .iter()
        .all(|n| n.geometry.keys.iter().map(|k| k.frame).collect::<Vec<_>>() == [9, 38]));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn sparse_legacy_node_keys_are_sorted_and_edited_without_losing_other_tracks() {
    let mut engine = engine();
    let mut mask = engine.project().layers[0].masks[0].clone();
    mask.path.nodes[0].geometry.set_animated(-10, true).unwrap();
    mask.path.nodes[0]
        .geometry
        .set_at(15, [20., 0., 0., 0., 0., 0.])
        .unwrap();
    mask.path.nodes[1].geometry.set_animated(5, true).unwrap();
    mask.path.nodes[1]
        .geometry
        .set_at(15, [80., 0., 0., 0., 0., 0.])
        .unwrap();
    engine
        .apply(Command::Mask {
            object: 7,
            action: MaskAction::Replace { mask: mask.clone() },
        })
        .unwrap();
    assert_eq!(mask_path::frames(&mask), [-10, 5, 15]);
    // Composition 17 is local 5; only the node with that key should move.
    let action = mask_path::command(
        7,
        &mask,
        12,
        MaskAction::MoveKey {
            mask: 1,
            property: MaskProperty::Expansion,
            from: 17,
            to: 20,
        },
    )
    .unwrap();
    apply(&mut engine, vec![action]);
    let changed = &engine.project().layers[0].masks[0];
    assert_eq!(changed.path.nodes[0], mask.path.nodes[0]);
    assert_eq!(changed.path.nodes[1].geometry.keys[0].frame, 8);
    assert_eq!(changed.path.nodes[2], mask.path.nodes[2]);
    assert_eq!(changed.opacity, mask.opacity);
    engine.undo().unwrap();
    assert_eq!(engine.project().layers[0].masks[0], mask);
    let invalid = mask_path::edit(
        &mask,
        12,
        MaskAction::Set {
            mask: 1,
            property: MaskProperty::Expansion,
            frame: 21,
            value: vec![f32::NAN; 18],
            animated: Some(true),
        },
    );
    assert!(invalid.is_err());
    assert_eq!(engine.project().layers[0].masks[0], mask);
}
#[test]
fn path_stopwatch_viewport_pose_and_key_gestures_use_one_undo_and_local_time() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut engine = engine();
    let mut controls = Controls::default();
    let initial = engine.project().clone();
    let actions = draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &press(MouseButton::Left, 24., 12.),
        &mut atlas,
    );
    assert_eq!(actions.len(), 1);
    apply(&mut engine, actions);
    assert!(engine.project().layers[0].masks[0]
        .path
        .nodes
        .iter()
        .all(|n| n.geometry.keys[0].frame == 9));
    let enabled = engine.project().clone();
    // The same command used by canvas anchor and handle gestures stores a full pose.
    let mask = &engine.project().layers[0].masks[0];
    let action = mask_path::node_command(7, mask, 12, 1, 60, [30., 40., -5., 0., 5., 0.]).unwrap();
    apply(
        &mut engine,
        vec![Action::History(2), action, Action::History(3)],
    );
    let mask = &engine.project().layers[0].masks[0];
    assert!(mask.path.nodes.iter().all(|n| n
        .geometry
        .keys
        .iter()
        .map(|k| k.frame)
        .collect::<Vec<_>>()
        == [9, 48]));
    assert_eq!(
        mask.path.nodes[1].geometry.sample(48.),
        [100., 0., 0., 0., 0., 0.]
    );
    assert!((mask.path.nodes[0].geometry.sample(28.5)[0] - 15.).abs() < 0.001);
    let posed = engine.project().clone();
    // Context-menu easing applies to all vertices, including unchanged vertices.
    let x = |frame: u32| 80. + frame as f32 / 119. * 304.;
    draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &press(MouseButton::Right, x(21), 35.),
        &mut atlas,
    );
    for _ in 0..5 {
        draw(
            &mut controls,
            &engine,
            21,
            mask_path::CHANNEL,
            &Input {
                keys: vec![(Key::Down, Default::default())],
                ..Default::default()
            },
            &mut atlas,
        );
    }
    let actions = draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
        &mut atlas,
    );
    apply(&mut engine, actions);
    assert!(engine.project().layers[0].masks[0]
        .path
        .nodes
        .iter()
        .all(|n| n.geometry.keys[0].ease == Ease::InOut));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &posed);
    // Release position is applied, and moving all nodes is one gesture.
    let actions = draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &press(MouseButton::Left, x(60), 35.),
        &mut atlas,
    );
    apply(&mut engine, actions);
    for (frame, release) in [(70, false), (75, true)] {
        let point = [x(frame), 35.];
        let actions = draw(
            &mut controls,
            &engine,
            21,
            mask_path::CHANNEL,
            &Input {
                mouse: point,
                held: (!release).then_some(MouseButton::Left),
                released: release.then_some((MouseButton::Left, point)),
                ..Default::default()
            },
            &mut atlas,
        );
        apply(&mut engine, actions);
    }
    assert!(engine.project().layers[0].masks[0]
        .path
        .nodes
        .iter()
        .all(|n| n.geometry.keys[1].frame == 63));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &posed);
    engine.redo().unwrap();
    let moved = engine.project().clone();
    // Cancelling another move restores every node, with no additional commit.
    let actions = draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &press(MouseButton::Left, x(75), 35.),
        &mut atlas,
    );
    apply(&mut engine, actions);
    let actions = draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &Input {
            mouse: [x(80), 35.],
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        &mut atlas,
    );
    apply(&mut engine, actions);
    let actions = draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        },
        &mut atlas,
    );
    apply(&mut engine, actions);
    assert_eq!(engine.project(), &moved);
    // Undo the move, pose and animation toggle individually.
    engine.undo().unwrap();
    assert_eq!(engine.project(), &posed);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &enabled);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &initial);
}

#[test]
fn path_key_menu_add_delete_collision_and_disabling_preserve_sampled_shape() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut engine = engine();
    let mut controls = Controls::default();
    let x = |frame: u32| 80. + frame as f32 / 119. * 304.;
    for frame in [21, 60] {
        draw(
            &mut controls,
            &engine,
            frame,
            mask_path::CHANNEL,
            &press(MouseButton::Right, x(frame), 35.),
            &mut atlas,
        );
        let actions = draw(
            &mut controls,
            &engine,
            frame,
            mask_path::CHANNEL,
            &Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
            &mut atlas,
        );
        apply(&mut engine, actions);
    }
    let before = engine.project().clone();
    let mask = &before.layers[0].masks[0];
    assert!(mask_path::edit(
        mask,
        12,
        MaskAction::MoveKey {
            mask: 1,
            property: MaskProperty::Expansion,
            from: 21,
            to: 60
        }
    )
    .is_err());
    // Deleting one path key removes the entire pose, then undo restores it.
    draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &press(MouseButton::Right, x(60), 35.),
        &mut atlas,
    );
    let actions = draw(
        &mut controls,
        &engine,
        21,
        mask_path::CHANNEL,
        &Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
        &mut atlas,
    );
    assert_eq!(actions.len(), 1);
    apply(&mut engine, actions);
    assert!(engine.project().layers[0].masks[0]
        .path
        .nodes
        .iter()
        .all(|n| n.geometry.keys.len() == 1));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    let values = mask_path::values(&before.layers[0].masks[0], 30.);
    let actions = draw(
        &mut controls,
        &engine,
        42,
        mask_path::CHANNEL,
        &press(MouseButton::Left, 24., 12.),
        &mut atlas,
    );
    apply(&mut engine, actions);
    assert!(!animated(
        &engine.project().layers[0].masks[0],
        mask_path::CHANNEL
    ));
    assert_eq!(
        mask_path::values(&engine.project().layers[0].masks[0], 30.),
        values
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    let serialized = serde_json::to_vec(engine.project()).unwrap();
    let reopened: motion_core::Project = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(&reopened, engine.project());
}
