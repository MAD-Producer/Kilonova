#[path = "support/vector_error_fixture.rs"]
mod fixture;
use motion_effects::Registry;
use motion_render::{effect_plan::PlanBuilder, RenderError, VectorGeometryError};
use std::{error::Error, sync::Arc};

fn native_model<'a>(error: &'a (dyn Error + 'static)) -> Option<&'a motion_model::Error> {
    error
        .downcast_ref()
        .or_else(|| error.source().and_then(native_model))
}

#[test]
fn real_vector_failures_keep_causes_inputs_and_cached_geometry_then_recover() {
    let (project, mut scene) = fixture::scene();
    let project_before = serde_json::to_value(&project).unwrap();
    let healthy = scene.layers[0].vector.clone();
    let mut builder = PlanBuilder::new(Registry::default()).unwrap();
    let expected = fixture::bits(builder.build(&scene, &[0], 64, 64, true).unwrap());
    let cached = builder.build(&scene, &[0], 64, 64, true).unwrap().vectors[0]
        .vertices
        .clone();
    for (name, vector) in fixture::malformed() {
        let input = Arc::new(vector);
        scene.layers[0].vector = Some(input.clone());
        let failure = builder.build(&scene, &[0], 64, 64, true).unwrap_err();
        for context in ["layer 7", "vector rasterize", "frame 5.5"] {
            assert!(failure.to_string().contains(context), "{name}: {failure}");
        }
        if name.contains("trim") || name == "dash" {
            assert!(
                matches!(native_model(&failure), Some(motion_model::Error::Invalid(message))
                if message.contains(if name == "dash" { "invalid sampled dash pattern" } else { "invalid sampled trim parameters" }))
            );
        } else {
            assert!(matches!(&failure, RenderError::Diagnostic { cause, .. }
                if matches!(cause.as_ref(), RenderError::VectorGeometry(VectorGeometryError::Invalid(message))
                    if message == "vector path coordinate exceeds numeric range")));
        }
        assert!(Arc::ptr_eq(
            scene.layers[0].vector.as_ref().unwrap(),
            &input
        ));
        assert_eq!(serde_json::to_value(&project).unwrap(), project_before);
        scene.layers[0].vector = healthy.clone();
        let recovered = builder.build(&scene, &[0], 64, 64, true).unwrap();
        assert_eq!(fixture::bits(recovered), expected);
        assert!(Arc::ptr_eq(&recovered.vectors[0].vertices, &cached));
    }
}

#[test]
fn unused_open_path_handles_and_skipped_degenerate_contours_keep_normal_results() {
    let (_, mut scene) = fixture::scene();
    let mut vector = fixture::healthy();
    vector.fill = None;
    vector.paths[0].closed = false;
    scene.layers[0].vector = Some(Arc::new(vector.clone()));
    let mut builder = PlanBuilder::new(Registry::default()).unwrap();
    let expected = fixture::bits(builder.build(&scene, &[0], 64, 64, true).unwrap());
    vector.paths[0].nodes.last_mut().unwrap()[4] = f32::NAN;
    scene.layers[0].vector = Some(Arc::new(vector));
    assert_eq!(
        fixture::bits(builder.build(&scene, &[0], 64, 64, true).unwrap()),
        expected
    );
    let mut skipped = fixture::healthy();
    skipped.paths[0].nodes.truncate(1);
    skipped.paths[0].nodes[0][0] = f32::NAN;
    scene.layers[0].vector = Some(Arc::new(skipped));
    let plan = builder.build(&scene, &[0], 64, 64, true).unwrap();
    assert!(plan.vectors.iter().all(|mesh| mesh.vertices.is_empty()));
    assert!(plan.diagnostics.is_empty());
}
