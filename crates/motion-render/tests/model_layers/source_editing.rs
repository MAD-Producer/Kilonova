//! Editing resolves only the required ancestry, using actual owned cameras.
use super::*;
use glam::{Mat4, Vec3};
use motion_core::{Command, Engine, ParentLink};
use motion_render::ModelSourceCatalog;
#[path = "../support/imported_camera.rs"]
mod fixture;

fn sampled(project: &Project, catalog: &ModelSourceCatalog) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene.set_model_source_catalog(catalog.clone());
    scene
        .sample(project, 30., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
fn source_camera_ancestry_converts_drag_vectors_and_keeps_unrelated_parenting_editable() {
    let root = tempfile::tempdir().unwrap();
    let mut project = fixture::project(root.path(), false);
    let mut rig = Layer::solid(2, "Camera rig", [1.; 2], [48., 32., 0.], [0.; 4]);
    rig.content = Content::Null;
    rig.three_d = true;
    rig.transform.rotation = motion_core::Track::constant([0., 0., 90.]);
    project.layers.push(rig);
    project.camera.parent = Some(ParentLink {
        object: Some(2),
        bind: Mat4::IDENTITY.to_cols_array_2d(),
    });
    let mut child = Layer::solid(3, "Camera child", [1.; 2], [48., 32., 4.], [1.; 4]);
    child.three_d = true;
    child.parent = Some(ParentLink {
        object: Some(0),
        bind: Mat4::IDENTITY.to_cols_array_2d(),
    });
    project.layers.push(child);
    project.layers.push(Layer::solid(
        4,
        "Independent layer",
        [4.; 2],
        [60., 40., 0.],
        [1.; 4],
    ));
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    let before = sampled(&project, &catalog);
    let delta = [7., 2., 0.];
    assert!(motion_core::scene_prefix_delta(&project, 3, 30., delta)
        .unwrap_err()
        .to_string()
        .contains("resolver"));
    let local =
        motion_core::scene_prefix_delta_with_cameras(&project, 3, 30., delta, &catalog).unwrap();
    let world = before
        .world_matrix(0)
        .unwrap()
        .transform_vector3(Vec3::new(local[0], -local[1], -local[2]));
    assert!(world.abs_diff_eq(Vec3::new(7., -2., 0.), 1e-6), "{world:?}");
    assert_eq!(
        motion_core::scene_prefix_delta(&project, 4, 30., delta).unwrap(),
        delta
    );
    let mut engine = Engine::new(project.clone()).unwrap();
    engine
        .apply(Command::Parent {
            object: 4,
            parent: Some(2),
            frame: 30,
        })
        .unwrap();
    let after = sampled(engine.project(), &catalog);
    assert!(before
        .world_matrix(4)
        .unwrap()
        .abs_diff_eq(after.world_matrix(4).unwrap(), 1e-6));
    assert_eq!(before.camera.view_projection, after.camera.view_projection);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &project);
    engine.redo().unwrap();
    engine
        .apply(Command::Parent {
            object: 4,
            parent: None,
            frame: 30,
        })
        .unwrap();
    assert!(before.world_matrix(4).unwrap().abs_diff_eq(
        sampled(engine.project(), &catalog).world_matrix(4).unwrap(),
        1e-6
    ));
    // Required camera failures are not hidden by limiting the ancestry traversal.
    let mut invalid = project;
    invalid.camera.imported.as_mut().unwrap().node = 99;
    let error = motion_core::scene_prefix_delta_with_cameras(&invalid, 3, 30., delta, &catalog)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("node=99") && error.contains("source=7"),
        "{error}"
    );
}

#[test]
fn source_camera_parent_transactions_preserve_world_history_and_reject_stale_sources() {
    let root = tempfile::tempdir().unwrap();
    let mut project = fixture::project(root.path(), false);
    let mut child = Layer::solid(2, "Editable child", [4.; 2], [60., 40., 0.], [1.; 4]);
    child.three_d = true;
    project.layers.push(child);
    let mut camera_layer = Layer::camera(5, "Source camera layer", 96, 64);
    camera_layer.content = Content::Camera {
        camera: Box::new(project.camera.clone()),
    };
    project.layers.push(camera_layer);
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    let source_path = root.path().join(&project.model_assets[0].path);
    let source_bytes = std::fs::read(&source_path).unwrap();
    std::fs::remove_file(&source_path).unwrap();
    let before = sampled(&project, &catalog).world_matrix(2).unwrap();
    let mut engine = Engine::new(project.clone()).unwrap();
    let command = Command::Parent {
        object: 2,
        parent: Some(0),
        frame: 30,
    };
    assert!(engine
        .apply(command.clone())
        .unwrap_err()
        .to_string()
        .contains("resolver"));
    assert_eq!(engine.project(), &project);
    assert_eq!(engine.revision(), 0);
    assert!(!engine.can_undo());
    engine
        .apply_batch_with_cameras_and_prepare(
            vec![Command::InComposition {
                composition: project.composition_id.clone(),
                command: Box::new(command),
            }],
            &catalog,
            |_| Ok(()),
        )
        .unwrap();
    let parented = engine.project().clone();
    assert!(before.abs_diff_eq(sampled(&parented, &catalog).world_matrix(2).unwrap(), 1e-6));
    assert_eq!(parented.layers[1].transform, project.layers[1].transform);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &project);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &parented);
    let revision = engine.revision();
    let failure = engine
        .apply_batch_with_cameras_and_prepare(
            vec![Command::Parent {
                object: 2,
                parent: Some(5),
                frame: 30,
            }],
            &catalog,
            |_| {
                Err(motion_core::Error::Invalid(
                    "controlled resource refusal".into(),
                ))
            },
        )
        .unwrap_err();
    assert!(failure.to_string().contains("controlled resource refusal"));
    assert_eq!(engine.project(), &parented);
    assert_eq!(engine.revision(), revision);
    engine
        .apply_batch_with_cameras_and_prepare(
            vec![Command::Parent {
                object: 2,
                parent: Some(5),
                frame: 30,
            }],
            &catalog,
            |_| Ok(()),
        )
        .unwrap();
    assert!(before.abs_diff_eq(
        sampled(engine.project(), &catalog).world_matrix(2).unwrap(),
        1e-6
    ));
    let layer_parented = engine.project().clone();
    engine
        .apply_batch_with_cameras_and_prepare(
            vec![Command::RemoveLayers {
                objects: vec![5],
                frame: 30,
            }],
            &catalog,
            |_| Ok(()),
        )
        .unwrap();
    assert!(before.abs_diff_eq(
        sampled(engine.project(), &catalog).world_matrix(2).unwrap(),
        1e-6
    ));
    assert!(engine.project().layers[1]
        .parent
        .as_ref()
        .and_then(|link| link.object)
        .is_none());
    engine.undo().unwrap();
    assert_eq!(engine.project(), &layer_parented);
    engine
        .apply_batch_with_cameras_and_prepare(
            vec![Command::ParentWithMode {
                object: 2,
                parent: None,
                frame: 30,
                mode: motion_core::ParentMode::PreserveLocal,
            }],
            &catalog,
            |_| Ok(()),
        )
        .unwrap();
    assert!(engine.project().layers[1].parent.is_none());
    assert_eq!(
        engine.project().layers[1].transform,
        project.layers[1].transform
    );
    engine.undo().unwrap();
    engine
        .apply_batch_with_cameras_and_prepare(
            vec![Command::Remove {
                object: 5,
                frame: 30,
            }],
            &catalog,
            |_| Ok(()),
        )
        .unwrap();
    assert!(before.abs_diff_eq(
        sampled(engine.project(), &catalog).world_matrix(2).unwrap(),
        1e-6
    ));
    assert!(motion_core::storage::save(root.path(), engine.project()).is_err());
    std::fs::write(&source_path, &source_bytes).unwrap();
    motion_core::storage::save(root.path(), engine.project()).unwrap();
    let reopened = motion_core::storage::load(root.path()).unwrap();
    assert!(before.abs_diff_eq(sampled(&reopened, &catalog).world_matrix(2).unwrap(), 1e-6));

    let mut changed = project.clone();
    changed.model_assets[0].sha256 = "0".repeat(64);
    let mut stale = Engine::new(changed.clone()).unwrap();
    let failure = stale
        .apply_batch_with_cameras_and_prepare(
            vec![Command::Parent {
                object: 2,
                parent: Some(0),
                frame: 30,
            }],
            &catalog,
            |_| panic!("stale source must fail before external preparation"),
        )
        .unwrap_err();
    assert!(
        failure.to_string().contains("source=7") && failure.to_string().contains("phase=binding")
    );
    assert_eq!(stale.project(), &changed);
    assert_eq!(stale.revision(), 0);
    assert!(!stale.can_undo());
}
