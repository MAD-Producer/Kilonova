use motion_core::{composition::CompositionError, Project};
use motion_host::{ops, Error};
use motion_render::{composition_plan::CompositionPlanError, RenderError};
use std::sync::Arc;

fn missing_composition() -> RenderError {
    Project::new(64, 64, 30, 60)
        .unwrap()
        .composition("missing-render-composition")
        .unwrap_err()
        .into()
}

fn detail(error: &RenderError) -> &CompositionError {
    match error {
        RenderError::Model(motion_core::Error::Composition(detail)) => detail,
        _ => panic!("expected the actual project lookup failure: {error:?}"),
    }
}

#[test]
fn render_wrappers_keep_native_composition_details_in_borrowed_and_owned_replies() {
    let original = Arc::new(missing_composition());
    let expected = detail(&original);
    let expected_json = serde_json::to_value(expected).unwrap();
    let weak = Arc::downgrade(&original);
    let render = RenderError::Context {
        message: "render request context".into(),
        cause: Box::new(RenderError::Diagnostic {
            message: "sample phase context".into(),
            cause: Box::new(RenderError::CompositionPlan(
                CompositionPlanError::Encoding(Box::new(RenderError::Shared(original.clone()))),
            )),
        }),
    };
    let error = Error::with_context("host render request", Error::from(render));
    assert!(std::ptr::eq(error.composition_detail().unwrap(), expected));
    for _ in 0..60 {
        let reply = ops::typed_error_envelope(&error);
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error_detail"], expected_json);
        assert_eq!(reply["error"], "host render request: render request context");
        assert_eq!(Arc::strong_count(&original), 2);
    }
    let reply = ops::dispatch(|| Err(error));
    assert_eq!(reply["error_detail"], expected_json);
    assert_eq!(reply["error"], "host render request: render request context");
    assert_eq!(Arc::strong_count(&original), 1);
    drop(original);
    assert!(weak.upgrade().is_none());
}

#[test]
fn render_composition_details_follow_only_the_primary_failure() {
    let primary = missing_composition();
    let expected = serde_json::to_value(detail(&primary)).unwrap();
    let error = Error::Cleanup {
        primary: Box::new(primary.into()),
        secondary: Box::new(Error::Message("cleanup failed".into())),
    };
    assert_eq!(ops::typed_error_envelope(&error)["error_detail"], expected);

    let io = tempfile::tempdir().unwrap().path().join("absent");
    let original = std::fs::File::open(&io).unwrap_err();
    assert_eq!(original.kind(), std::io::ErrorKind::NotFound);
    let code = original.raw_os_error();
    let error = Error::Cleanup {
        primary: Box::new(Error::from(RenderError::from(original))),
        secondary: Box::new(missing_composition().into()),
    };
    let Error::Cleanup { primary, .. } = &error else { unreachable!() };
    assert!(matches!(primary.as_ref(), Error::Render(RenderError::Io(cause))
        if cause.kind() == std::io::ErrorKind::NotFound && cause.raw_os_error() == code));
    assert!(error.composition_detail().is_none());
    let reply = ops::typed_error_envelope(&error);
    assert_eq!(reply["error"], error.to_string());
    assert!(reply["error_detail"].is_null());
}

#[test]
fn render_text_and_non_composition_variants_cannot_forge_native_details() {
    let text = missing_composition().to_string();
    assert!(text.starts_with("composition_error:"));
    let errors = [
        RenderError::Invalid(text.clone()),
        RenderError::Readback(text.clone()),
        RenderError::Context {
            message: text.clone(),
            cause: Box::new(RenderError::Invalid("validation failed".into())),
        },
        RenderError::Diagnostic {
            message: text.clone(),
            cause: Box::new(RenderError::Shared(Arc::new(RenderError::Invalid(text)))),
        },
        CompositionPlanError::MissingReference { composition: "missing".into(), layer: 7 }.into(),
        RenderError::Model(motion_core::Error::Missing(7)),
    ];
    for render in errors {
        let error = Error::from(render);
        let before = error.to_string();
        assert!(error.composition_detail().is_none());
        let reply = ops::dispatch(|| Err(error));
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"], before);
        assert!(reply["error_detail"].is_null(), "text forged details: {reply}");
    }
}
