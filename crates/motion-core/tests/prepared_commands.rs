use motion_core::{Command, Engine, Error, Layer, Project, Property};

fn engine() -> Engine {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project
        .layers
        .push(Layer::solid(1, "Card", [20.; 2], [32., 32., 0.], [1.; 4]));
    Engine::new(project).unwrap()
}
fn position(x: f32) -> Command {
    Command::SetVector {
        object: 1,
        property: Property::Position,
        frame: 0,
        value: [x, 32., 0.],
    }
}
fn refused(_: &Project) -> motion_core::Result<()> {
    Err(Error::Invalid("resource preparation refused".into()))
}

#[test]
fn failed_preparation_preserves_project_revision_and_unrelated_undo() {
    let mut engine = engine();
    let initial = engine.snapshot();
    engine.apply(position(40.)).unwrap();
    let before = engine.snapshot();
    let revision = engine.revision();
    assert!(engine
        .apply_batch_with_prepare(vec![position(50.)], refused)
        .is_err());
    assert_eq!(before, engine.snapshot());
    assert_eq!(revision, engine.revision());
    assert!(engine.can_undo());
    assert!(!engine.can_redo());
    engine.undo().unwrap();
    assert_eq!(initial, engine.snapshot());
    engine.redo().unwrap();
    assert_eq!(before, engine.snapshot());
}

#[test]
fn failed_noop_preparation_preserves_both_undo_and_redo_entries() {
    let mut engine = engine();
    engine.apply(position(40.)).unwrap();
    let first = engine.snapshot();
    engine.apply(position(50.)).unwrap();
    let second = engine.snapshot();
    engine.undo().unwrap();
    let revision = engine.revision();
    assert!(engine
        .apply_batch_with_prepare(vec![position(40.)], refused)
        .is_err());
    assert_eq!(first, engine.snapshot());
    assert_eq!(revision, engine.revision());
    assert!(engine.can_undo());
    assert!(engine.can_redo());
    engine.redo().unwrap();
    assert_eq!(second, engine.snapshot());
    engine.undo().unwrap();
    assert_eq!(first, engine.snapshot());
}

#[test]
fn failed_preparation_preserves_active_gesture_and_its_commit_or_cancel() {
    for commit in [false, true] {
        let mut engine = engine();
        let initial = engine.snapshot();
        engine.begin_gesture().unwrap();
        engine.apply(position(40.)).unwrap();
        let before = engine.snapshot();
        let revision = engine.revision();
        assert!(engine
            .apply_batch_with_prepare(vec![position(50.)], refused)
            .is_err());
        assert_eq!(before, engine.snapshot());
        assert_eq!(revision, engine.revision());
        assert!(engine.gesture_active());
        assert!(!engine.can_undo());
        engine.end_gesture(commit).unwrap();
        if commit {
            assert_eq!(before, engine.snapshot());
            engine.undo().unwrap();
        }
        assert_eq!(initial, engine.snapshot());
    }
}

#[test]
fn invalid_command_rolls_back_before_invoking_external_preparation() {
    let mut engine = engine();
    let initial = engine.snapshot();
    let mut prepared = false;
    let result = engine.apply_batch_with_prepare(
        vec![position(40.), Command::Delete { object: 99 }],
        |_| {
            prepared = true;
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(!prepared);
    assert_eq!(initial, engine.snapshot());
    assert_eq!(engine.revision(), 0);
    assert!(!engine.can_undo());
}

#[test]
fn prepared_batch_commits_once_and_callback_sees_final_validated_model() {
    let mut engine = engine();
    let initial = engine.snapshot();
    let mut calls = 0;
    engine
        .apply_batch_with_prepare(vec![position(40.), position(50.)], |project| {
            project.validate()?;
            assert_eq!(project.layers[0].transform.position.value[0], 50.);
            calls += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(engine.revision(), 1);
    engine.undo().unwrap();
    assert_eq!(initial, engine.snapshot());
    assert!(!engine.can_undo());
    engine.redo().unwrap();
    assert_eq!(engine.project().layers[0].transform.position.value[0], 50.);
}

#[test]
fn successful_noop_preparation_does_not_add_history_or_revision() {
    let mut engine = engine();
    let mut calls = 0;
    let initial = engine.snapshot();
    engine
        .apply_batch_with_prepare(vec![position(32.)], |_| {
            calls += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(initial, engine.snapshot());
    assert_eq!(engine.revision(), 0);
    assert!(!engine.can_undo());
}
