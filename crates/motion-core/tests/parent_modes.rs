use motion_core::{Command, Engine, Layer, ParentMode, Project, Property};
use motion_render::Scene;

fn fixture() -> Engine {
    let mut p = Project::new(640, 480, 30, 120).unwrap();
    for (id, position) in [
        (7, [100., 80., 0.]),
        (8, [200., 120., 0.]),
        (42, [300., 220., 0.]),
    ] {
        p.layers.push(Layer::solid(
            id,
            &format!("Layer {id}"),
            [32.; 2],
            position,
            [1.; 4],
        ));
    }
    let mut engine = Engine::new(p).unwrap();
    for object in [7, 42] {
        engine
            .apply(Command::Animate {
                object,
                property: Property::Position,
                axis: None,
                frame: 0,
                enabled: true,
            })
            .unwrap();
        engine
            .apply(Command::SetVector {
                object,
                property: Property::Position,
                frame: 90,
                value: [400., 70., 0.],
            })
            .unwrap();
    }
    engine
        .apply(Command::SetVector {
            object: 42,
            property: Property::Rotation,
            frame: 0,
            value: [0., 0., 38.],
        })
        .unwrap();
    engine
}
fn world(p: &Project, object: u64, frame: f64) -> glam::Mat4 {
    let mut scene = Scene::new(p).unwrap();
    scene
        .sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene.world_matrix(object).unwrap()
}
fn command(object: u64, parent: Option<u64>, mode: ParentMode) -> Command {
    Command::ParentWithMode {
        object,
        parent,
        frame: 23,
        mode,
    }
}

#[test]
fn both_parent_policies_preserve_tracks_and_roundtrip_as_one_undo() {
    for mode in [ParentMode::PreserveWorld, ParentMode::PreserveLocal] {
        let mut engine = fixture();
        let before = engine.project().clone();
        engine.apply(command(7, Some(42), mode)).unwrap();
        let after = engine.project().clone();
        assert_eq!(after.layers[0].transform, before.layers[0].transform);
        assert_eq!(after.layers[0].timeline, before.layers[0].timeline);
        let link = after.layers[0].parent.as_ref().unwrap();
        assert_eq!(link.object, Some(42));
        let matrix = world(&after, 7, 23.);
        match mode {
            ParentMode::PreserveWorld => assert!(matrix.abs_diff_eq(world(&before, 7, 23.), 0.005)),
            ParentMode::PreserveLocal => {
                assert_eq!(link.bind, glam::Mat4::IDENTITY.to_cols_array_2d());
                assert!(!matrix.abs_diff_eq(world(&before, 7, 23.), 0.005));
                for frame in [0., 23., 45.5, 90.] {
                    let expected = world(&after, 42, frame) * world(&before, 7, frame);
                    assert!(world(&after, 7, frame).abs_diff_eq(expected, 0.005));
                }
            }
        }
        let directory = tempfile::tempdir().unwrap();
        motion_core::storage::save(directory.path(), &after).unwrap();
        let reopened = motion_core::storage::load(directory.path()).unwrap();
        assert_eq!(reopened, after);
        for frame in [0., 23., 45.5, 90.] {
            assert!(world(&reopened, 7, frame).abs_diff_eq(world(&after, 7, frame), 0.005));
        }
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        engine.redo().unwrap();
        assert_eq!(engine.project(), &after);
    }
}

#[test]
fn unparenting_local_discards_the_bind_while_world_retains_the_current_offset() {
    let mut base = fixture();
    base.apply(command(7, Some(42), ParentMode::PreserveLocal))
        .unwrap();
    let before = base.project().clone();
    for mode in [ParentMode::PreserveWorld, ParentMode::PreserveLocal] {
        let mut engine = Engine::new(before.clone()).unwrap();
        engine.apply(command(7, None, mode)).unwrap();
        assert_eq!(
            engine.project().layers[0].transform,
            before.layers[0].transform
        );
        match mode {
            ParentMode::PreserveWorld => {
                assert!(world(engine.project(), 7, 23.).abs_diff_eq(world(&before, 7, 23.), 0.005));
                assert!(engine.project().layers[0]
                    .parent
                    .as_ref()
                    .unwrap()
                    .object
                    .is_none());
            }
            ParentMode::PreserveLocal => {
                assert!(engine.project().layers[0].parent.is_none());
                let mut unbound = before.clone();
                unbound.layers[0].parent = None;
                for frame in [0., 23., 45.5, 90.] {
                    assert!(world(engine.project(), 7, frame)
                        .abs_diff_eq(world(&unbound, 7, frame), 0.005));
                }
            }
        }
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }
}

#[test]
fn parent_policy_failures_keep_primary_context_and_atomic_batch_state() {
    for mode in [ParentMode::PreserveWorld, ParentMode::PreserveLocal] {
        let mut engine = fixture();
        for (object, parent) in [(7, Some(7)), (7, Some(999)), (7, Some(0)), (999, Some(42))] {
            let before = engine.project().clone();
            let error = engine.apply(command(object, parent, mode)).unwrap_err();
            assert!(
                matches!(error, motion_core::Error::ParentOperation {
                object: actual, parent: target, frame: 23, mode: actual_mode, ..
            } if actual == object && target == parent && actual_mode == mode),
                "{error}"
            );
            assert!(std::error::Error::source(&error).is_some());
            assert_eq!(engine.project(), &before);
        }
        engine.apply(command(42, Some(7), mode)).unwrap();
        let before = engine.project().clone();
        let error = engine.apply(command(7, Some(42), mode)).unwrap_err();
        assert!(error.to_string().contains("cycle"), "{error}");
        assert_eq!(engine.project(), &before);
        engine
            .apply(Command::Flags {
                object: 8,
                visible: true,
                locked: true,
            })
            .unwrap();
        let before = engine.project().clone();
        let error = engine
            .apply_batch(vec![command(7, None, mode), command(8, None, mode)])
            .unwrap_err();
        assert!(error.to_string().contains("locked"), "{error}");
        assert_eq!(engine.project(), &before);
    }
    let mut engine = fixture();
    engine
        .apply(Command::SetVector {
            object: 42,
            property: Property::Scale,
            frame: 0,
            value: [0., 100., 100.],
        })
        .unwrap();
    let before = engine.project().clone();
    let error = engine
        .apply(command(7, Some(42), ParentMode::PreserveWorld))
        .unwrap_err();
    assert!(error.to_string().contains("zero-scale"), "{error}");
    assert_eq!(engine.project(), &before);
    engine
        .apply(command(7, Some(42), ParentMode::PreserveLocal))
        .unwrap();
    assert_eq!(
        engine.project().layers[0].parent.as_ref().unwrap().object,
        Some(42)
    );
}

#[test]
fn parent_policy_registry_is_explicit_and_multi_object_edits_use_one_history_step() {
    let catalogue = motion_core::CommandRegistry::catalogue();
    let entry = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "parent_with_mode")
        .unwrap();
    assert_eq!(
        entry["inputSchema"]["properties"]["mode"]["enum"],
        serde_json::json!(["preserve_world", "preserve_local"])
    );
    let commands = motion_core::CommandRegistry::parse(
        r#"[
        {"op":"parent_with_mode","object":7,"parent":42,"frame":23,"mode":"preserve_local"},
        {"op":"parent_with_mode","object":8,"parent":42,"frame":23,"mode":"preserve_local"}
    ]"#,
    )
    .unwrap();
    let mut engine = fixture();
    let before = engine.project().clone();
    engine.apply_batch(commands).unwrap();
    for layer in &engine.project().layers[..2] {
        assert_eq!(layer.parent.as_ref().unwrap().object, Some(42));
    }
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(motion_core::CommandRegistry::parse(
        r#"{"op":"parent_with_mode","object":7,"parent":42,"frame":23}"#
    )
    .is_err());
    let parsed =
        motion_core::CommandRegistry::parse(r#"{"op":"parent","object":7,"parent":42,"frame":23}"#)
            .unwrap();
    let [Command::InComposition {
        composition,
        command,
    }] = parsed.as_slice()
    else {
        panic!("registry must preserve the composition envelope: {parsed:?}")
    };
    assert_eq!(composition, motion_core::MAIN_COMPOSITION);
    assert!(matches!(
        **command,
        Command::Parent {
            object: 7,
            parent: Some(42),
            frame: 23
        }
    ));
}
