use motion_core::{Command, Engine, Project};
use motion_effects::image_contract::ChannelPrecision;

#[test]
fn project_precision_save_reopen_history_and_failed_batches_preserve_the_complete_project() {
    let original = Project::new(64, 64, 30, 60).unwrap();
    let mut engine = Engine::new(original.clone()).unwrap();
    for precision in [
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let before = engine.snapshot();
        let command = serde_json::json!({"op":"set_working_precision", "precision":precision});
        engine
            .apply_batch(motion_core::parse_commands(&command.to_string()).unwrap())
            .unwrap();
        assert_eq!(engine.project().working_precision, precision);
        let mut expected = before.clone();
        expected.working_precision = precision;
        assert_eq!(engine.snapshot(), expected);
        let folder = tempfile::tempdir().unwrap();
        motion_core::storage::save(folder.path(), engine.project()).unwrap();
        assert_eq!(motion_core::storage::load(folder.path()).unwrap(), expected);
        assert!(engine.undo().unwrap());
        assert_eq!(engine.snapshot(), before);
        assert!(engine.redo().unwrap());
        assert_eq!(engine.snapshot(), expected);
        let revision = engine.revision();
        let error = engine
            .apply_batch(vec![
                Command::SetWorkingPrecision {
                    precision: ChannelPrecision::Unorm8,
                },
                Command::SetWorkArea {
                    in_frame: 60,
                    out_frame: 60,
                },
            ])
            .unwrap_err();
        assert!(error.to_string().contains("invalid work area"));
        assert_eq!(engine.snapshot(), expected);
        assert_eq!(engine.revision(), revision);
    }
}

#[test]
fn composition_scopes_share_the_document_precision_and_publish_a_strict_command_schema() {
    use motion_core::{CompositionAction, CompositionSettings};
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let created = project
        .edit_composition(CompositionAction::Create {
            settings: CompositionSettings {
                name: "Child".into(),
                width: 32,
                height: 32,
                fps: 24,
                frames: 48,
                timing: "preserve_seconds".into(),
                shorten: "reject".into(),
            },
        })
        .unwrap();
    let child = created["composition"].as_str().unwrap().to_owned();
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(Command::InComposition {
            composition: child.clone(),
            command: Box::new(Command::SetWorkingPrecision {
                precision: ChannelPrecision::Float32,
            }),
        })
        .unwrap();
    assert_eq!(
        engine.project().composition_id,
        motion_core::MAIN_COMPOSITION
    );
    for id in engine.project().composition_ids() {
        assert_eq!(
            engine.project().composition(&id).unwrap().working_precision,
            ChannelPrecision::Float32
        );
        assert_eq!(
            engine
                .project()
                .composition_frame_view(&id)
                .unwrap()
                .working_precision,
            ChannelPrecision::Float32
        );
    }
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project().working_precision, ChannelPrecision::Unorm8);
    let catalogue = motion_core::CommandRegistry::catalogue();
    let descriptor = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "set_working_precision")
        .unwrap();
    assert_eq!(
        descriptor["inputSchema"]["properties"]["precision"]["enum"],
        serde_json::json!(["unorm8", "unorm16", "float16", "float32"])
    );
    assert!(
        motion_core::parse_commands(r#"{"op":"set_working_precision","precision":"16bit"}"#)
            .is_err()
    );
    assert!(motion_core::parse_commands(r#"{"op":"set_working_precision"}"#).is_err());
}
