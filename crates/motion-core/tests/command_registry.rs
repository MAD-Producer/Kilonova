use motion_core::{CommandRegistry, Engine, Layer, Project};

fn engine() -> Engine {
    let mut project = Project::new(320, 180, 30, 120).unwrap();
    project.layers.push(Layer::solid(
        1,
        "Source",
        [80., 40.],
        [160., 90., 0.],
        [1.; 4],
    ));
    Engine::new(project).unwrap()
}
#[test]
fn duplicate_separate_and_split_share_one_atomic_undo_boundary() {
    let mut engine = engine();
    let original = engine.snapshot();
    let commands=CommandRegistry::parse(r#"[{"op":"duplicate","object":1},{"op":"separate_dimensions","object":2,"property":"position"},{"op":"split_layer_clip","object":2,"frame":60}]"#).unwrap();
    CommandRegistry::run(&mut engine, commands).unwrap();
    assert_eq!(engine.project().layers.len(), 3);
    assert!(engine
        .project()
        .layers
        .iter()
        .filter(|l| l.id != 1)
        .all(|l| l.transform.position.axes.is_some()));
    engine.undo().unwrap();
    assert_eq!(engine.snapshot(), original);
    assert!(!engine.can_undo());
    engine.redo().unwrap();
    assert_eq!(engine.project().layers.len(), 3);
}
#[test]
fn rejected_batch_restores_project_history_and_revision() {
    let mut engine = engine();
    let original = engine.snapshot();
    let revision = engine.revision();
    let commands = CommandRegistry::parse(
        r#"[{"op":"duplicate","object":1},{"op":"split_layer_clip","object":999,"frame":60}]"#,
    )
    .unwrap();
    assert!(CommandRegistry::run(&mut engine, commands).is_err());
    assert_eq!(engine.snapshot(), original);
    assert_eq!(engine.revision(), revision);
    assert!(!engine.can_undo());
    assert!(CommandRegistry::parse(r#"{"op":"duplicate","object":1,"typo":true}"#).is_err());
}
#[test]
fn command_directory_has_unique_machine_readable_schemas_and_no_internal_wrappers() {
    let catalogue = CommandRegistry::catalogue();
    let commands = catalogue["commands"].as_array().unwrap();
    let ids: std::collections::BTreeSet<_> =
        commands.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), commands.len());
    for id in [
        "duplicate",
        "separate_dimensions",
        "split_layer_clip",
        "parent",
        "effect",
        "set_component",
    ] {
        assert!(ids.contains(id));
    }
    assert!(!ids.contains("@composition"));
    for command in commands {
        assert_eq!(
            command["inputSchema"]["properties"]["op"]["const"],
            command["id"]
        );
    }
}
