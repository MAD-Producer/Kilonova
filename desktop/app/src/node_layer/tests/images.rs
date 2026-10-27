use super::*;
use crate::node_image_fixture::{graph, image, png};
use std::sync::atomic::AtomicBool;

fn image_editor(editor: &mut node_editor::NodeEditor) -> node_editor::EditorSnapshot {
    let target = editor.layer_target.clone();
    editor.replace_document(graph(image()), None);
    editor.layer_target = target;
    editor.snapshot()
}
#[test]
fn node_images_and_last_valid_runtime_reopen_offline_with_small_draft_metadata() {
    let (folder, mut session, mut editor) = setup();
    let first = image_editor(&mut editor);
    let compiled = first.compile().unwrap();
    apply(&mut session, &first, &compiled).unwrap();
    let valid = session.engine.project().clone();
    let source = valid.layers[0].effects[0].node_graph.as_ref().unwrap();
    assert!(source["runtime"]["package_hex"].is_null());
    assert!(source["document"]["images"]
        .as_object()
        .unwrap()
        .values()
        .all(|v| v.get("png").is_none()));
    assert!(serde_json::to_vec(source).unwrap().len() < 16 * 1024);
    assert_eq!(valid.node_resources().unwrap().len(), 2);
    assert_eq!(
        load_document(source, &session.root).unwrap(),
        first.document
    );
    let hash = valid.layers[0].effects[0].hash.clone();

    // An unfinished draft retains both its newly imported image and the last
    // valid runtime's original image, independently of compiler reachability.
    let other = motion_graph::GraphImage::from_png(png(&[12, 34, 56, 255], 1, 1)).unwrap();
    let mut draft = first.document.clone();
    draft.images.insert(other.id().into(), other.clone());
    draft.nodes.push(Node::new(
        "unused_image",
        NodeOp::ImageAsset {
            image: other.id().into(),
        },
    ));
    draft.connections.retain(|c| c.to.node != "output");
    editor.replace_document(draft, None);
    editor.layer_target = first.layer_target.clone();
    let broken = editor.snapshot();
    assert!(broken.compile().is_err());
    persist(&mut session, &broken).unwrap();
    let project = session.engine.project().clone();
    assert_eq!(project.node_resources().unwrap().len(), 3);
    assert_eq!(project.layers[0].effects[0].hash, hash);
    let archive = folder.path().join("with-images.msproj");
    motion_core::storage::export_package(&session.root, &project, &archive).unwrap();
    let offline = tempfile::tempdir().unwrap();
    let root = offline.path().join("project");
    let restored = motion_core::storage::import_package(&archive, &root).unwrap();
    // This parent has no installed plugin cache to mask missing payload files.
    assert!(!offline.path().join("plugins").exists());
    let reopened =
        motion_host::Session::new(restored, root.clone(), motion_desktop_media::platform())
            .unwrap();
    let effect = &reopened.engine.project().layers[0].effects[0];
    let package = reopened
        .effects
        .registry
        .resolve(&effect.plugin, &effect.version, &effect.hash)
        .unwrap();
    assert!(package
        .files
        .values()
        .any(|bytes| bytes.as_slice() == image().png()));
    assert_eq!(
        load_document(effect.node_graph.as_ref().unwrap(), &root).unwrap(),
        broken.document
    );
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &valid);
    session.engine.redo().unwrap();
    assert_eq!(session.engine.project(), &project);
}

#[test]
fn node_image_animation_preset_restores_runtime_and_resources_in_a_fresh_project() {
    let (folder, mut source, mut editor) = setup();
    let snapshot = image_editor(&mut editor);
    apply(&mut source, &snapshot, &snapshot.compile().unwrap()).unwrap();
    let preset = motion_core::AnimationPreset::capture(
        source.engine.project(),
        1,
        "图像节点".into(),
        "Node".into(),
        &[],
        &[1],
    )
    .unwrap();
    assert_eq!(preset.node_resources().unwrap().len(), 2);
    let file = folder.path().join("image.msanim");
    let cancel = AtomicBool::new(false);
    motion_core::storage::save_animation_preset(&source.root, &preset, &file, &cancel).unwrap();
    let destination = tempfile::tempdir().unwrap();
    let root = destination.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let mut project = motion_core::Project::new(128, 64, 30, 120).unwrap();
    project.layers.push(motion_core::Layer::solid(
        9,
        "target",
        [128., 64.],
        [64., 32., 0.],
        [1.; 4],
    ));
    let mut target =
        motion_host::Session::new(project, root.clone(), motion_desktop_media::platform()).unwrap();
    let before = target.engine.project().clone();
    let stage = motion_core::storage::stage_animation_preset(&file, &root, &cancel).unwrap();
    target
        .apply_command_batch(vec![Command::ApplyAnimationPreset {
            preset: Box::new(stage.preset.clone()),
            objects: vec![9],
            at_frame: 0,
        }])
        .unwrap();
    stage.keep_resources();
    let effect = &target.engine.project().layers[0].effects[0];
    assert!(target
        .effects
        .registry
        .resolve(&effect.plugin, &effect.version, &effect.hash)
        .is_ok());
    assert_eq!(
        load_document(effect.node_graph.as_ref().unwrap(), &root).unwrap(),
        snapshot.document
    );
    target.engine.undo().unwrap();
    assert_eq!(target.engine.project(), &before);
    target.engine.redo().unwrap();
    motion_core::storage::validate_assets(&root, target.engine.project()).unwrap();
}

#[test]
fn refused_node_image_update_keeps_model_and_existing_files_and_leaves_no_new_resource() {
    let (_folder, mut session, mut editor) = setup();
    let first = image_editor(&mut editor);
    apply(&mut session, &first, &first.compile().unwrap()).unwrap();
    let before = session.engine.project().clone();
    let directory = session.root.join("assets");
    let entries = || {
        let mut names: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        names
    };
    let files = entries();
    // A real target guard failure occurs before staging any newly imported PNG.
    session
        .engine
        .apply(Command::Flags {
            object: 1,
            visible: true,
            locked: true,
        })
        .unwrap();
    let locked = session.engine.project().clone();
    let next = motion_graph::GraphImage::from_png(png(&[90, 80, 70, 255], 1, 1)).unwrap();
    let mut document = first.document.clone();
    document.images.insert(next.id().into(), next.clone());
    document.nodes.push(Node::new(
        "new_image",
        NodeOp::ImageAsset {
            image: next.id().into(),
        },
    ));
    editor.replace_document(document, None);
    editor.layer_target = first.layer_target;
    assert!(persist(&mut session, &editor.snapshot())
        .unwrap_err()
        .to_string()
        .contains("locked"));
    assert_eq!(session.engine.project(), &locked);
    assert_eq!(entries(), files);
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &before);
}
