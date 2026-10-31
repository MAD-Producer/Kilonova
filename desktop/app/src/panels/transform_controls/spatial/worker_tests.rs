use super::*;
use crate::engine::Engine;

pub(super) fn fixture(separated: bool) -> (tempfile::TempDir, Engine, Value, Rect) {
    let (folder, engine, _, view) = super::super::tests::fixture(true);
    let mut parent = motion_core::Layer::solid(8, "Parent", [100.; 2], [960., 540., 0.], [1.; 4]);
    parent.three_d = true;
    let mut commands = vec![
        Command::Add { layer: parent },
        Command::Parent {
            object: 7,
            parent: Some(8),
            frame: 0,
        },
        Command::SetVector {
            object: 8,
            property: Property::Scale,
            frame: 0,
            value: [-140., 70., 180.],
        },
        Command::SetVector {
            object: 8,
            property: Property::Rotation,
            frame: 0,
            value: [12., 28., 17.],
        },
        Command::SetVector {
            object: 7,
            property: Property::Scale,
            frame: 0,
            value: [140., 80., 120.],
        },
        Command::SetVector {
            object: 7,
            property: Property::Rotation,
            frame: 0,
            value: [20., -30., 18.],
        },
        Command::Anchor {
            object: 7,
            anchor: [0.2, 0.7],
        },
        Command::SetRotationOrder {
            object: 7,
            order: motion_core::RotationOrder::FixedXyz,
        },
    ];
    if separated {
        for property in [Property::Position, Property::Scale] {
            commands.push(Command::SeparateDimensions {
                object: 7,
                property,
            });
        }
    }
    engine
        .edit(serde_json::to_string(&commands).unwrap())
        .unwrap();
    let created = engine
        .edit(serde_json::to_string(&Command::CreateCamera).unwrap())
        .unwrap();
    let camera = created["project"]["layers"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let state = engine
        .edit(
            serde_json::to_string(&Command::SetVector {
                object: camera,
                property: Property::Position,
                frame: 0,
                value: [2100., -100., -2000.],
            })
            .unwrap(),
        )
        .unwrap();
    (folder, engine, state, view)
}

fn pointer(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
fn draw(controls: &mut Controls, engine: &Engine, state: &Value, view: Rect, input: &Input) {
    let mut actions = vec![];
    controls.draw(
        &mut PaintList::default(),
        view,
        state,
        Some(7),
        input,
        &mut actions,
    );
    super::super::tests::apply(engine, actions);
}
fn track<'a>(state: &'a Value, property: &str) -> &'a Value {
    &state["project"]["layers"][0]["transform"][property]
}
fn trajectory(frame: &Frame, constraint: Constraint) -> (DVec3, DVec3) {
    match constraint {
        Constraint::Axis(i) => (
            axis(i) * frame.lengths[i].unwrap() * 0.7,
            axis(i) * frame.lengths[i].unwrap() * 0.2,
        ),
        Constraint::Plane(i) => {
            let a = (i + 1) % 3;
            let b = (i + 2) % 3;
            (
                axis(a) * frame.lengths[a].unwrap() * 0.3
                    + axis(b) * frame.lengths[b].unwrap() * 0.3,
                axis(a) * frame.lengths[a].unwrap() * 0.2
                    - axis(b) * frame.lengths[b].unwrap() * 0.15,
            )
        }
    }
}

#[test]
fn actual_worker_constraints_preserve_other_tracks_and_one_undo_restores_the_full_project() {
    for separated in [false, true] {
        let (_folder, engine, before, view) = fixture(separated);
        for tool in [Tool::Move, Tool::Scale] {
            let property = if tool == Tool::Move {
                "position"
            } else {
                "scale"
            };
            let pose = Pose::new(&before, 7, view).unwrap();
            let frame = Frame::new(&pose, tool, &PaintList::default().styles()).unwrap();
            for constraint in [
                Constraint::Axis(0),
                Constraint::Axis(1),
                Constraint::Axis(2),
                Constraint::Plane(0),
                Constraint::Plane(1),
                Constraint::Plane(2),
            ] {
                let (start, delta) = trajectory(&frame, constraint);
                let from = frame.projection.screen3(start).unwrap();
                let to = frame.projection.screen3(start + delta).unwrap();
                assert!(
                    distance(from, to) >= 3.,
                    "fixture must exceed the production threshold"
                );
                let handle = Handle::Spatial(constraint, tool == Tool::Scale);
                let mut drag =
                    Drag::new(Seed::new(from, &before, view), pose.clone(), handle).unwrap();
                drag.spatial_context = context(&before, 7, handle);
                assert!(drag.spatial_context.is_some());
                let actions = drag.commands(&before, &pointer(to));
                assert!(matches!(actions.first(), Some(Action::History(2))));
                super::super::tests::apply(&engine, actions);
                let after = engine.state().unwrap();
                assert!(
                    drag.spatial_invalid(&after).is_none(),
                    "own edit invalidated the seed"
                );
                let sampled = after["sampledLayers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|layer| layer["id"] == 7)
                    .unwrap();
                let actual = array::<3>(&sampled[property]).unwrap();
                let original = if tool == Tool::Move {
                    pose.position
                } else {
                    pose.scale
                };
                for i in 0..3 {
                    let active = match constraint {
                        Constraint::Axis(a) => i == a,
                        Constraint::Plane(n) => i != n,
                    };
                    let expected = if !active {
                        original[i]
                    } else if tool == Tool::Move {
                        original[i] + delta[i] as f32
                    } else {
                        original[i] * (1. + delta[i] as f32 / frame.lengths[i].unwrap() as f32)
                    };
                    assert!(
                        (actual[i] - expected).abs() < 0.03,
                        "{tool:?} {constraint:?} separated={separated}: {actual:?}"
                    );
                }
                let mut expected = before["project"].clone();
                expected["layers"][0]["transform"][property] = track(&after, property).clone();
                assert_eq!(
                    after["project"], expected,
                    "unrelated persistent state changed"
                );
                assert!(drag.commands(&after, &pointer(to)).is_empty());
                engine.history(3).unwrap();
                assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
            }
        }
    }
}

#[test]
fn spatial_context_rejects_lock_hierarchy_camera_mode_and_document_changes_without_followup_edits()
{
    let (_folder, engine, before, view) = fixture(false);
    let pose = Pose::new(&before, 7, view).unwrap();
    let frame = Frame::new(&pose, Tool::Move, &PaintList::default().styles()).unwrap();
    let (start, delta) = trajectory(&frame, Constraint::Axis(0));
    let from = frame.projection.screen3(start).unwrap();
    let to = frame.projection.screen3(start + delta).unwrap();
    let mut changes = vec![];
    for path in [
        "/project/layers/0/locked",
        "/project/layers/0/three_d",
        "/project/layers/0/parent/bind/0/0",
        "/sampledLayers/1/scale",
        "/sampledLayers/2/position",
        "/sampledCamera/position",
        "/viewRevision",
        "/document_generation",
    ] {
        let mut changed = before.clone();
        let previous = changed.pointer(path).unwrap().clone();
        *changed.pointer_mut(path).unwrap() = match path {
            "/project/layers/0/locked" => true.into(),
            "/project/layers/0/three_d" => false.into(),
            "/viewRevision" | "/document_generation" => (previous.as_u64().unwrap() + 1).into(),
            "/project/layers/0/parent/bind/0/0" => 2.into(),
            _ => serde_json::json!([999., 1., 2.]),
        };
        changes.push((path, changed));
    }
    for (path, mut changed) in changes {
        let mut controls = Controls {
            tool: Tool::Move,
            ..Default::default()
        };
        // Seed through the actual Controls path, including context capture.
        draw(
            &mut controls,
            &engine,
            &before,
            view,
            &Input {
                pressed: Some((MouseButton::Left, from)),
                ..pointer(from)
            },
        );
        assert!(matches!(
            controls.drag.as_ref().unwrap().handle,
            Handle::Spatial(Constraint::Axis(0), false)
        ));
        draw(&mut controls, &engine, &before, view, &pointer(to));
        assert_ne!(engine.state().unwrap()["project"], before["project"]);
        let after = engine.state().unwrap();
        // Preserve the real preview edit; only inject the tested invalidation.
        changed["project"]["layers"][0]["transform"]["position"] =
            track(&after, "position").clone();
        let mut actions = vec![];
        controls.draw(
            &mut PaintList::default(),
            view,
            &changed,
            Some(7),
            &pointer([to[0] + 20., to[1]]),
            &mut actions,
        );
        assert!(
            matches!(actions.as_slice(), [Action::History(4)]),
            "{path}: {actions:?}"
        );
        assert!(!controls.gesture_active());
        super::super::tests::apply(&engine, actions);
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
    }
}

#[test]
fn spatial_release_focus_cancel_and_persistence_use_the_existing_preview_history() {
    let (folder, engine, before, view) = fixture(false);
    let pose = Pose::new(&before, 7, view).unwrap();
    let frame = Frame::new(&pose, Tool::Move, &PaintList::default().styles()).unwrap();
    let (start, delta) = trajectory(&frame, Constraint::Axis(0));
    let from = frame.projection.screen3(start).unwrap();
    let to = frame.projection.screen3(start + delta).unwrap();
    for cancel in [
        Input {
            focus_lost: true,
            ..Default::default()
        },
        Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        },
    ] {
        let mut controls = Controls::default();
        draw(
            &mut controls,
            &engine,
            &before,
            view,
            &Input {
                pressed: Some((MouseButton::Left, from)),
                ..pointer(from)
            },
        );
        draw(&mut controls, &engine, &before, view, &pointer(to));
        draw(
            &mut controls,
            &engine,
            &engine.state().unwrap(),
            view,
            &cancel,
        );
        assert!(!controls.gesture_active());
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
    }
    let mut controls = Controls::default();
    draw(
        &mut controls,
        &engine,
        &before,
        view,
        &Input {
            pressed: Some((MouseButton::Left, from)),
            ..pointer(from)
        },
    );
    draw(&mut controls, &engine, &before, view, &pointer(to));
    let after = engine.state().unwrap();
    draw(
        &mut controls,
        &engine,
        &after,
        view,
        &Input {
            mouse: to,
            released: Some((MouseButton::Left, to)),
            ..Default::default()
        },
    );
    assert!(!controls.gesture_active());
    draw(
        &mut controls,
        &engine,
        &after,
        view,
        &Input {
            mouse: [to[0] + 100., to[1]],
            ..Default::default()
        },
    );
    assert_eq!(engine.state().unwrap()["project"], after["project"]);
    engine.save().unwrap();
    assert_eq!(
        serde_json::to_value(motion_core::storage::load(&folder.path().join("project")).unwrap())
            .unwrap(),
        after["project"]
    );
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
    drop(engine);
    let reopened = Engine::start(
        folder.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    assert_eq!(reopened.state().unwrap()["project"], after["project"]);
}

#[test]
fn actual_plane_presses_reach_constraints_and_locked_or_singular_handles_explain_refusal() {
    let (_folder, engine, before, view) = fixture(false);
    for tool in [Tool::Move, Tool::Scale] {
        let pose = Pose::new(&before, 7, view).unwrap();
        let frame = Frame::new(&pose, tool, &PaintList::default().styles()).unwrap();
        for normal in 0..3 {
            let constraint = Constraint::Plane(normal);
            let (start, delta) = trajectory(&frame, constraint);
            let from = frame.projection.screen3(start).unwrap();
            let to = frame.projection.screen3(start + delta).unwrap();
            let mut controls = Controls {
                tool,
                ..Default::default()
            };
            draw(
                &mut controls,
                &engine,
                &before,
                view,
                &Input {
                    pressed: Some((MouseButton::Left, from)),
                    ..pointer(from)
                },
            );
            assert_eq!(
                controls.drag.as_ref().unwrap().handle,
                Handle::Spatial(constraint, tool == Tool::Scale)
            );
            draw(&mut controls, &engine, &before, view, &pointer(to));
            let after = engine.state().unwrap();
            assert_ne!(after["project"], before["project"]);
            draw(
                &mut controls,
                &engine,
                &after,
                view,
                &Input {
                    mouse: to,
                    released: Some((MouseButton::Left, to)),
                    ..Default::default()
                },
            );
            assert!(!controls.gesture_active());
            assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        }
    }
    let locked = engine
        .edit(
            serde_json::to_string(&Command::Flags {
                object: 7,
                visible: true,
                locked: true,
            })
            .unwrap(),
        )
        .unwrap();
    let pose = Pose::new(&locked, 7, view).unwrap();
    let frame = Frame::new(&pose, Tool::Move, &PaintList::default().styles()).unwrap();
    let point = frame.tips[0].unwrap();
    let mut controls = Controls::default();
    assert!(controls.handle_hit(
        &locked,
        Some(7),
        view,
        point,
        &PaintList::default().styles()
    ));
    let mut actions = vec![];
    assert!(controls.draw(
        &mut PaintList::default(),
        view,
        &locked,
        Some(7),
        &Input {
            pressed: Some((MouseButton::Left, point)),
            ..pointer(point)
        },
        &mut actions
    ));
    assert!(actions.is_empty());
    assert!(matches!(controls.notice.take(), Some(Problem::Locked)));
    assert!(!controls.gesture_active());
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    let singular = engine
        .edit(
            serde_json::to_string(&Command::SetVector {
                object: 8,
                property: Property::Scale,
                frame: 0,
                value: [0., 70., 180.],
            })
            .unwrap(),
        )
        .unwrap();
    assert!(Pose::new(&singular, 7, view).is_none());
    let anchor = singular["projectedLayers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["id"] == 7)
        .unwrap()["anchor"]
        .clone();
    // project_node carries composition X/Y plus depth; hit testing uses X/Y.
    let point = [
        anchor[0].as_f64().unwrap() as f32,
        anchor[1].as_f64().unwrap() as f32,
    ];
    let mut actions = vec![];
    controls.draw(
        &mut PaintList::default(),
        view,
        &singular,
        Some(7),
        &Input {
            pressed: Some((MouseButton::Left, point)),
            ..pointer(point)
        },
        &mut actions,
    );
    assert!(actions.is_empty());
    assert!(matches!(controls.notice.take(), Some(Problem::Transform)));
    assert!(!controls.gesture_active());
    assert_eq!(engine.state().unwrap()["project"], singular["project"]);
}

#[test]
fn spatial_context_tracks_camera_parent_motion_and_shared_or_legacy_camera_ancestors() {
    let (_folder, engine, _, view) = fixture(false);
    let shared = engine
        .edit(
            serde_json::to_string(&Command::Parent {
                object: 9,
                parent: Some(8),
                frame: 0,
            })
            .unwrap(),
        )
        .unwrap();
    let handle = Handle::Spatial(Constraint::Axis(0), false);
    assert!(
        context(&shared, 7, handle).is_some(),
        "shared camera/layer ancestor is not a cycle"
    );
    let before = engine
        .edit(
            serde_json::to_string(&Command::Parent {
                object: 7,
                parent: None,
                frame: 0,
            })
            .unwrap(),
        )
        .unwrap();
    let pose = Pose::new(&before, 7, view).unwrap();
    let frame = Frame::new(&pose, Tool::Move, &PaintList::default().styles()).unwrap();
    let point = frame.tips[0].unwrap();
    let mut drag = Drag::new(Seed::new(point, &before, view), pose, handle).unwrap();
    drag.spatial_context = context(&before, 7, handle);
    assert!(drag.spatial_context.is_some());
    let changed = engine
        .edit(
            serde_json::to_string(&Command::SetVector {
                object: 8,
                property: Property::Scale,
                frame: 0,
                value: [-160., 70., 180.],
            })
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        drag.spatial_invalid(&changed),
        Some(Problem::Changed)
    ));

    let folder = tempfile::tempdir().unwrap();
    let mut project = motion_core::Project::new(1920, 1080, 30, 100).unwrap();
    project.camera = motion_core::Camera::new(1920, 1080);
    let mut layer =
        motion_core::Layer::solid(7, "Camera child", [200., 100.], [960., 540., 0.], [1.; 4]);
    layer.three_d = true;
    project.layers.push(layer);
    motion_core::storage::save(folder.path(), &project).unwrap();
    let engine = Engine::start(
        folder.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let state = engine
        .edit(
            serde_json::to_string(&Command::Parent {
                object: 7,
                parent: Some(0),
                frame: 0,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(invertible(&state, 7));
    assert!(context(&state, 7, handle).is_some());
}
