use super::*;
use motion_model::{EffectInstance, Layer, Project};
use std::error::Error;

fn project() -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "gaussian_blur")
        .unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "test", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        layer.size,
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project
}

#[test]
fn shader_failure_retains_native_effect_error_and_source() {
    let error = utility("invalid WGSL").err().unwrap();
    assert!(matches!(
        &error,
        RenderError::Effects(motion_effects::Error::ShaderParse { .. })
    ));
    assert!(error.source().unwrap().is::<motion_effects::Error>());
    let RenderError::Effects(motion_effects::Error::ShaderParse { cause, .. }) = &error else {
        unreachable!()
    };
    assert!(std::ptr::addr_eq(
        error.source().unwrap().source().unwrap(),
        cause.as_ref(),
    ));
}

#[test]
fn cached_resolution_failure_retains_same_native_error_and_recovers() {
    let project = project();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let hash = scene.effects[0].hash.clone();
    scene.effects[0].hash = "f".repeat(64);
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    let first = builder.build(&scene, &[0], 64, 64, true).err().unwrap();
    let cached = builder.resolved[0]
        .as_ref()
        .unwrap()
        .error
        .as_ref()
        .unwrap()
        .clone();
    assert!(matches!(cached.as_ref(), RenderError::Effects(_)));
    let second = builder.build(&scene, &[0], 64, 64, true).err().unwrap();
    for error in [&first, &second] {
        let RenderError::Diagnostic { message, cause } = error else {
            panic!("{error:?}")
        };
        assert!(message.contains("layer 1, effect 1: missing plugin"));
        let RenderError::Shared(original) = cause.as_ref() else {
            panic!("{cause:?}")
        };
        assert!(
            Arc::ptr_eq(original, &cached),
            "cache must not reconstruct a string error"
        );
        assert!(original.source().unwrap().is::<motion_effects::Error>());
        assert!(error.resource_rejection().is_none());
    }
    scene.effects[0].hash = hash;
    assert!(builder
        .build(&scene, &[0], 64, 64, true)
        .unwrap()
        .diagnostics
        .is_empty());
    assert!(builder.resolved[0].as_ref().unwrap().error.is_none());
}

#[test]
fn preflight_failure_preserves_location_and_original_effect_source() {
    let mut project = project();
    project.layers[0].effects[0].hash = "f".repeat(64);
    let builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    let error = builder.preflight_project(&project).unwrap_err();
    assert!(error
        .to_string()
        .contains("layer 1, effect 1 (gaussian_blur): missing plugin"));
    let RenderError::Context { cause, .. } = error else {
        panic!("{error:?}")
    };
    assert!(matches!(cause.as_ref(), RenderError::Effects(_)));
    assert!(cause.source().unwrap().is::<motion_effects::Error>());
}

#[test]
fn malformed_dependency_pass_ranges_return_errors_and_healthy_range_recovers() {
    let mut frame = EffectFramePlan::default();
    frame.draws.push(PlannedDraw {
        layer: 1,
        words: [0.; DRAW_WORDS],
        pass_start: 0,
        pass_end: 0,
    });
    frame.compositing.push(LayerComposite {
        blend: 0,
        space: 0,
        matte: 0,
        mode: 0,
        visible: true,
    });
    for (start, end) in [(0, 1), (2, 1), (usize::MAX, usize::MAX)] {
        frame.draws[0].pass_start = start;
        frame.draws[0].pass_end = end;
        let error = image_input_order(&frame).unwrap_err();
        assert!(matches!(error, RenderError::Invalid(_)));
        assert_eq!(error.to_string(), format!("GPU validation: effect image input draw 0 has invalid pass range {start}..{end} (0 passes)"));
    }
    frame.draws[0].pass_start = 0;
    frame.draws[0].pass_end = 0;
    assert_eq!(image_input_order(&frame).unwrap(), [0]);
}

#[test]
fn undersized_plan_buffer_refuses_without_writing() {
    let scene = Scene::new(&project()).unwrap();
    let plan = EffectFramePlan::default();
    let mut output = [0xab; 1];
    let error = plan.write(&scene, &mut output).unwrap_err();
    assert!(matches!(error, RenderError::Invalid(_)));
    assert_eq!(
        error.to_string(),
        format!(
            "GPU validation: render plan buffer requires {} bytes",
            plan.buffer_bytes(&scene)
        )
    );
    assert_eq!(output, [0xab]);
}

#[test]
fn nested_encoding_and_shared_context_keep_the_original_resource_refusal() {
    let rejection = ResourceRejection::TextureDimensions {
        width: 65,
        height: 64,
        max_dimension: 64,
    };
    let shared = Arc::new(RenderError::Resource {
        rejection,
        message: "measured refusal".into(),
    });
    let error =
        RenderError::Shared(shared.clone()).with_diagnostic("layer 1: measured refusal".into());
    let error = RenderError::from(crate::composition_plan::CompositionPlanError::Encoding(
        Box::new(error),
    ));
    assert!(std::ptr::eq(
        error.resource_rejection().unwrap(),
        shared.resource_rejection().unwrap()
    ));
    assert_eq!(
        error.to_string(),
        "GPU validation: layer 1: measured refusal"
    );
}
