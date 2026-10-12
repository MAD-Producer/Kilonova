use motion_core::{Command, Engine, Project};

#[test]
fn panicking_prepare_restores_candidate_without_changing_history_revision_or_gesture() {
    for gesture in [false, true] {
        let mut engine = Engine::new(Project::new(64, 64, 30, 60).unwrap()).unwrap();
        let initial = engine.snapshot();
        engine
            .apply(Command::SetColor {
                object: 0,
                value: [0., 1., 0., 1.],
            })
            .unwrap();
        if gesture {
            engine.begin_gesture().unwrap();
        }
        let before = engine.snapshot();
        let revision = engine.revision();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine.apply_batch_with_prepare(
                vec![Command::SetColor {
                    object: 0,
                    value: [1., 0., 0., 1.],
                }],
                |_| panic!("injected prepare panic"),
            )
        }));
        assert!(result.is_err());
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.revision(), revision);
        assert_eq!(engine.gesture_active(), gesture);
        if gesture {
            engine.end_gesture(false).unwrap();
        }
        engine.undo().unwrap();
        assert_eq!(engine.snapshot(), initial);
        engine.redo().unwrap();
        assert_eq!(engine.snapshot(), before);
    }
}
