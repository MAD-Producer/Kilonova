use motion_core::{
    prepare_expression_sources, ExpressionTarget, Layer, Project, ProjectExpressions, Property,
    PropertyExpression, EXPRESSION_PROFILE,
};

fn project(source: &str) -> Project {
    let mut project = Project::new(64, 48, 30, 60).unwrap();
    project.layers.push(Layer::solid(
        1,
        "Prepared expression",
        [20.; 2],
        [0.; 3],
        [1.; 4],
    ));
    project.expressions.push(PropertyExpression {
        target: ExpressionTarget::Property {
            object: 1,
            property: Property::Position,
            axis: None,
        },
        source: source.into(),
        enabled: true,
        seed: 7,
        profile: EXPRESSION_PROFILE.into(),
    });
    project.validate().unwrap();
    project
}

#[test]
fn preparation_compiles_without_executing_user_code_or_mutating_the_project() {
    // A fresh OS thread owns a fresh runtime and an empty bytecode cache.
    std::thread::spawn(|| {
        let project = project("throw new Error('execution phase only')");
        let original = project.clone();
        prepare_expression_sources(&project).unwrap();
        assert_eq!(project, original);
        let error = project.evaluated_at(0.).unwrap_err();
        assert!(
            error.to_string().contains("execution phase only"),
            "{error}"
        );
        assert_eq!(project, original);
    })
    .join()
    .unwrap();
}

#[test]
fn preparation_preserves_native_syntax_failure_and_skips_disabled_sources() {
    let mut project = project("[500,");
    let original = project.clone();
    let error = prepare_expression_sources(&project).unwrap_err();
    let motion_core::Error::Expression {
        target,
        frame,
        source,
    } = &error
    else {
        panic!("{error:?}");
    };
    assert_eq!(*frame, 0.);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(target).unwrap(),
        serde_json::to_value(&project.expressions[0].target).unwrap()
    );
    assert!(source.to_string().contains("JavaScript"), "{error}");
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(project, original);
    project.expressions[0].enabled = false;
    prepare_expression_sources(&project).unwrap();
    assert_eq!(
        project.evaluated_at(0.).unwrap().layers[0]
            .transform
            .position
            .value,
        [0.; 3]
    );
}

#[test]
fn prepared_sources_keep_cancellation_and_execution_limits() {
    std::thread::spawn(|| {
        let literal = project("[500,0,0]");
        prepare_expression_sources(&literal).unwrap();
        assert_eq!(
            literal.evaluated_at(0.).unwrap().layers[0]
                .transform
                .position
                .value,
            [500., 0., 0.]
        );
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let error = literal.evaluated_at_cancellable(0., cancelled).unwrap_err();
        assert!(error.to_string().contains("cancelled"), "{error}");
        let infinite = project("while (true) {}\n[500,0,0]");
        prepare_expression_sources(&infinite).unwrap();
        let error = infinite.evaluated_at(0.).unwrap_err();
        assert!(error.to_string().contains("budget exceeded"), "{error}");
    })
    .join()
    .unwrap();
}
