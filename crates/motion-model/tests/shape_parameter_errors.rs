use motion_model::{
    vector::{
        groups::VectorGroup, shape_parameters, VectorAction, VectorContent, VectorSource, SHAPES,
    },
    Error,
};

fn missing(vector: &mut VectorContent, parameter: &str) {
    let VectorSource::Shape { parameters, .. } = &mut vector.source else {
        panic!("expected parametric fixture");
    };
    assert!(parameters.remove(parameter).is_some());
}

fn refusal(error: Error, operation: &str, parameter: &str) {
    let Error::Invalid(message) = error else {
        panic!("shape refusal lost model identity: {error:?}");
    };
    for context in [
        format!("operation={operation}"),
        format!("parameter={parameter}"),
        "shape=".into(),
    ] {
        assert!(message.contains(&context), "{message}");
    }
}

#[test]
fn every_missing_shape_parameter_is_rejected_by_sampling_without_mutating_input() {
    for (shape, ..) in SHAPES {
        for parameter in shape_parameters(shape) {
            let mut vector = VectorContent::shape(shape);
            missing(&mut vector, parameter.id);
            let before = vector.clone();
            refusal(
                vector.sample(7.5, [160., 90.]).unwrap_err(),
                "sample",
                parameter.id,
            );
            assert_eq!(vector, before);
        }
    }
}

#[test]
fn every_missing_shape_parameter_is_rejected_before_editing_tracks() {
    for (shape, ..) in SHAPES {
        for parameter in shape_parameters(shape) {
            let mut vector = VectorContent::shape(shape);
            missing(&mut vector, parameter.id);
            let before = vector.clone();
            refusal(
                vector
                    .edit(
                        VectorAction::SetParameter {
                            parameter: parameter.id.into(),
                            frame: 7,
                            value: parameter.default,
                            animated: true,
                        },
                        [160., 90.],
                        3,
                    )
                    .unwrap_err(),
                "edit",
                parameter.id,
            );
            assert_eq!(vector, before);
        }
    }
}

#[test]
fn missing_shape_parameters_propagate_through_conversion_and_nested_group_sampling() {
    for (shape, ..) in SHAPES {
        for parameter in shape_parameters(shape) {
            let mut vector = VectorContent::shape(shape);
            missing(&mut vector, parameter.id);
            let before = vector.clone();
            refusal(
                vector
                    .edit(VectorAction::ConvertToPath { frame: 7 }, [160., 90.], 3)
                    .unwrap_err(),
                "sample",
                parameter.id,
            );
            assert_eq!(vector, before);
            let nested = VectorGroup::wrap(VectorGroup::wrap(vector, [160., 90.]), [160., 90.]);
            let before = nested.clone();
            refusal(
                nested.sample(7.5, [160., 90.]).unwrap_err(),
                "sample",
                parameter.id,
            );
            assert_eq!(nested, before);
        }
    }
}

#[test]
fn all_catalog_shapes_keep_default_geometry_after_parameter_edits_and_conversion() {
    for (shape, ..) in SHAPES {
        let mut vector = VectorContent::shape(shape);
        vector.validate().unwrap();
        let paths = vector.sample(7.5, [160., 90.]).unwrap().paths;
        for parameter in shape_parameters(shape) {
            vector
                .edit(
                    VectorAction::SetParameter {
                        parameter: parameter.id.into(),
                        frame: 7,
                        value: parameter.default,
                        animated: false,
                    },
                    [160., 90.],
                    3,
                )
                .unwrap();
            assert_eq!(vector.sample(7.5, [160., 90.]).unwrap().paths, paths);
        }
        vector
            .edit(VectorAction::ConvertToPath { frame: 7 }, [160., 90.], 3)
            .unwrap();
        assert!(matches!(vector.source, VectorSource::Paths { .. }));
        assert_eq!(vector.sample(7.5, [160., 90.]).unwrap().paths, paths);
    }
}
