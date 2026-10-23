use motion_core::{parse_commands, storage, Engine, Project};

#[test]
fn failed_saved_edit_preserves_disk_revision_and_history() {
    let mut engine = Engine::new(Project::demo().unwrap()).unwrap();
    engine
        .apply_batch(
            parse_commands(
                r#"[
        {"op":"animate","object":2,"property":"position","frame":0,"enabled":true},
        {"op":"set_vector","object":2,"property":"position","frame":60,"value":[640,960,0]}
    ]"#,
            )
            .unwrap(),
        )
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    storage::save(directory.path(), engine.project()).unwrap();
    let path = directory.path().join("project.json");
    let bytes = std::fs::read(&path).unwrap();
    let before = engine.snapshot();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    for op in ["move_key", "copy_key"] {
        let commands = parse_commands(&format!(
            r#"[
            {{"op":"rename","object":2,"name":"must roll back"}},
            {{"op":"{op}","object":2,"property":"position","from":0,"to":60}}
        ]"#
        ))
        .unwrap();
        let error = engine
            .apply_batch_saved(commands, directory.path())
            .unwrap_err();
        assert!(error.to_string().contains("occupied"), "{error}");
        assert_eq!(engine.project(), &before);
        assert_eq!(engine.revision(), revision);
        assert_eq!(engine.history_diagnostics(), history);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(storage::load(directory.path()).unwrap(), before);
    }
}
