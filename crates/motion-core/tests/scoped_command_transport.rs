use motion_core::{
    parse_commands, Command, CompositionAction, CompositionSettings, Engine, Layer, Project,
};

fn engine() -> (Engine, String) {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project
        .layers
        .push(Layer::solid(1, "Main", [8.; 2], [32., 32., 0.], [1.; 4]));
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(Command::Composition {
            action: CompositionAction::Create {
                settings: CompositionSettings {
                    name: "Child".into(),
                    width: 64,
                    height: 64,
                    fps: 30,
                    frames: 120,
                    timing: "preserve_frames".into(),
                    shorten: "reject".into(),
                },
            },
        })
        .unwrap();
    let child = engine.project().compositions[0].id.clone();
    engine
        .apply(Command::InComposition {
            composition: child.clone(),
            command: Box::new(Command::Add {
                layer: Layer::solid(1, "Child", [8.; 2], [32., 32., 0.], [1.; 4]),
            }),
        })
        .unwrap();
    (engine, child)
}
#[test]
fn typed_menu_actions_roundtrip_through_transport_without_changing_other_compositions() {
    let (mut engine, child) = engine();
    let before = engine.project().clone();
    let action = Command::InComposition {
        composition: child.clone(),
        command: Box::new(Command::Rename {
            object: 1,
            name: "Edited child".into(),
        }),
    };
    let wire = serde_json::to_string(&action).unwrap();
    engine.apply_batch(parse_commands(&wire).unwrap()).unwrap();
    assert_eq!(engine.project().layers[0].name, "Main");
    assert_eq!(
        engine.project().composition(&child).unwrap().layers[0].name,
        "Edited child"
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}
#[test]
fn flat_and_typed_scopes_match_and_malformed_nested_scope_is_rejected() {
    let (mut engine, child) = engine();
    let before = engine.project().clone();
    let flat = serde_json::json!({"op":"rename","composition":child,"object":1,"name":"Flat"});
    engine
        .apply_batch(parse_commands(&flat.to_string()).unwrap())
        .unwrap();
    let flat_after = engine.project().clone();
    engine.undo().unwrap();
    let typed = Command::InComposition {
        composition: child,
        command: Box::new(Command::Rename {
            object: 1,
            name: "Flat".into(),
        }),
    };
    engine
        .apply_batch(parse_commands(&serde_json::to_string(&typed).unwrap()).unwrap())
        .unwrap();
    assert_eq!(engine.project(), &flat_after);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    for invalid in [
        serde_json::json!({"op":"in_composition","command":{"op":"delete","object":1}}),
        serde_json::json!({"op":"in_composition","composition":"comp-main",
            "command":{"op":"in_composition","composition":"../../bad","command":{"op":"delete","object":1}}}),
        serde_json::json!({"op":"in_composition","composition":"comp-main","extra":true,
            "command":{"op":"delete","object":1}}),
    ] {
        assert!(parse_commands(&invalid.to_string()).is_err());
    }
}
