use motion_core::{composition::CompositionError, Project};
use motion_host::{ops, Error};
use motion_render::{composition_plan::CompositionPlanError, image_resources, RenderError, VectorGeometryError};
use std::sync::Arc;

fn missing() -> motion_core::Error {
    Project::new(32, 32, 30, 60).unwrap()
        .composition("missing-resource-composition").unwrap_err()
}

fn image_detail(error: &image_resources::Error) -> &CompositionError {
    match error {
        image_resources::Error::Model(motion_core::Error::Composition(detail)) => detail,
        _ => panic!("unexpected fixture: {error:?}"),
    }
}

#[test]
fn shared_image_causes_keep_the_same_borrowed_detail_and_release_after_dispatch() {
    let original = Arc::new(image_resources::Error::from(missing()));
    let detail = image_detail(&original);
    let expected = serde_json::to_value(detail).unwrap();
    let weak = Arc::downgrade(&original);
    let error = Error::with_context("image request", Error::from(
        image_resources::Error::Shared(Arc::new(image_resources::Error::Shared(original.clone())))));
    for _ in 0..60 {
        assert!(std::ptr::eq(error.composition_detail().expect("native image detail"), detail));
        assert_eq!(ops::typed_error_envelope(&error)["error_detail"], expected);
        assert_eq!(Arc::strong_count(&original), 2);
    }
    let text = error.to_string();
    let reply = ops::dispatch(|| Err(error));
    assert_eq!(reply["error"], text);
    assert_eq!(reply["error_detail"], expected);
    assert_eq!(Arc::strong_count(&original), 1);
    drop(original);
    assert!(weak.upgrade().is_none());
}

#[test]
fn resource_wrappers_preserve_details_without_using_the_display_prefix() {
    let expected = serde_json::to_value(match missing() {
        motion_core::Error::Composition(detail) => detail,
        other => panic!("unexpected fixture: {other:?}"),
    }).unwrap();
    let cases = [
        Error::from(image_resources::Error::from(missing())),
        Error::from(RenderError::from(image_resources::Error::Shared(Arc::new(
            image_resources::Error::from(missing()))))),
        Error::from(RenderError::Diagnostic {
            message: "image render context".into(),
            cause: Box::new(RenderError::Shared(Arc::new(RenderError::from(
                image_resources::Error::from(missing()))))),
        }),
    ];
    for error in cases {
        let text = error.to_string();
        assert_eq!(ops::typed_error_envelope(&error)["error_detail"], expected, "{text}");
        let reply = ops::dispatch(|| Err(error));
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"], text);
        assert_eq!(reply["error_detail"], expected);
    }
}

#[test]
fn vector_wrappers_preserve_native_details_through_render_context() {
    let expected = serde_json::to_value(match missing() {
        motion_core::Error::Composition(detail) => detail,
        other => panic!("unexpected fixture: {other:?}"),
    }).unwrap();
    let cases = [
        Error::from(RenderError::from(VectorGeometryError::from(missing()))),
        Error::from(RenderError::Context {
            message: "vector render context".into(),
            cause: Box::new(RenderError::CompositionPlan(CompositionPlanError::Encoding(
                Box::new(RenderError::from(VectorGeometryError::from(missing())))))),
        }),
    ];
    for error in cases {
        let text = error.to_string();
        assert_eq!(ops::typed_error_envelope(&error)["error_detail"], expected, "{text}");
        let reply = ops::dispatch(|| Err(error));
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"], text);
        assert_eq!(reply["error_detail"], expected);
    }
}

#[test]
fn resource_text_and_secondary_causes_do_not_forge_primary_details() {
    let text = missing().to_string();
    let cases = [
        Error::from(image_resources::Error::Invalid(text.clone())),
        Error::from(image_resources::Error::Shared(Arc::new(
            image_resources::Error::Invalid(text.clone())))),
        Error::from(RenderError::from(VectorGeometryError::Invalid(text))),
        Error::Cleanup {
            primary: Box::new(image_resources::Error::Cancelled.into()),
            secondary: Box::new(image_resources::Error::from(missing()).into()),
        },
    ];
    for error in cases {
        assert!(error.composition_detail().is_none());
        let text = error.to_string();
        let reply = ops::dispatch(|| Err(error));
        assert_eq!(reply["error"], text);
        assert!(reply["error_detail"].is_null());
    }
}
