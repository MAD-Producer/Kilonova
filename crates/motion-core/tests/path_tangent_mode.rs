use motion_core::{
    masks::{LayerMask, MaskAction},
    vector::{
        groups::VectorGroup, PathNode, ShapeKind, TangentMode, VectorAction, VectorContent,
        VectorPath, VectorSource,
    },
    Command, Content, Ease, Engine, Keyframe, Layer, Project, Track,
};

fn fixture() -> Engine {
    let mut project = Project::new(128, 128, 30, 30).unwrap();
    let path = VectorPath {
        id: 42,
        closed: true,
        tangents: TangentMode::Manual,
        nodes: [[10., 10.], [90., 10.], [80., 80.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let mut geometry = Track::constant([p[0], p[1], 1., 2., 3., 4.]);
                geometry.keys = [0, 20]
                    .into_iter()
                    .map(|frame| Keyframe {
                        frame,
                        value: [p[0] + frame as f32, p[1], 1., 2., 3., 4.],
                        ease: Ease::InOut,
                        curve: None,
                        spatial: None,
                    })
                    .collect();
                PathNode {
                    id: i as u64 + 1,
                    geometry,
                }
            })
            .collect(),
    };
    for object in [7, 8, 9] {
        let mut layer = Layer::solid(object, "Path tangents", [128.; 2], [64., 64., 0.], [1.; 4]);
        if object == 9 {
            layer.masks.push(LayerMask::new(6, path.clone()));
        } else {
            let mut vector = VectorContent::shape(ShapeKind::Rectangle);
            vector.source = VectorSource::Paths {
                paths: vec![path.clone()],
            };
            layer.content = Content::Vector {
                vector: if object == 8 {
                    VectorGroup::wrap(vector, [128.; 2])
                } else {
                    vector
                },
            };
        }
        project.layers.push(layer);
    }
    Engine::new(project).unwrap()
}
fn command(object: u64, path: u64, mode: TangentMode) -> Command {
    if object == 9 {
        Command::Mask {
            object,
            action: MaskAction::SetPathTangents {
                mask: 6,
                path,
                mode,
            },
        }
    } else {
        Command::Vector {
            object,
            action: VectorAction::SetPathTangents {
                item: (object == 8).then_some(2),
                path,
                mode,
            },
        }
    }
}

#[test]
fn flat_group_and_mask_modes_save_reopen_and_undo_without_rewriting_tracks() {
    for object in [7, 8, 9] {
        let mut engine = fixture();
        let before = engine.project().clone();
        let json = serde_json::to_string(&command(object, 42, TangentMode::Automatic)).unwrap();
        engine.apply(serde_json::from_str(&json).unwrap()).unwrap();
        let after = engine.project().clone();
        assert_ne!(after, before);
        let folder = tempfile::tempdir().unwrap();
        motion_core::storage::save(folder.path(), &after).unwrap();
        assert_eq!(motion_core::storage::load(folder.path()).unwrap(), after);
        assert!(engine.undo().unwrap());
        assert_eq!(*engine.project(), before);
        assert!(engine.redo().unwrap());
        assert_eq!(*engine.project(), after);
        engine
            .apply(command(object, 42, TangentMode::Manual))
            .unwrap();
        assert_eq!(
            *engine.project(),
            before,
            "restoring manual mode preserves every original handle, key and node ID"
        );
    }
}

#[test]
fn missing_and_locked_targets_preserve_project_history_and_original_cause() {
    use std::error::Error;
    for object in [7, 8, 9] {
        for locked in [false, true] {
            let mut project = fixture().project().clone();
            project
                .layers
                .iter_mut()
                .find(|l| l.id == object)
                .unwrap()
                .locked = locked;
            let mut engine = Engine::new(project).unwrap();
            let before = engine.project().clone();
            let revision = engine.revision();
            let history = engine.history_diagnostics();
            let error = engine
                .apply(command(
                    object,
                    if locked { 42 } else { 404 },
                    TangentMode::Automatic,
                ))
                .unwrap_err();
            assert!(error.to_string().contains("operation=set_tangents"));
            assert!(error.source().is_some());
            assert_eq!(*engine.project(), before);
            assert_eq!(engine.revision(), revision);
            assert_eq!(engine.history_diagnostics(), history);
        }
    }
}

#[test]
fn conversion_retains_auto_mode_and_rejects_a_nonuniform_group_basis_atomically() {
    use motion_core::{vector::groups::ParameterValue, PathTarget};
    let mut engine = fixture();
    engine
        .apply(command(8, 42, TangentMode::Automatic))
        .unwrap();
    let before = engine.project().clone();
    let convert = || Command::CopyPathAs {
        target: PathTarget::Vector {
            object: 8,
            item: Some(2),
        },
        path: 42,
    };
    engine.apply(convert()).unwrap();
    let layer = engine.project().layers.iter().find(|l| l.id == 8).unwrap();
    assert_eq!(layer.masks[0].path.tangents, TangentMode::Automatic);
    assert!(engine.undo().unwrap());
    assert_eq!(*engine.project(), before);
    engine
        .apply(Command::Vector {
            object: 8,
            action: VectorAction::SetGroupParameter {
                item: 1,
                parameter: "scale".into(),
                frame: 0,
                value: ParameterValue::Vector([200., 100.]),
                animated: Some(false),
            },
        })
        .unwrap();
    let before = engine.project().clone();
    let history = engine.history_diagnostics();
    let error = engine.apply(convert()).unwrap_err().to_string();
    assert!(error.contains("nonuniform or skewed"), "{error}");
    assert_eq!(*engine.project(), before);
    assert_eq!(engine.history_diagnostics(), history);
}
