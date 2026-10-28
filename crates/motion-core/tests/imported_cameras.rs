use motion_core::{Command, CommandRegistry, Engine, ImportedCamera, Layer, ModelAsset, Project};
use serde_json::json;

fn project() -> Project {
    let mut project = Project::new(96, 64, 30, 61).unwrap();
    project.camera.created = true;
    project.model_assets.push(ModelAsset {
        id: 7,
        path: "assets/models/camera.mscn".into(),
        bytes: 1,
        sha256: "0".repeat(64),
    });
    project
        .layers
        .push(Layer::camera(1, "Camera instance", 96, 64));
    project.layers.push(Layer::solid(
        2,
        "Rollback control",
        [1.; 2],
        [0.; 3],
        [1.; 4],
    ));
    project
}
fn binding() -> ImportedCamera {
    ImportedCamera {
        asset: 7,
        scene: 0,
        node: 1,
        pixels_per_metre: 20.,
        pivot: [0.; 3],
        animation: None,
    }
}
#[test]
fn imported_camera_command_schema_roundtrip_and_undo_are_typed() {
    let mut engine = Engine::new(project()).unwrap();
    let payload = json!({"op":"camera_imported","object":1,"binding":binding()});
    let parsed = CommandRegistry::parse(&payload.to_string()).unwrap();
    let Command::InComposition {
        composition,
        command,
    } = &parsed[0]
    else {
        panic!("composition routing missing");
    };
    assert_eq!(composition, "comp-main");
    assert_eq!(CommandRegistry::id(command), "camera_imported");
    CommandRegistry::run(&mut engine, parsed).unwrap();
    assert_eq!(
        engine.project().camera_for(1).unwrap().imported,
        Some(binding())
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project().camera_for(1).unwrap().imported, None);
    assert!(engine.redo().unwrap());
    let restored: Project =
        serde_json::from_value(serde_json::to_value(engine.project()).unwrap()).unwrap();
    assert_eq!(&restored, engine.project());
    let catalogue = CommandRegistry::catalogue();
    let schema = &catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "camera_imported")
        .unwrap()["inputSchema"];
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("binding")));
    let schema = &schema["properties"]["binding"];
    assert_eq!(schema["anyOf"][1]["type"], "null");
    assert_eq!(schema["anyOf"][0]["properties"]["asset"]["minimum"], 1);
    let clear =
        CommandRegistry::parse(r#"{"op":"camera_imported","object":1,"binding":null}"#).unwrap();
    assert!(CommandRegistry::parse(r#"{"op":"camera_imported","object":1}"#).is_err());
    engine.apply_batch(clear).unwrap();
    assert_eq!(engine.project().camera_for(1).unwrap().imported, None);
}

#[test]
fn imported_camera_invalid_asset_binding_lock_and_batch_preserve_history() {
    for kind in 0..4 {
        let mut project = project();
        if kind == 0 {
            project.layers[0].locked = true;
        }
        let mut engine = Engine::new(project.clone()).unwrap();
        let mut binding = binding();
        if kind == 1 {
            binding.asset = 99;
        }
        if kind == 2 {
            binding.pixels_per_metre = 0.;
        }
        let object = if kind == 3 { 99 } else { 1 };
        let history = engine.history_diagnostics();
        let error = engine
            .apply_batch(vec![
                Command::Rename {
                    object: 2,
                    name: "Must roll back".into(),
                },
                Command::CameraImported {
                    object,
                    binding: Some(binding),
                },
            ])
            .unwrap_err();
        assert!(!error.to_string().is_empty());
        assert_eq!(engine.project(), &project);
        assert_eq!(engine.history_diagnostics(), history);
        assert_eq!(engine.revision(), 0);
    }
}

#[test]
fn imported_camera_clipboard_retains_and_remaps_layer_and_nested_camera_sources() {
    for nested in [false, true] {
        let mut source = project();
        let selected = if nested {
            let mut camera = source.camera.clone();
            camera.imported = Some(binding());
            source
                .compositions
                .push(motion_core::composition::Composition {
                    id: "comp-camera".into(),
                    name: "Camera source only".into(),
                    width: 96,
                    height: 64,
                    fps: 30,
                    frames: 61,
                    work_area: motion_core::WorkArea::full(61),
                    guides: vec![],
                    background: [0.; 4],
                    camera,
                    layers: vec![],
                    expressions: vec![],
                });
            let mut reference =
                Layer::solid(3, "Nested camera", [96., 64.], [48., 32., 0.], [1.; 4]);
            reference.content = motion_core::Content::Composition {
                clip: motion_core::composition::CompositionClip::new("comp-camera".into()),
            };
            source.layers.push(reference);
            3
        } else {
            source.camera_for_mut(1).unwrap().imported = Some(binding());
            1
        };
        let clipboard = motion_core::LayerClipboard::capture(&source, vec![selected]).unwrap();
        assert_eq!(clipboard.resolve().unwrap().model_assets.len(), 1);
        let mut target = Project::new(96, 64, 30, 61).unwrap();
        target.model_assets.push(ModelAsset {
            id: 100,
            path: "assets/models/unrelated.mscn".into(),
            bytes: 1,
            sha256: "f".repeat(64),
        });
        let before = target.clone();
        let mut engine = Engine::new(target).unwrap();
        engine
            .apply(Command::PasteLayers {
                clipboard: Box::new(clipboard),
                at_frame: None,
                keep_external_references: false,
            })
            .unwrap();
        let selected_camera = if nested {
            &engine.project().compositions[0].camera
        } else {
            engine.project().layers[0].camera_data().unwrap()
        };
        assert_eq!(selected_camera.imported.as_ref().unwrap().asset, 101);
        assert_eq!(engine.project().model_assets.len(), 2);
        assert_eq!(engine.project().model_assets[1].id, 101);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        engine.redo().unwrap();
        engine.project().validate().unwrap();
    }
}
