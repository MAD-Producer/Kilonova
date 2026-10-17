use motion_core::{CameraMode, Command, EditResult, Engine, Layer, Project, Property, Track};
use motion_render::Scene;

fn create(e: &mut Engine) -> u64 {
    match e
        .apply_batch(vec![Command::CreateCamera])
        .unwrap()
        .as_slice()
    {
        [EditResult::CreateCamera { object }] => *object,
        result => panic!("unexpected create result: {result:?}"),
    }
}
fn scene(p: &Project, frame: f64) -> Scene {
    let mut scene = Scene::new(p);
    scene
        .sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn empty() -> Engine {
    Engine::new(Project::new(640, 360, 30, 120).unwrap()).unwrap()
}

#[test]
fn repeated_creation_keeps_distinct_layers_and_undo_redo_and_storage() {
    let mut e = empty();
    let a = create(&mut e);
    e.apply(Command::SetScalar {
        object: a,
        property: Property::Fov,
        frame: 0,
        value: 60.,
    })
    .unwrap();
    let before = e.snapshot();
    let b = create(&mut e);
    assert_ne!(a, b);
    assert!(!e.project().camera.created);
    assert_eq!(e.project().camera_for(a).unwrap().fov.value, 60.);
    assert_eq!(e.project().camera_for(b).unwrap().fov.value, 45.);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    e.redo().unwrap();
    e.apply(Command::Duplicate { object: a }).unwrap();
    let duplicate = e.project().layers.last().unwrap().id;
    assert_ne!(duplicate, a);
    assert_eq!(
        e.project().camera_for(duplicate).unwrap(),
        e.project().camera_for(a).unwrap()
    );
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), e.project()).unwrap();
    assert_eq!(
        motion_core::storage::load(root.path()).unwrap(),
        e.snapshot()
    );
}

#[test]
fn legacy_camera_is_visible_in_serialized_state_and_does_not_block_creation() {
    let mut value = serde_json::to_value(Project::demo()).unwrap();
    value["camera"].as_object_mut().unwrap().remove("created");
    let legacy: Project = serde_json::from_value(value).unwrap();
    assert!(legacy.camera.created);
    assert_eq!(
        serde_json::to_value(&legacy).unwrap()["camera"]["created"],
        true
    );
    let original = legacy.camera.clone();
    let mut e = Engine::new(legacy).unwrap();
    let a = create(&mut e);
    let b = create(&mut e);
    assert_ne!(a, b);
    assert_eq!(e.project().camera, original);
    e.apply(Command::Remove {
        object: b,
        frame: 0,
    })
    .unwrap();
    e.apply(Command::Remove {
        object: a,
        frame: 0,
    })
    .unwrap();
    assert_eq!(e.project().camera, original);
}

#[test]
fn rendering_uses_topmost_enabled_in_range_camera_and_implicit_fallback() {
    let mut e = empty();
    let a = create(&mut e);
    let b = create(&mut e);
    e.apply(Command::SetVector {
        object: a,
        property: Property::Position,
        frame: 0,
        value: [320., 180., -1000.],
    })
    .unwrap();
    e.apply(Command::SetVector {
        object: b,
        property: Property::Position,
        frame: 0,
        value: [320., 180., -2000.],
    })
    .unwrap();
    assert_eq!(scene(e.project(), 0.).camera.eye.z, 2000.);
    assert!(scene(e.project(), 0.).layers.is_empty());
    e.apply(Command::SetLayerRange {
        object: b,
        in_frame: 30.5,
        out_frame: 60.25,
    })
    .unwrap();
    for (frame, z) in [
        (30.499, 1000.),
        (30.5, 2000.),
        (60.249, 2000.),
        (60.25, 1000.),
    ] {
        assert_eq!(scene(e.project(), frame).camera.eye.z, z);
    }
    e.apply(Command::Flags {
        object: b,
        visible: false,
        locked: false,
    })
    .unwrap();
    assert_eq!(scene(e.project(), 45.).camera.eye.z, 1000.);
    e.apply(Command::Flags {
        object: b,
        visible: true,
        locked: false,
    })
    .unwrap();
    e.apply(Command::Reorder {
        object: a,
        index: 1,
    })
    .unwrap();
    assert_eq!(scene(e.project(), 45.).camera.eye.z, 1000.);
    e.apply(Command::Remove {
        object: a,
        frame: 45,
    })
    .unwrap();
    assert_eq!(scene(e.project(), 45.).camera.eye.z, 2000.);
    e.apply(Command::Remove {
        object: b,
        frame: 45,
    })
    .unwrap();
    let expected = motion_core::Camera::new(640, 360).pose(45., 640, 360);
    assert!(scene(e.project(), 45.)
        .camera
        .view_projection
        .abs_diff_eq(expected.view_projection, 1e-5));
}

#[test]
fn camera_keys_follow_local_clip_time_and_mode_conversion_and_splits() {
    let mut e = empty();
    let a = create(&mut e);
    e.apply(Command::Animate {
        object: a,
        property: Property::Position,
        axis: None,
        frame: 0,
        enabled: true,
    })
    .unwrap();
    e.apply(Command::SetVector {
        object: a,
        property: Property::Position,
        frame: 60,
        value: [420., 180., -800.],
    })
    .unwrap();
    e.apply(Command::MoveLayerClip {
        object: a,
        in_frame: -30,
    })
    .unwrap();
    let before = e.snapshot();
    let poses: Vec<_> = (0..90)
        .map(|f| scene(e.project(), f as f64).camera.view_projection)
        .collect();
    e.apply(Command::CameraMode {
        object: a,
        mode: CameraMode::Orbit,
    })
    .unwrap();
    e.apply(Command::CameraMode {
        object: a,
        mode: CameraMode::Position,
    })
    .unwrap();
    for (frame, expected) in poses.iter().enumerate() {
        assert!(scene(e.project(), frame as f64)
            .camera
            .view_projection
            .abs_diff_eq(*expected, 1e-3));
    }
    e.undo().unwrap();
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    e.apply(Command::SplitLayerClip {
        object: a,
        frame: 30,
    })
    .unwrap();
    for (frame, expected) in poses.iter().enumerate() {
        assert!(scene(e.project(), frame as f64)
            .camera
            .view_projection
            .abs_diff_eq(*expected, 1e-5));
    }
}

#[test]
fn camera_parenting_and_parent_deletion_keep_world_pose_and_child_animation() {
    let mut e = empty();
    let a = create(&mut e);
    let mut rig = Layer::solid(99, "Rig", [10.; 2], [320., 180., 0.], [0.; 4]);
    rig.content = motion_core::Content::Null;
    rig.three_d = true;
    e.apply(Command::Add { layer: rig }).unwrap();
    let original = scene(e.project(), 0.).camera.view_projection;
    e.apply(Command::Parent {
        object: a,
        parent: Some(99),
        frame: 0,
    })
    .unwrap();
    assert!(scene(e.project(), 0.)
        .camera
        .view_projection
        .abs_diff_eq(original, 1e-4));
    e.apply(Command::SetVector {
        object: 99,
        property: Property::Position,
        frame: 0,
        value: [420., 180., 0.],
    })
    .unwrap();
    let moved = scene(e.project(), 0.).camera.view_projection;
    assert!(!moved.abs_diff_eq(original, 1e-3));
    e.apply(Command::Remove {
        object: 99,
        frame: 0,
    })
    .unwrap();
    assert!(scene(e.project(), 0.)
        .camera
        .view_projection
        .abs_diff_eq(moved, 1e-4));
}

#[test]
fn camera_expressions_sample_selected_object_and_failed_edits_remain_atomic() {
    let mut e = empty();
    let a = create(&mut e);
    let b = create(&mut e);
    e.apply(Command::SetExpression {
        expression: motion_core::PropertyExpression {
            target: motion_core::ExpressionTarget::Property {
                object: a,
                property: Property::Fov,
                axis: None,
            },
            source: "value + time * 10".into(),
            enabled: true,
            seed: 0,
            profile: motion_core::EXPRESSION_PROFILE.into(),
        },
        frame: 0,
    })
    .unwrap();
    use motion_core::ProjectExpressions;
    let evaluated = e.project().evaluated_at(30.).unwrap();
    assert_eq!(evaluated.camera_for(a).unwrap().fov.value, 55.);
    assert_eq!(evaluated.camera_for(b).unwrap().fov.value, 45.);
    let before = e.snapshot();
    for command in [
        Command::SetScalar {
            object: b,
            property: Property::Fov,
            frame: 0,
            value: 0.,
        },
        Command::SetVector {
            object: b,
            property: Property::Scale,
            frame: 0,
            value: [200.; 3],
        },
        Command::CameraMode {
            object: 999,
            mode: CameraMode::Orbit,
        },
    ] {
        assert!(e.apply(command).is_err());
        assert_eq!(e.project(), &before);
    }
    e.apply(Command::Flags {
        object: b,
        visible: true,
        locked: true,
    })
    .unwrap();
    assert!(e
        .apply(Command::CameraMode {
            object: b,
            mode: CameraMode::Orbit
        })
        .is_err());
    assert!(e
        .apply(Command::SetVector {
            object: b,
            property: Property::Position,
            frame: 0,
            value: [0.; 3]
        })
        .is_err());
    assert_eq!(
        e.project().camera_for(b).unwrap().position,
        Track::constant(motion_core::Camera::new(640, 360).position.value)
    );
}

#[test]
fn mode_conversion_preserves_trimmed_out_keys_and_rejects_unbounded_bakes() {
    let mut e = empty();
    let a = create(&mut e);
    e.apply(Command::Animate {
        object: a,
        property: Property::Position,
        axis: None,
        frame: 0,
        enabled: true,
    })
    .unwrap();
    e.apply(Command::SetVector {
        object: a,
        property: Property::Position,
        frame: 90,
        value: [400., 180., -800.],
    })
    .unwrap();
    let expected = e.project().camera_for(a).unwrap().position_at(90.);
    e.apply(Command::TrimLayerClip {
        object: a,
        in_frame: 30,
        out_frame: 60,
    })
    .unwrap();
    e.apply(Command::CameraMode {
        object: a,
        mode: CameraMode::Orbit,
    })
    .unwrap();
    e.apply(Command::CameraMode {
        object: a,
        mode: CameraMode::Position,
    })
    .unwrap();
    let actual = e.project().camera_for(a).unwrap().position_at(90.);
    for axis in 0..3 {
        assert!((actual[axis] - expected[axis]).abs() < 1e-3);
    }
    let mut p = e.snapshot();
    p.camera_for_mut(a)
        .unwrap()
        .position
        .keys
        .last_mut()
        .unwrap()
        .frame = i32::MAX;
    let mut e = Engine::new(p).unwrap();
    let before = e.snapshot();
    assert!(e
        .apply(Command::CameraMode {
            object: a,
            mode: CameraMode::Orbit
        })
        .unwrap_err()
        .to_string()
        .contains("overflow"));
    assert_eq!(e.project(), &before);
}

#[test]
fn camera_id_exhaustion_and_layer_limit_are_atomic() {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    p.layers.push(Layer::solid(
        u64::MAX,
        "Existing",
        [1.; 2],
        [0.; 3],
        [1.; 4],
    ));
    let mut e = Engine::new(p).unwrap();
    let before = e.snapshot();
    assert!(e
        .apply(Command::CreateCamera)
        .unwrap_err()
        .to_string()
        .contains("ID space exhausted"));
    assert_eq!(e.project(), &before);
    let mut e = empty();
    for _ in 0..motion_core::MAX_LAYERS {
        create(&mut e);
    }
    let before = e.snapshot();
    assert!(e
        .apply(Command::CreateCamera)
        .unwrap_err()
        .to_string()
        .contains("layer limit"));
    assert_eq!(e.project(), &before);
}

#[test]
fn extreme_camera_clip_offset_returns_error_without_mutation() {
    let mut e = empty();
    let a = create(&mut e);
    let mut p = e.snapshot();
    p.layers.iter_mut().find(|layer| layer.id == a).unwrap().timeline =
        Some(motion_core::LayerTimeline {
            in_frame: 0,
            out_frame: 121,
            offset_frame: 120 - i32::MAX,
        });
    let mut e = Engine::new(p).unwrap();
    let before = e.snapshot();
    assert!(e.apply(Command::CameraMode {
        object: a,
        mode: CameraMode::Orbit,
    }).unwrap_err().to_string().contains("overflow"));
    assert_eq!(e.project(), &before);
}
