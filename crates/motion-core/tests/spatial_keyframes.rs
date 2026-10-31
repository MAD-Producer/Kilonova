use motion_core::{
    parse_commands, storage, CameraMode, Command, CommandRegistry, Ease, Engine,
    ExpressionEvaluator, KeyframeTarget, Layer, LayerTimeline, Project, Property, Track,
};
use motion_model::keyframe_clipboard::spatial::SpatialKeyEdit;
use motion_render::Scene;

fn target(object: u64, property: Property) -> KeyframeTarget {
    KeyframeTarget::Property {
        object,
        property,
        axis: None,
    }
}
fn command(object: u64, edit: SpatialKeyEdit) -> Command {
    Command::SpatialKeyframes {
        target: target(object, Property::Position),
        edit,
    }
}
fn fixture() -> Engine {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "spatial", [32.; 2], [0.; 3], [1.; 4]);
    layer.timeline = Some(LayerTimeline {
        in_frame: 7,
        out_frame: 100,
        offset_frame: 7,
    });
    for (frame, x) in [(0, 0.), (4, 1.), (10, 7.), (20, 10.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 0., 0.], Ease::Linear)
            .unwrap();
    }
    project.layers.push(layer);
    Engine::new(project).unwrap()
}
fn enable(engine: &mut Engine) {
    engine
        .apply(command(7, SpatialKeyEdit::Enable { frames: vec![11] }))
        .unwrap();
}

#[test]
fn group_translation_uses_one_source_schedule_and_preserves_ids_and_one_step_history() {
    let mut engine = fixture();
    enable(&mut engine);
    let before = engine.snapshot();
    let track = &before.layers[0].transform.position;
    let mut sequential = track.spatial_animation.clone().unwrap();
    assert!(
        sequential.set_fixed_time(1, 15.).is_err(),
        "old neighbour must block a sequential first-key edit"
    );
    let input = r#"{"op":"spatial_keyframes","target":{"kind":"property","object":7,"property":"position","axis":null},"edit":{"kind":"move","keys":[1,2,3,4],"delta":15.25}}"#;
    CommandRegistry::run(&mut engine, parse_commands(input).unwrap()).unwrap();
    let moved = engine.snapshot();
    for ((id, a), (actual_id, b)) in track
        .spatial_animation
        .as_ref()
        .unwrap()
        .prepared()
        .keys()
        .zip(
            moved.layers[0]
                .transform
                .position
                .spatial_animation
                .as_ref()
                .unwrap()
                .prepared()
                .keys(),
        )
    {
        assert_eq!(id, actual_id);
        assert_eq!(b, a + 15.25);
    }
    assert!(moved.layers[0]
        .transform
        .position
        .spatial_animation
        .as_ref()
        .unwrap()
        .keys()
        .iter()
        .all(|key| key.fixed.is_some()));
    assert_eq!(before.layers[0].timeline, moved.layers[0].timeline);
    for frame in [0., 1.25, 4., 7.75, 10., 20.] {
        assert_eq!(
            track.sample(frame).unwrap(),
            moved.layers[0]
                .transform
                .position
                .sample(frame + 15.25)
                .unwrap()
        );
    }
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &moved);
    let tmp = tempfile::tempdir().unwrap();
    storage::save(tmp.path(), &moved).unwrap();
    assert_eq!(storage::load(tmp.path()).unwrap(), moved);
}

#[test]
fn group_move_failures_preserve_source_history_and_redo_and_zero_delta_keeps_roving() {
    let mut engine = fixture();
    enable(&mut engine);
    engine
        .apply(command(7, SpatialKeyEdit::Time { key: 2, time: 8.25 }))
        .unwrap();
    engine.undo().unwrap();
    let original = engine.snapshot();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    for edit in [
        SpatialKeyEdit::Move {
            keys: vec![1, 999],
            delta: 1.,
        },
        SpatialKeyEdit::Move {
            keys: vec![2, 2],
            delta: 1.,
        },
        SpatialKeyEdit::Move {
            keys: vec![],
            delta: 1.,
        },
        SpatialKeyEdit::Move {
            keys: vec![2],
            delta: f64::NAN,
        },
        SpatialKeyEdit::Move {
            keys: vec![2],
            delta: 40.,
        },
    ] {
        let error = engine.apply(command(7, edit)).unwrap_err().to_string();
        assert!(
            error.contains("operation=spatial_keyframes") && error.contains("phase=move"),
            "{error}"
        );
        assert_eq!(engine.project(), &original);
        assert_eq!(engine.revision(), revision);
        assert_eq!(engine.history_diagnostics(), history);
    }
    engine
        .apply(command(
            7,
            SpatialKeyEdit::Move {
                keys: vec![2],
                delta: 0.,
            },
        ))
        .unwrap();
    assert_eq!(engine.project(), &original);
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.history_diagnostics(), history);
}

#[test]
fn registry_conversion_and_id_time_edits_keep_full_undo_redo_and_fractional_offsets() {
    let mut engine = fixture();
    let before = engine.snapshot();
    CommandRegistry::run(&mut engine,parse_commands(r#"{"op":"spatial_keyframes","target":{"kind":"property","object":7,"property":"position","axis":null},"edit":{"kind":"enable","frames":[11]}}"#).unwrap()).unwrap();
    let roving = engine.snapshot();
    assert_eq!(
        roving.layers[0].transform.position.key_frames().nth(1),
        Some(10. / 7.)
    );
    engine
        .apply(command(7, SpatialKeyEdit::Time { key: 2, time: 8.25 }))
        .unwrap();
    let moved = engine.snapshot();
    let track = &moved.layers[0].transform.position;
    assert_eq!(track.key_frames().nth(1), Some(1.25));
    assert_eq!(track.spatial_animation.as_ref().unwrap().keys()[1].id, 2);
    assert_eq!(
        track.spatial_animation.as_ref().unwrap().keys()[1].fixed,
        Some(1.25)
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &roving);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &roving);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &moved);
    let state = engine.history_diagnostics();
    assert_eq!(state["undoEntries"], 2);
    assert_eq!(state["redoEntries"], 0);
}

#[test]
fn document_and_package_reopen_and_frozen_scene_share_the_real_sampler() {
    let mut engine = fixture();
    let ordinary = engine.snapshot();
    enable(&mut engine);
    let frozen = engine.snapshot();
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_SPATIAL_TRACK_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    storage::save(&root, &frozen).unwrap();
    let reopened = storage::load(&root).unwrap();
    let package = root.join("spatial.msproj");
    storage::export_package(&root, &frozen, &package).unwrap();
    let imported = storage::import_package(&package, &root.join("reopened")).unwrap();
    assert_eq!(frozen, reopened);
    assert_eq!(frozen, imported);
    let mut a = Scene::new(&frozen).unwrap();
    let mut b = Scene::new(&reopened).unwrap();
    let mut c = Scene::new(&ordinary).unwrap();
    let mut trace = Vec::new();
    for tick in 0..=100 {
        let frame = 7. + f64::from(tick) / 10.;
        a.sample(&frozen, frame, None, &ExpressionEvaluator)
            .unwrap();
        b.sample(&reopened, frame, None, &ExpressionEvaluator)
            .unwrap();
        assert_eq!(a.world_matrix(7), b.world_matrix(7));
        // Independent straight-path expectation, not a second call to Track.
        let mut expected = frozen.clone();
        expected.layers[0].transform.position =
            Track::constant([0.7 * (frame as f32 - 7.), 0., 0.]);
        let mut s = Scene::new(&expected).unwrap();
        s.sample(&expected, frame, None, &ExpressionEvaluator)
            .unwrap();
        let actual = a.world_matrix(7).unwrap().to_cols_array();
        let expected = s.world_matrix(7).unwrap().to_cols_array();
        trace.push(serde_json::json!({"frame":frame,"matrix":actual,"expected":expected}));
        for (a, b) in actual.into_iter().zip(expected) {
            assert!((a - b).abs() < 1e-5, "frame={frame}: {a} {b}");
        }
    }
    a.sample(&frozen, 12., None, &ExpressionEvaluator).unwrap();
    c.sample(&ordinary, 12., None, &ExpressionEvaluator)
        .unwrap();
    assert_ne!(
        a.world_matrix(7),
        c.world_matrix(7),
        "ordinary integer-time scene must differ"
    );
    std::fs::write(root.join("scene-samples.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "samples":trace,"negativeControl":{"frame":12.,"roving":a.world_matrix(7).unwrap().to_cols_array(),
        "ordinary":c.world_matrix(7).unwrap().to_cols_array()}})).unwrap()).unwrap();
    engine
        .apply(Command::SetVector {
            object: 7,
            property: Property::Position,
            frame: 17,
            value: [8., 2., 0.],
        })
        .unwrap();
    assert_ne!(engine.project(), &frozen);
    assert_eq!(
        frozen, reopened,
        "editing must not mutate a frozen export snapshot"
    );
    assert_eq!(
        engine.project().layers[0]
            .transform
            .position
            .spatial_animation
            .as_ref()
            .unwrap()
            .keys()[2]
            .id,
        3
    );
}

#[test]
fn failures_keep_project_revision_history_and_redo_and_preserve_original_causes() {
    use std::error::Error as _;
    let mut engine = fixture();
    enable(&mut engine);
    let roving = engine.snapshot();
    engine
        .apply(Command::Rename {
            object: 7,
            name: "redo".into(),
        })
        .unwrap();
    engine.undo().unwrap();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    for edit in [
        SpatialKeyEdit::Roving {
            keys: vec![1],
            enabled: true,
        },
        SpatialKeyEdit::Time { key: 999, time: 8. },
        SpatialKeyEdit::Time { key: 2, time: 18. },
        SpatialKeyEdit::Time {
            key: 2,
            time: f64::NAN,
        },
        SpatialKeyEdit::Delete { key: 999 },
        SpatialKeyEdit::Roving {
            keys: vec![2, 2],
            enabled: true,
        },
    ] {
        let error = engine.apply(command(7, edit)).unwrap_err();
        assert!(
            error.to_string().contains("operation=spatial_keyframes"),
            "{error}"
        );
        assert!(error.source().is_some(), "{error}");
        assert_eq!(engine.project(), &roving);
        assert_eq!(engine.revision(), revision);
        assert_eq!(engine.history_diagnostics(), history);
    }
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project().layers[0].name, "redo");
}

#[test]
fn unsupported_properties_locks_and_batch_failure_do_not_partially_enable_roving() {
    let mut engine = fixture();
    let before = engine.snapshot();
    for property in [Property::Rotation, Property::Scale, Property::Opacity] {
        assert!(engine
            .apply(Command::SpatialKeyframes {
                target: target(7, property),
                edit: SpatialKeyEdit::Enable { frames: vec![11] }
            })
            .is_err());
        assert_eq!(engine.project(), &before);
    }
    assert!(engine
        .apply_batch(vec![
            command(7, SpatialKeyEdit::Enable { frames: vec![11] }),
            command(
                7,
                SpatialKeyEdit::Roving {
                    keys: vec![999],
                    enabled: true
                }
            )
        ])
        .is_err());
    assert_eq!(engine.project(), &before);
    let mut locked = before.clone();
    locked.layers[0].locked = true;
    let mut engine = Engine::new(locked.clone()).unwrap();
    assert!(engine
        .apply(command(7, SpatialKeyEdit::Enable { frames: vec![11] }))
        .unwrap_err()
        .to_string()
        .contains("locked"));
    assert_eq!(engine.project(), &locked);
}

#[test]
fn id_delete_and_freeze_survive_save_reopen_and_keep_survivor_identity() {
    let mut engine = fixture();
    enable(&mut engine);
    let before = engine.snapshot();
    engine
        .apply(command(
            7,
            SpatialKeyEdit::Roving {
                keys: vec![2],
                enabled: false,
            },
        ))
        .unwrap();
    let frozen = engine.snapshot();
    for tick in 0..100 {
        let at = f64::from(tick) / 10.;
        assert_eq!(
            before.layers[0].transform.position.sample(at).unwrap(),
            frozen.layers[0].transform.position.sample(at).unwrap()
        );
    }
    engine
        .apply(command(7, SpatialKeyEdit::Delete { key: 2 }))
        .unwrap();
    assert_eq!(
        engine.project().layers[0]
            .transform
            .position
            .spatial_animation
            .as_ref()
            .unwrap()
            .keys()
            .iter()
            .map(|k| k.id)
            .collect::<Vec<_>>(),
        vec![1, 3, 4]
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &frozen);
}

#[test]
fn old_integer_bakes_fail_explicitly_instead_of_losing_continuous_timing() {
    let mut engine = fixture();
    enable(&mut engine);
    let before = engine.snapshot();
    assert!(engine
        .apply(Command::Anchor {
            object: 7,
            anchor: [0.2, 0.8]
        })
        .unwrap_err()
        .to_string()
        .contains("continuous spatial timing"));
    assert_eq!(engine.project(), &before);
    let mut camera = before.camera.clone();
    camera.position = before.layers[0].transform.position.clone();
    camera.target = Track::constant([0., 0., 100.]);
    let before = camera.clone();
    assert!(camera
        .convert_mode_range(CameraMode::Orbit, 0, 120)
        .unwrap_err()
        .to_string()
        .contains("continuous spatial timing"));
    assert_eq!(camera, before);
}
