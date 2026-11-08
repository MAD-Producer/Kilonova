use motion_core::{Command, Engine, Error, Project};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn state(engine: &Engine) -> (Project, serde_json::Value, u64, bool, bool, bool) {
    (
        engine.snapshot(),
        engine.history_diagnostics(),
        engine.revision(),
        engine.can_undo(),
        engine.can_redo(),
        engine.gesture_active(),
    )
}
fn change(engine: &mut Engine, value: f32) {
    engine
        .apply(Command::SetColor {
            object: 0,
            value: [value, value, value, 1.],
        })
        .unwrap();
}

#[test]
fn history_preparation_failure_and_unwind_preserve_complete_state() {
    let mut engine = Engine::new(Project::new(64, 64, 30, 120).unwrap()).unwrap();
    let original = engine.snapshot();
    change(&mut engine, 0.25);
    let edited = engine.snapshot();
    let before = state(&engine);
    let error = engine
        .undo_with_prepare(|destination| {
            assert_eq!(destination, &original);
            Err(Error::Invalid("original undo preparation cause".into()))
        })
        .unwrap_err();
    assert!(matches!(error,Error::Invalid(message) if message=="original undo preparation cause"));
    assert_eq!(state(&engine), before);
    let panic = catch_unwind(AssertUnwindSafe(|| {
        engine.undo_with_prepare(|_| panic!("original undo panic"))
    }))
    .unwrap_err();
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"original undo panic"));
    assert_eq!(state(&engine), before);
    assert!(engine
        .undo_with_prepare(|destination| {
            assert_eq!(destination, &original);
            Ok(())
        })
        .unwrap());
    assert_eq!(engine.snapshot(), original);

    let before = state(&engine);
    assert!(engine
        .redo_with_prepare(|_| Err(Error::Invalid("redo preparation".into())))
        .is_err());
    assert_eq!(state(&engine), before);
    let panic = catch_unwind(AssertUnwindSafe(|| {
        engine.redo_with_prepare(|_| panic!("original redo panic"))
    }))
    .unwrap_err();
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"original redo panic"));
    assert_eq!(state(&engine), before);
    assert!(engine
        .redo_with_prepare(|destination| {
            assert_eq!(destination, &edited);
            Ok(())
        })
        .unwrap());
    assert_eq!(engine.snapshot(), edited);

    engine.begin_gesture().unwrap();
    change(&mut engine, 0.5);
    let before = state(&engine);
    assert!(engine
        .end_gesture_with_prepare(false, |_| Err(Error::Invalid("cancel preparation".into())))
        .is_err());
    assert_eq!(state(&engine), before);
    let panic = catch_unwind(AssertUnwindSafe(|| {
        engine.end_gesture_with_prepare(false, |_| panic!("original cancel panic"))
    }))
    .unwrap_err();
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"original cancel panic"));
    assert_eq!(state(&engine), before);
    engine
        .end_gesture_with_prepare(false, |destination| {
            assert_eq!(destination, &edited);
            Ok(())
        })
        .unwrap();
    assert_eq!(engine.snapshot(), edited);
    assert!(!engine.gesture_active());
}

#[test]
fn no_op_and_blocked_history_never_prepare_a_destination() {
    let mut engine = Engine::new(Project::new(64, 64, 30, 120).unwrap()).unwrap();
    assert!(!engine
        .undo_with_prepare(|_| panic!("empty undo is normal"))
        .unwrap());
    assert!(!engine
        .redo_with_prepare(|_| panic!("empty redo is normal"))
        .unwrap());
    engine.begin_gesture().unwrap();
    assert!(matches!(
        engine.undo_with_prepare(|_| panic!("gesture blocks undo")),
        Err(Error::Gesture)
    ));
    assert!(matches!(
        engine.redo_with_prepare(|_| panic!("gesture blocks redo")),
        Err(Error::Gesture)
    ));
    engine
        .end_gesture_with_prepare(false, |_| panic!("unchanged cancel is normal"))
        .unwrap();
    engine.begin_gesture().unwrap();
    change(&mut engine, 0.25);
    engine
        .end_gesture_with_prepare(true, |_| {
            panic!("commit already owns prepared current state")
        })
        .unwrap();
}
