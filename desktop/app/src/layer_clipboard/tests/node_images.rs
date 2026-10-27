use super::*;

#[test]
fn node_image_layer_clipboard_survives_removed_sources_cancel_and_repeated_paste() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let mut source_session = Session::new(
        project(source.path(), 1),
        source.path().into(),
        motion_desktop_media::platform(),
    )
    .unwrap();
    let mut editor = crate::node_editor::NodeEditor::new(crate::node_image_fixture::graph(
        crate::node_image_fixture::image(),
    ));
    editor.layer_target = Some(crate::node_layer::Target {
        root: source.path().into(),
        composition: source_session.engine.project().composition_id.clone(),
        object: 1,
        effect: 1,
        key: "clipboard-image".into(),
        label: "Image".into(),
        creating: true,
    });
    let snapshot = editor.snapshot();
    crate::node_layer::apply(&mut source_session, &snapshot, &snapshot.compile().unwrap()).unwrap();
    motion_core::storage::save(source.path(), source_session.engine.project()).unwrap();
    let records = source_session.engine.project().node_resources().unwrap();
    assert_eq!(records.len(), 2);
    drop(source_session);
    let source_engine = engine(source.path());
    project(target.path(), 0);
    let target_engine = engine(target.path());
    let frozen = freeze(&source_engine, vec![1], cache.path());
    drop(source_engine);
    for record in &records {
        fs::remove_file(source.path().join(&record.path)).unwrap();
    }
    let prepared = prepare(frozen.clone(), &target_engine).unwrap();
    assert_eq!(prepared.files.len(), 3); // Source layer image, node PNG, runtime.
    let staged: Vec<_> = records
        .iter()
        .map(|r| target.path().join(&r.path))
        .collect();
    assert!(staged.iter().all(|p| p.exists()));
    drop(prepared); // Cancellation owns exactly these newly staged files.
    assert!(staged.iter().all(|p| !p.exists()));
    let state = paste(
        prepare(frozen.clone(), &target_engine).unwrap(),
        &target_engine,
    );
    assert_eq!(state["clipboard_status"], "applied");
    let effect = &state["project"]["layers"][0]["effects"][0];
    assert_eq!(
        crate::node_layer::load_document(&effect["node_graph"], target.path()).unwrap(),
        snapshot.document
    );
    let prepared = prepare(frozen, &target_engine).unwrap();
    assert!(prepared.files.is_empty());
    let state = paste(prepared, &target_engine);
    assert_eq!(state["project"]["layers"].as_array().unwrap().len(), 2);
    let project: motion_core::Project = serde_json::from_value(state["project"].clone()).unwrap();
    assert_eq!(project.node_resources().unwrap(), records);
    motion_core::storage::validate_assets(target.path(), &project).unwrap();
}
