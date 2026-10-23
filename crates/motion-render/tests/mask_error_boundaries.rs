#[path = "support/mask_error_fixture.rs"]
mod fixture;
use motion_effects::Registry;
use motion_render::{effect_plan::PlanBuilder, RenderError, VectorGeometryError};
use std::{error::Error, sync::Arc};

fn geometry_cause<'a>(error: &'a (dyn Error + 'static)) -> Option<&'a VectorGeometryError> {
    if let Some(RenderError::VectorGeometry(cause)) = error.downcast_ref() {
        Some(cause)
    } else if let Some(cause) = error.downcast_ref::<Box<RenderError>>() {
        geometry_cause(cause.as_ref())
    } else {
        error.source().and_then(geometry_cause)
    }
}

#[test]
fn mask_failures_keep_native_geometry_causes_inputs_and_cached_shape_then_recover() {
    let (project, mut scene) = fixture::scene();
    let before = serde_json::to_value(&project).unwrap();
    let healthy = scene.layers[0].masks.clone();
    let mut builder = PlanBuilder::new(Registry::default()).unwrap();
    let first = builder.build(&scene, &[0], 64, 64, true).unwrap();
    let expected = fixture::bits(first);
    let cached = first.masks[0].vertices.clone();
    for (name, mask) in fixture::malformed() {
        let input = Arc::new(vec![mask]);
        scene.layers[0].masks = input.clone();
        let failure = builder.build(&scene, &[0], 64, 64, true).unwrap_err();
        assert!(
            matches!(geometry_cause(&failure), Some(VectorGeometryError::Invalid(message))
            if message == "vector path coordinate exceeds numeric range"),
            "{name}: {failure:?}"
        );
        for context in ["layer 7", "mask 19", "mask tessellate", "frame 5.5"] {
            assert!(failure.to_string().contains(context), "{name}: {failure}");
        }
        assert!(Arc::ptr_eq(&input, &scene.layers[0].masks));
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
        scene.layers[0].masks = healthy.clone();
        let recovered = builder.build(&scene, &[0], 64, 64, true).unwrap();
        assert_eq!(fixture::bits(recovered), expected);
        assert!(Arc::ptr_eq(&cached, &recovered.masks[0].vertices));
    }
}
