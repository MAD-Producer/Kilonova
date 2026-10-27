use motion_core::{
    storage, Command, Content, Engine, Layer, LayerClipboard, ModelAsset, ModelClip, Project,
    Property,
};

fn project() -> Project {
    let mut project = Project::new(96, 64, 30, 90).unwrap();
    project.model_assets.push(ModelAsset {
        id: 7,
        path: "assets/models/owned.mscn".into(),
        bytes: 4,
        sha256: "a".repeat(64),
    });
    let mut layer = Layer::solid(1, "Owned model", [40., 40.], [48., 32., 0.], [1.; 4]);
    layer.three_d = true;
    layer.content = Content::Model {
        model: ModelClip {
            asset: 7,
            scene: 0,
            pixels_per_metre: 20.,
            pivot: [0.; 3],
            samples: 4,
        },
    };
    layer.transform.rotation.set_animated(0, true).unwrap();
    project.layers.push(layer);
    project.validate().unwrap();
    project
}

#[test]
fn model_validation_rejects_missing_sources_flat_mode_and_invalid_units() {
    let original = project();
    let mut bad = original.clone();
    bad.model_assets.clear();
    assert!(bad.validate().is_err());
    let mut bad = original.clone();
    bad.layers[0].three_d = false;
    assert!(bad.validate().is_err());
    let mut bad = original.clone();
    bad.model_assets[0].path = "assets/models/../../outside.mscn".into();
    assert!(bad.validate().is_err());
    let mut bad = original.clone();
    bad.model_assets[0].sha256 = "G".repeat(64);
    assert!(bad.validate().is_err());
    let mut bad = original.clone();
    if let Content::Model { model } = &mut bad.layers[0].content {
        model.pixels_per_metre = f32::NAN;
    }
    assert!(bad.validate().is_err());
    let mut json = serde_json::to_value(&original).unwrap();
    json.as_object_mut().unwrap().remove("model_assets");
    assert!(
        serde_json::from_value::<Project>(json).is_err(),
        "managed models table is required in the current schema"
    );
}

#[test]
fn model_keyframes_use_normal_edit_history_and_survive_save_and_package() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    std::fs::create_dir_all(root.join("assets/models")).unwrap();
    // Storage treats immutable model bytes as opaque; the renderer separately
    // verifies their digest and frozen format before preparing a catalog.
    std::fs::write(root.join("assets/models/owned.mscn"), b"test").unwrap();
    let mut engine = Engine::new(project()).unwrap();
    engine
        .apply(Command::SetVector {
            object: 1,
            property: Property::Rotation,
            frame: 60,
            value: [10., 240., 0.],
        })
        .unwrap();
    let edited = engine.snapshot();
    assert_eq!(
        edited.layers[0].transform.rotation.sample(60.).unwrap(),
        [10., 240., 0.]
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &project());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &edited);
    storage::save(&root, engine.project()).unwrap();
    assert_eq!(storage::load(&root).unwrap(), edited);
    let package = temp.path().join("model.motion");
    storage::export_package(&root, engine.project(), &package).unwrap();
    let restored = temp.path().join("restored");
    assert_eq!(
        storage::import_package(&package, &restored).unwrap(),
        edited
    );
    assert_eq!(
        std::fs::read(restored.join("assets/models/owned.mscn")).unwrap(),
        b"test"
    );
    std::fs::remove_file(restored.join("assets/models/owned.mscn")).unwrap();
    assert!(storage::load(&restored).is_err());
}

#[test]
fn clipboard_retains_model_dependency_and_remaps_its_shared_asset_id() {
    let mut original = project();
    original.layers[0]
        .transform
        .rotation
        .set_at(60, [0., 180., 0.])
        .unwrap();
    let clipboard = LayerClipboard::capture(&original, vec![1]).unwrap();
    let resolved = clipboard.resolve().unwrap();
    assert_eq!(resolved.model_assets.len(), 1);
    assert!(resolved.assets.is_empty());
    let mut target = Project::new(96, 64, 30, 90).unwrap();
    target.assets.push(motion_core::Asset {
        id: 100,
        path: "assets/image.png".into(),
        width: 1,
        height: 1,
    });
    let mut engine = Engine::new(target).unwrap();
    engine
        .apply(Command::PasteLayers {
            clipboard: Box::new(clipboard),
            at_frame: None,
            keep_external_references: false,
        })
        .unwrap();
    let target = engine.project();
    assert_eq!(target.model_assets.len(), 1);
    assert_eq!(target.model_assets[0].id, 101);
    let Content::Model { model } = &target.layers[0].content else {
        panic!("pasted model type lost")
    };
    assert_eq!(model.asset, 101);
    assert_eq!(
        target.layers[0].transform.rotation,
        original.layers[0].transform.rotation
    );
}
