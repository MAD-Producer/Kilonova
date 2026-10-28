use motion_core::{
    Asset, Command, CommandRegistry, Content, Engine, Layer, LayerClipboard, Project,
};
use motion_model::depth::*;
use serde_json::json;

fn saved() -> SavedDepthCalibration {
    let resource = DepthResource::new(
        DepthDescriptor {
            size: [2, 1],
            source: DepthSource {
                asset: 7,
                revision: 77,
                pts: DepthTime {
                    ticks: 0,
                    timescale: 1,
                },
            },
            geometry: DepthGeometry {
                source_size: [2, 1],
                crop: [0, 0, 2, 1],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "owned-test".into(),
                revision: "r1".into(),
                sha256: [7; 32],
                processing: "owned-v1".into(),
            },
            measure: DepthMeasure::Relative {
                near: NearDirection::Increasing,
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        },
        vec![0.25, 2.],
        None,
        None,
    )
    .unwrap();
    SavedDepthCalibration::new(
        &resource,
        PreparedDepthInput {
            rgb_size: [2, 1],
            rgb_sha256: [42; 32],
        },
        [
            DepthAnchor {
                value: 0.25,
                camera_z_meters: 8.,
            },
            DepthAnchor {
                value: 2.,
                camera_z_meters: 4.,
            },
        ],
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
        20.,
    )
    .unwrap()
}
fn project() -> Project {
    let mut p = Project::new(32, 32, 30, 60).unwrap();
    p.assets.push(Asset {
        id: 7,
        path: "assets/owned.ppm".into(),
        width: 2,
        height: 1,
    });
    let mut layer = Layer::solid(1, "Owned source", [2., 1.], [16., 16., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 7 };
    p.layers.push(layer);
    p
}

#[test]
fn calibration_command_roundtrips_storage_history_and_required_nullable_schema() {
    let initial = project();
    let mut engine = Engine::new(initial.clone()).unwrap();
    let request = json!({"op":"depth_calibration","object":1,"calibration":saved()});
    CommandRegistry::run(
        &mut engine,
        CommandRegistry::parse(&request.to_string()).unwrap(),
    )
    .unwrap();
    let stored = engine.project().clone();
    assert_eq!(stored.layers[0].depth_calibration, Some(saved()));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &initial);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &stored);
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    // Independently authored PPM: storage metadata tests do not claim GPU/image decoding.
    std::fs::write(
        root.path().join("assets/owned.ppm"),
        b"P6\n2 1\n255\n\x01\x02\x03\x04\x05\x06",
    )
    .unwrap();
    motion_core::storage::save(root.path(), &stored).unwrap();
    assert_eq!(motion_core::storage::load(root.path()).unwrap(), stored);
    assert!(CommandRegistry::parse(r#"{"op":"depth_calibration","object":1}"#).is_err());
    let catalog = CommandRegistry::catalogue();
    let descriptor = catalog["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == "depth_calibration")
        .unwrap();
    assert!(descriptor["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "calibration"));
    CommandRegistry::run(
        &mut engine,
        CommandRegistry::parse(r#"{"op":"depth_calibration","object":1,"calibration":null}"#)
            .unwrap(),
    )
    .unwrap();
    assert!(engine.project().layers[0].depth_calibration.is_none());
    engine.undo().unwrap();
    assert_eq!(engine.project(), &stored);
}

#[test]
fn calibration_source_validation_is_atomic_and_clipboard_remaps_the_same_owned_pixels() {
    let mut initial = project();
    initial.layers[0].locked = true;
    let mut engine = Engine::new(initial.clone()).unwrap();
    assert!(engine
        .apply(Command::DepthCalibration {
            object: 1,
            calibration: Some(saved())
        })
        .is_err());
    assert_eq!(engine.project(), &initial);
    assert!(!engine.can_undo());
    initial.layers[0].locked = false;
    engine = Engine::new(initial.clone()).unwrap();
    let mut wrong = saved();
    wrong.remap_asset(99);
    assert!(engine
        .apply(Command::DepthCalibration {
            object: 1,
            calibration: Some(wrong)
        })
        .is_err());
    assert_eq!(engine.project(), &initial);
    assert_eq!(engine.revision(), 0);
    engine
        .apply(Command::DepthCalibration {
            object: 1,
            calibration: Some(saved()),
        })
        .unwrap();
    let clipboard = LayerClipboard::capture(engine.project(), vec![1]).unwrap();
    let mut destination = Project::new(32, 32, 30, 60).unwrap();
    destination.assets.push(Asset {
        id: 100,
        path: "assets/unrelated.ppm".into(),
        width: 2,
        height: 1,
    });
    let before = destination.clone();
    let mut target = Engine::new(destination).unwrap();
    target
        .apply(Command::PasteLayers {
            clipboard: Box::new(clipboard),
            at_frame: None,
            keep_external_references: false,
        })
        .unwrap();
    let calibration = target.project().layers[0]
        .depth_calibration
        .as_ref()
        .unwrap();
    assert_eq!(calibration.asset(), 101);
    assert_eq!(calibration.input(), saved().input());
    assert!(matches!(
        target.project().layers[0].content,
        Content::Image { asset: 101 }
    ));
    target.project().validate().unwrap();
    target.undo().unwrap();
    assert_eq!(target.project(), &before);
    target.redo().unwrap();
    target.project().validate().unwrap();
}
