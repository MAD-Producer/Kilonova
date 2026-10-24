use motion_core::{Command, Engine, Layer, ParentLink, Project};
use motion_render::Scene;

fn fixture() -> Project {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    p.layers.push(Layer::solid(
        1,
        "Background",
        [200.; 2],
        [320., 180., 0.],
        [0.2, 0.4, 0.8, 1.],
    ));
    let mut child = Layer::solid(2, "Child", [80.; 2], [20., 0., 0.], [1., 0.5, 0.2, 1.]);
    child.parent = Some(ParentLink {
        object: Some(1),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    p.layers.push(child);
    p
}

#[test]
fn shy_metadata_is_undoable_persistent_and_cannot_change_sampled_output() {
    let before = fixture();
    let mut engine = Engine::new(before.clone()).unwrap();
    engine
        .apply_batch(vec![
            Command::SetLayerShy {
                object: 1,
                shy: true,
            },
            Command::SetLayerShy {
                object: 2,
                shy: true,
            },
        ])
        .unwrap();
    assert!(engine.project().layers.iter().all(|l| l.shy && l.visible));
    let after = engine.snapshot();
    for frame in [0., 29.5, 119.] {
        let sample = |project: &Project| {
            let mut scene = Scene::new(project).unwrap();
            scene
                .sample(project, frame, None, &motion_core::ExpressionEvaluator)
                .unwrap();
            scene
        };
        let a = sample(&before);
        let b = sample(&after);
        assert_eq!(a.layers.len(), b.layers.len());
        for (a, b) in a.layers.iter().zip(&b.layers) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.model, b.model);
            assert_eq!(a.color, b.color);
            assert_eq!(a.opacity, b.opacity);
            assert_eq!(a.composite_visible, b.composite_visible);
        }
    }
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), engine.project()).unwrap();
    assert_eq!(motion_core::storage::load(root.path()).unwrap(), after);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &after);
    let mut absent = serde_json::to_value(&after).unwrap();
    absent["layers"][0].as_object_mut().unwrap().remove("shy");
    assert!(serde_json::from_value::<Project>(absent)
        .unwrap_err()
        .to_string()
        .contains("shy"));
}

#[test]
fn locked_missing_and_bad_batches_preserve_project_revision_and_history() {
    let mut p = fixture();
    p.layers[1].locked = true;
    let mut engine = Engine::new(p.clone()).unwrap();
    let revision = engine.revision();
    for object in [2, 999] {
        assert!(engine
            .apply_batch(vec![
                Command::SetLayerShy {
                    object: 1,
                    shy: true
                },
                Command::SetLayerShy { object, shy: true }
            ])
            .is_err());
        assert_eq!(engine.project(), &p);
        assert_eq!(engine.revision(), revision);
        assert!(!engine.can_undo());
    }
    let mut old = p;
    old.version = motion_core::CURRENT_PROJECT_VERSION - 1;
    assert!(old
        .validate()
        .unwrap_err()
        .to_string()
        .contains("unsupported project format"));
}
