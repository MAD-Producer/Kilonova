use motion_core::{
    Axis, Command, Engine, Error, ExpressionTarget, Layer, Project, ProjectExpressions, Property,
    PropertyExpression, EXPRESSION_PROFILE,
};
use std::sync::{atomic::AtomicBool, Arc};

fn target() -> ExpressionTarget {
    ExpressionTarget::Property { object: 7, property: Property::Position, axis: Some(Axis::X) }
}

fn engine() -> Engine {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.layers.push(Layer::solid(7, "Native cause", [20.; 2], [10., 20., 0.], [1.; 4]));
    Engine::new(project).unwrap()
}

fn set(engine: &mut Engine, source: &str) -> motion_core::Result<()> {
    engine.apply(Command::SetExpression {
        expression: PropertyExpression {
            target: target(), source: source.into(), enabled: true, seed: 42,
            profile: EXPRESSION_PROFILE.into(),
        },
        frame: 0,
    })
}

fn assert_cause(error: &Error, expected_frame: f64, contains: &str) {
    let Error::Expression { target: metadata, frame, source } = error else {
        panic!("missing expression context: {error:?}");
    };
    assert_eq!(*frame, expected_frame);
    assert_eq!(serde_json::from_str::<serde_json::Value>(metadata).unwrap(), serde_json::to_value(target()).unwrap());
    assert!(matches!(source.as_ref(), Error::Invalid(reason) if reason.contains(contains)), "{source:?}");
    let retained = std::error::Error::source(error).expect("native source was flattened");
    assert_eq!(retained.to_string(), source.to_string());
    assert_eq!(error.to_string(), format!("expression {metadata} at frame {frame}: {source}"));
}

#[test]
fn refused_expression_edits_keep_native_causes_without_history_or_model_changes() {
    for (source, reason) in [("value +", "JavaScript"), ("[1,2]", "scalar")] {
        let mut engine = engine();
        let before = engine.snapshot();
        let history = engine.history_diagnostics();
        let error = set(&mut engine, source).unwrap_err();
        assert_cause(&error, 0., reason);
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.history_diagnostics(), history);
        assert!(!engine.can_undo());
        set(&mut engine, "value + 2").unwrap();
        assert_eq!(engine.project().evaluated_at(0.).unwrap().layers[0].transform.position.value[0], 12.);
    }
}

#[test]
fn deferred_execution_and_normal_cancellation_keep_causes_and_allow_recovery() {
    let mut engine = engine();
    set(&mut engine, "time < 1 ? value : (()=>1).constructor('return 41')()").unwrap();
    let project = engine.snapshot();
    let before = project.clone();
    let error = project.evaluated_at(30.).unwrap_err();
    assert_cause(&error, 30., "JavaScript");
    assert_eq!(project, before);
    assert_eq!(project.evaluated_at(0.).unwrap().layers[0].transform.position.value[0], 10.);
    let cancelled = project.evaluated_at_cancellable(5.5, Arc::new(AtomicBool::new(true))).unwrap_err();
    assert_cause(&cancelled, 5.5, "cancelled");
    assert_eq!(project, before);
    assert_eq!(project.evaluated_at(0.).unwrap().layers[0].transform.position.value[0], 10.);
}
