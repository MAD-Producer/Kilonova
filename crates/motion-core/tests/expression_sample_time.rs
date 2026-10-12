use motion_core::{
    Command, Ease, Engine, ExpressionTarget, Layer, Project, ProjectExpressions, Property,
    PropertyExpression, EXPRESSION_PROFILE,
};

fn engine() -> Engine {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "Card", [20.; 2], [20., 30., 0.], [1.; 4]);
    layer
        .transform
        .position
        .upsert(0, [20., 30., 0.], Ease::Linear)
        .unwrap();
    layer
        .transform
        .position
        .upsert(30, [50., 60., 0.], Ease::Linear)
        .unwrap();
    project.layers.push(layer);
    Engine::new(project).unwrap()
}
fn expression(source: &str) -> PropertyExpression {
    PropertyExpression {
        target: ExpressionTarget::Property {
            object: 1,
            property: Property::Position,
            axis: None,
        },
        source: source.into(),
        enabled: true,
        seed: 1,
        profile: EXPRESSION_PROFILE.into(),
    }
}

#[test]
fn nonfinite_sampling_is_an_expression_error_and_keeps_the_edit_atomic() {
    let mut engine = engine();
    let before = engine.snapshot();
    for source in [
        "valueAtTime(NaN)",
        "valueAtTime(0/0)",
        "valueAtTime(1/0)",
        "valueAtTime(-1/0)",
        "globalThis.__msSample(NaN)",
        "globalThis['__msSample'](NaN)",
    ] {
        let error = engine
            .apply(Command::SetExpression {
                expression: expression(source),
                frame: 0,
            })
            .unwrap_err();
        assert!(
            error.to_string().contains("invalid sample time"),
            "{source}: {error}"
        );
        assert_eq!(before, engine.snapshot());
        assert_eq!(engine.revision(), 0);
        assert!(!engine.can_undo());
    }
    engine
        .apply(Command::SetExpression {
            expression: expression("valueAtTime(0.25)"),
            frame: 0,
        })
        .unwrap();
    let evaluated = engine.project().evaluated_at(0.).unwrap();
    assert_eq!(
        evaluated.layers[0].transform.position.value,
        [27.5, 37.5, 0.]
    );
}

#[test]
fn deferred_nonfinite_sampling_reports_error_without_poisoning_later_evaluation_or_history() {
    let mut engine = engine();
    engine
        .apply(Command::SetExpression {
            expression: expression("time == 0 ? value : valueAtTime(0/0)"),
            frame: 0,
        })
        .unwrap();
    let before = engine.snapshot();
    let revision = engine.revision();
    for _ in 0..3 {
        let error = engine.project().evaluated_at(15.).unwrap_err();
        assert!(error.to_string().contains("invalid sample time"));
        assert!(error.to_string().contains("frame 15"));
        assert_eq!(before, engine.snapshot());
        assert_eq!(revision, engine.revision());
        assert!(engine.project().evaluated_at(0.).is_ok());
    }
    let mut disabled = engine.project().expressions[0].clone();
    disabled.enabled = false;
    engine
        .apply(Command::SetExpression {
            expression: disabled,
            frame: 15,
        })
        .unwrap();
    assert!(engine.project().evaluated_at(15.).is_ok());
    engine.undo().unwrap();
    assert_eq!(before, engine.snapshot());
    engine.redo().unwrap();
    assert!(engine.project().evaluated_at(15.).is_ok());
}
