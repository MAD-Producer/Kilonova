use motion_core::{
    layer_time::{LayerTimeAction, TimeStretch},
    storage, Command, CommandRegistry, Engine, Error, Layer, LayerTimeline, Project,
};
use serde_json::json;

fn engine() -> Engine {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "timed", [20.; 2], [0.; 3], [1.; 4]);
    layer.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 70,
        offset_frame: 10,
    });
    layer.transform.position.set_animated(0, true).unwrap();
    layer
        .transform
        .position
        .set_at(60, [60., 120., 0.])
        .unwrap();
    project.layers.push(layer);
    Engine::new(project).unwrap()
}
fn stretch() -> Command {
    Command::LayerTime {
        object: 7,
        action: LayerTimeAction::Stretch {
            stretch: TimeStretch::new(3, 2).unwrap(),
            anchor_frame: 10.,
            resize_clip: true,
        },
    }
}

#[test]
fn command_transport_and_catalogue_agree_on_required_time_mapping_fields() {
    let mut engine = engine();
    let command = serde_json::to_value(stretch()).unwrap();
    assert_eq!(command["op"], "layer_time");
    let commands = CommandRegistry::parse(&serde_json::to_string(&command).unwrap()).unwrap();
    CommandRegistry::run(&mut engine, commands).unwrap();
    assert_eq!(engine.project().layers[0].local_frame(55.), 30.);
    let catalogue = CommandRegistry::catalogue();
    let schema = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "layer_time")
        .unwrap();
    assert!(schema["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("action")));
    assert_eq!(
        schema["inputSchema"]["properties"]["action"]["oneOf"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    for invalid in [
        json!({"op":"layer_time","object":7}),
        json!({"op":"layer_time","object":7,"action":{"kind":"reverse","typo":true}}),
        json!({"op":"layer_time","object":7,"action":{"kind":"remap"}}),
    ] {
        assert!(CommandRegistry::parse(&invalid.to_string()).is_err());
    }
}

#[test]
fn mapping_undo_redo_and_real_storage_roundtrip_restore_the_entire_project() {
    let mut engine = engine();
    let original = engine.snapshot();
    engine.apply(stretch()).unwrap();
    let stretched = engine.snapshot();
    assert_ne!(original, stretched);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("时间映射-保存重开");
    storage::save(&path, &stretched).unwrap();
    let restored = storage::load(&path).unwrap();
    assert_eq!(restored, stretched);
    let reopened = Engine::new(restored).unwrap();
    assert_eq!(
        reopened.project().layers[0]
            .property_composition_frame(30.)
            .unwrap(),
        55.
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.snapshot(), original);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.snapshot(), stretched);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.snapshot(), original);
}

#[test]
fn a_later_command_failure_rolls_back_mapping_revision_and_all_history_accounting() {
    let mut engine = engine();
    let before = engine.snapshot();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    let error = engine
        .apply_batch(vec![
            stretch(),
            Command::LayerTime {
                object: 999,
                action: LayerTimeAction::Reverse {},
            },
        ])
        .unwrap_err();
    assert!(matches!(
        error,
        Error::LayerTimeObject {
            object: 999,
            phase: "edit_lookup",
            ..
        }
    ));
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.history_diagnostics(), history);
}

#[test]
fn failed_real_project_publication_preserves_mapping_history_and_original_destination() {
    let mut engine = engine();
    let before = engine.snapshot();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("project.json");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("keep"), b"original").unwrap();
    let error = engine
        .apply_batch_saved(vec![stretch()], root.path())
        .unwrap_err();
    assert!(matches!(error, Error::Storage { .. }));
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.history_diagnostics(), history);
    assert_eq!(
        std::fs::read(destination.join("keep")).unwrap(),
        b"original"
    );
    assert!(!root.path().join("project.json.tmp").exists());
}

#[test]
fn cancelled_gesture_restores_the_full_unmapped_project_and_does_not_create_an_undo_entry() {
    let mut engine = engine();
    let before = engine.snapshot();
    let history = engine.history_diagnostics();
    engine.begin_gesture().unwrap();
    engine.apply(stretch()).unwrap();
    assert!(engine.project().layers[0].time_mapping.is_some());
    assert!(!engine.can_undo());
    engine.end_gesture(false).unwrap();
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.history_diagnostics(), history);
    assert!(!engine.undo().unwrap());
}

#[test]
fn repeating_the_same_duration_scale_does_not_publish_a_new_revision_or_history_entry() {
    let mut engine = engine();
    engine.apply(stretch()).unwrap();
    let before = engine.snapshot();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    engine
        .apply(Command::LayerTime {
            object: 7,
            action: LayerTimeAction::Stretch {
                stretch: TimeStretch::new(3, 2).unwrap(),
                anchor_frame: 53.,
                resize_clip: true,
            },
        })
        .unwrap();
    assert_eq!(engine.snapshot(), before);
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.history_diagnostics(), history);
}
