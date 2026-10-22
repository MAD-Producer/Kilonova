use super::*;
use motion_core::{ExpressionEvaluator, Layer, Project};
use motion_effects::{LensElement, LensShape};
use motion_render::{
    scene_generator::{generate, GeneratorScratch},
    Scene,
};
use std::collections::HashMap;

pub(super) fn actual_refusals() -> Vec<RenderError> {
    let mut bad_rate = effect();
    bad_rate.values[0][0] = f32::NAN;
    let failure = validate_settings(&bad_rate, RendererKind::Particles).unwrap_err();
    assert!(failure
        .to_string()
        .contains("parameter rate contains non-finite components"));
    let mut failures = vec![failure];
    let mut project = Project::new(128, 128, 30, 120).unwrap();
    project.layers.push(Layer::solid(
        7,
        "Numeric",
        [128.; 2],
        [64., 64., 0.],
        [1.; 4],
    ));
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &ExpressionEvaluator)
        .unwrap();
    let mut effect = effect();
    effect.param_ids = [
        "position",
        "intensity",
        "attenuation",
        "reference_distance",
        "occlusion_radius",
        "scale",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    effect.values = [[0.; 4]; MAX_PARAMS];
    effect.values[0] = [64., 64., 0., 0.];
    effect.values[1][0] = 1.;
    effect.values[3][0] = 100.;
    effect.values[5][0] = 100.;
    effect.scene = Some(SceneSettings {
        elements: vec![LensElement {
            id: 1,
            shape: LensShape::Glow,
            enabled: true,
            offset: 0.,
            size: [16.; 2],
            color: [1.; 4],
            intensity: 1.,
            rays: 2,
            chromatic: 0.,
        }],
        ..Default::default()
    });
    let values = effect.values;
    let mut output = Vec::new();
    let mut scratch = GeneratorScratch::default();
    let run = |effect: &SampledEffect, output: &mut Vec<_>, scratch: &mut GeneratorScratch| {
        generate(
            &scene,
            &scene.layers[0],
            effect,
            RendererKind::LensFlare,
            &HashMap::new(),
            output,
            scratch,
        )
    };
    run(&effect, &mut output, &mut scratch).unwrap();
    let expected = output.clone();
    assert!(!expected.is_empty());
    for (index, value) in [(1, f32::NAN), (5, f32::INFINITY), (1, f32::MAX)] {
        effect.values = values;
        effect.values[index][0] = value;
        effect.scene.as_mut().unwrap().elements[0].intensity =
            if value.is_finite() { 32. } else { 1. };
        let input = effect.values.map(|v| v.map(f32::to_bits));
        let failure = run(&effect, &mut output, &mut scratch).unwrap_err();
        assert!(matches!(&failure, RenderError::Context { cause, .. }
            if matches!(cause.as_ref(), RenderError::Invalid(_))));
        assert_eq!(effect.values.map(|v| v.map(f32::to_bits)), input);
        assert_eq!(output, expected);
        failures.push(failure);
    }
    effect.values = values;
    effect.scene.as_mut().unwrap().elements[0].intensity = 1.;
    output.clear();
    run(&effect, &mut output, &mut scratch).unwrap();
    assert_eq!(output, expected);
    effect.param_ids = ["rate", "lifetime", "prewarm"]
        .into_iter()
        .map(str::to_string)
        .collect();
    effect.values = [[1.; 4]; MAX_PARAMS];
    effect.scene = Some(SceneSettings::default());
    for frame in [f64::INFINITY, f64::MAX] {
        effect.local_frame = frame;
        let failure = generate(
            &scene,
            &scene.layers[0],
            &effect,
            RendererKind::Particles,
            &HashMap::new(),
            &mut output,
            &mut scratch,
        )
        .unwrap_err();
        assert!(failure.to_string().contains(if frame.is_finite() {
            "birth index exceeds signed range"
        } else {
            "frame is not finite"
        }));
        assert_eq!(output, expected);
        failures.push(failure);
    }
    failures
}
