use motion_core::{parse_commands, Command, Engine, Guide, GuideAxis, Layer, Project, MAX_GUIDES, MAX_GUIDE_POSITION};
use serde_json::{json, Value};

fn project() -> Project {
    let mut project = Project::new(640, 360, 30, 90).unwrap();
    project.layers.push(Layer::solid(7, "Guide study", [80.; 2], [320., 180., 0.], [1.; 4]));
    project
}
fn guides() -> Vec<Guide> {
    vec![Guide { id: 1, axis: GuideAxis::X, position: 120.25 },
         Guide { id: 2, axis: GuideAxis::Y, position: -24.5 }]
}

#[test]
fn guide_commands_preserve_layers_and_roundtrip_storage_with_single_step_undo_redo() {
    let original = project();
    assert!(serde_json::to_value(&original).unwrap().get("guides").is_none());
    let mut engine = Engine::new(original.clone()).unwrap();
    let commands = parse_commands(&json!({"op":"set_guides","composition":"comp-main","guides":guides()}).to_string()).unwrap();
    engine.apply_batch(commands).unwrap();
    let authored = engine.snapshot();
    assert_eq!(authored.guides, guides());
    assert_eq!(authored.layers, original.layers);
    assert_eq!(engine.history_diagnostics()["undoEntries"], 1);
    let temp = tempfile::tempdir().unwrap();
    motion_core::storage::save(temp.path(), &authored).unwrap();
    assert_eq!(motion_core::storage::load(temp.path()).unwrap(), authored);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &original);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &authored);
    engine.apply(Command::SetGuides { guides: Vec::new() }).unwrap();
    assert!(engine.project().guides.is_empty());
    assert!(serde_json::to_value(engine.project()).unwrap().get("guides").is_none());
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &authored);
}

#[test]
fn composition_guides_keep_their_own_identity_through_scope_switches_and_reopen() {
    let mut project = project();
    let source = serde_json::to_value(&project).unwrap();
    let mut child = json!({});
    for key in ["name","width","height","fps","frames","work_area","background","camera"] {
        child[key] = source[key].clone();
    }
    child["id"] = json!("comp-guide-detail");
    child["layers"] = json!([]);
    project.compositions.push(serde_json::from_value(child).unwrap());
    let mut engine = Engine::new(project).unwrap();
    engine.apply(Command::SetGuides { guides: guides() }).unwrap();
    engine.apply_batch(parse_commands(&json!({"op":"set_guides","composition":"comp-guide-detail", "guides":[
        {"id":1,"axis":"y","position":48.5}
    ]}).to_string()).unwrap()).unwrap();
    let before = engine.snapshot();
    assert_eq!(before.guides, guides());
    let child_guides = before.composition_view("comp-guide-detail").unwrap().guides;
    assert_eq!(child_guides, &[Guide { id: 1, axis: GuideAxis::Y, position: 48.5 }]);
    let mut switched = before.clone();
    switched.activate_composition("comp-guide-detail").unwrap();
    assert_eq!(switched.guides, child_guides);
    switched.activate_composition("comp-main").unwrap();
    assert_eq!(switched, before);
    let revision = engine.revision();
    assert!(engine.apply_batch(parse_commands(&json!({"op":"set_guides","composition":"comp-does-not-exist","guides":[]}).to_string()).unwrap()).is_err());
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.revision(), revision);
    let temp = tempfile::tempdir().unwrap();
    motion_core::storage::save(temp.path(), &before).unwrap();
    assert_eq!(motion_core::storage::load(temp.path()).unwrap(), before);
    assert!(engine.undo().unwrap());
    assert!(engine.project().composition_view("comp-guide-detail").unwrap().guides.is_empty());
    assert_eq!(engine.project().guides, guides());
}

#[test]
fn invalid_guide_values_and_mixed_batches_leave_the_document_revision_and_history_unchanged() {
    let mut project = project();
    project.guides = guides();
    let mut engine = Engine::new(project.clone()).unwrap();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    let mut invalid = vec![vec![Guide { id: 0, axis: GuideAxis::X, position: 0. }],
        vec![Guide { id: 1, axis: GuideAxis::X, position: 0. }, Guide { id: 1, axis: GuideAxis::Y, position: 1. }]];
    for position in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, MAX_GUIDE_POSITION * 2., -MAX_GUIDE_POSITION * 2.] {
        invalid.push(vec![Guide { id: 4, axis: GuideAxis::X, position }]);
    }
    invalid.push((1..=MAX_GUIDES+1).map(|id| Guide { id: id as u64, axis: GuideAxis::X, position: id as f64 }).collect());
    for guides in invalid {
        let error = engine.apply(Command::SetGuides { guides }).unwrap_err();
        assert!(error.to_string().contains("guide"));
        assert_eq!(engine.project(), &project);
        assert_eq!(engine.revision(), revision);
        assert_eq!(engine.history_diagnostics(), history);
    }
    assert!(engine.apply_batch(vec![Command::SetGuides { guides: Vec::new() },
        Command::SetGuides { guides: vec![Guide { id: 0, axis: GuideAxis::Y, position: 0. }] }]).is_err());
    assert_eq!(engine.project(), &project);
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.history_diagnostics(), history);
    let at_limit = (1..=MAX_GUIDES).map(|id| Guide { id: id as u64, axis: GuideAxis::X, position: MAX_GUIDE_POSITION }).collect();
    engine.apply(Command::SetGuides { guides: at_limit }).unwrap();
    assert_eq!(engine.project().guides.len(), MAX_GUIDES);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &project);
}

#[test]
fn guide_schema_errors_are_strict_bounded_and_do_not_echo_untrusted_values() {
    let secret = "private-guide-input-".repeat(512);
    let good = json!({"id":1,"axis":"x","position":5.});
    let mut bad_key = good.clone();
    bad_key.as_object_mut().unwrap().insert(secret.clone(), Value::Bool(true));
    let mut payloads = vec![json!(secret), json!([secret]), json!([bad_key])];
    for key in ["id", "axis", "position"] {
        let mut bad = good.clone();bad[key] = json!(secret);payloads.push(json!([bad]));
    }
    for guides in payloads {
        let error = parse_commands(&json!({"op":"set_guides","guides":guides}).to_string()).unwrap_err().to_string();
        assert!(!error.contains("private-guide-input"));
        assert!(error.len() < 256, "diagnostic must remain bounded");
    }
    for text in [r#"{"id":1,"id":2,"axis":"x","position":5}"#,
                 r#"{"id":1,"axis":"x","axis":"y","position":5}"#,
                 r#"{"id":1,"axis":"x","position":5,"position":6}"#] {
        assert!(serde_json::from_str::<Guide>(text).is_err());
    }
}
