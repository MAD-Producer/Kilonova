use super::*;
use scene_generator::{AlphaImage, GeneratorScratch};

fn render(
    scene: &Scene,
    effect: &SampledEffect,
    output: &mut Vec<scene_generator::Sprite>,
) -> Result<(), RenderError> {
    let alpha = HashMap::from([(
        99,
        AlphaImage {
            width: 2,
            height: 2,
            pixels: vec![0, 64, 128, 255],
        },
    )]);
    scene_generator::generate(
        scene,
        &scene.layers[0],
        effect,
        RendererKind::LensFlare,
        &alpha,
        output,
        &mut GeneratorScratch::default(),
    )
    .map(|_| ())
}

#[test]
fn every_used_lens_component_rejects_nonfinite_input_and_recovers_original_output() {
    let (scene, mut effect) = occluded_scene();
    effect.values[2][0] = 1.; // Exercise reference_distance as well.
    let values = effect.values;
    let mut expected = Vec::new();
    render(&scene, &effect, &mut expected).unwrap();
    assert!(!expected.is_empty());
    for (index, components) in [(0, 3), (1, 1), (2, 1), (3, 1), (4, 1), (5, 1)] {
        for component in 0..components {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                effect.values = values;
                effect.values[index][component] = bad;
                let input = effect.values.map(|v| v.map(f32::to_bits));
                let mut output = expected.clone();
                let error = render(&scene, &effect, &mut output).unwrap_err();
                for text in [
                    "non-finite components",
                    &effect.param_ids[index],
                    "layer 7",
                    "effect 11",
                    "frame 5.5",
                ] {
                    assert!(error.to_string().contains(text), "{error}");
                }
                assert!(matches!(&error, RenderError::Context { cause, .. }
                    if matches!(cause.as_ref(), RenderError::Invalid(_))));
                assert_eq!(effect.values.map(|v| v.map(f32::to_bits)), input);
                assert_eq!(output, expected);
                effect.values = values;
                let mut recovered = Vec::new();
                render(&scene, &effect, &mut recovered).unwrap();
                assert_eq!(recovered, expected);
            }
        }
    }
}

#[test]
fn particle_vector_color_and_scalar_queries_reject_only_their_used_components() {
    let (_, scene, _) = sampled();
    let mut effect = scene.effects[0].clone();
    let values = effect.values;
    let mut expected = Vec::new();
    let mut scratch = GeneratorScratch::default();
    let run = |effect: &SampledEffect, output: &mut Vec<_>, scratch: &mut GeneratorScratch| {
        scene_generator::generate(
            &scene,
            &scene.layers[0],
            effect,
            RendererKind::Particles,
            &HashMap::new(),
            output,
            scratch,
        )
    };
    run(&effect, &mut expected, &mut scratch).unwrap();
    assert!(!expected.is_empty());
    for (name, components) in [
        ("rate", 1),
        ("lifetime", 1),
        ("color", 4),
        ("end_color", 4),
        ("gravity", 3),
        ("extent", 3),
        ("speed", 1),
        ("spread", 1),
        ("shape", 1),
        ("size", 1),
        ("end_size", 1),
        ("fade", 1),
        ("prewarm", 1),
    ] {
        let index = effect.param_ids.iter().position(|id| id == name).unwrap();
        for component in 0..components {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                effect.values = values;
                effect.values[index][component] = bad;
                let input = effect.values.map(|v| v.map(f32::to_bits));
                let mut output = expected.clone();
                let error = run(&effect, &mut output, &mut scratch).unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains(&format!("parameter {name} contains non-finite components")),
                    "{error}"
                );
                assert_eq!(output, expected);
                assert_eq!(effect.values.map(|v| v.map(f32::to_bits)), input);
            }
        }
        effect.values = values;
        for component in components..4 {
            effect.values[index][component] = f32::NAN;
        }
        let mut recovered = Vec::new();
        run(&effect, &mut recovered, &mut scratch).unwrap();
        assert_eq!(recovered, expected, "unused components of {name}");
    }
}

#[test]
fn unused_lens_components_keep_their_previous_behavior() {
    let (scene, mut effect) = occluded_scene();
    let mut expected = Vec::new();
    render(&scene, &effect, &mut expected).unwrap();
    assert!(!expected.is_empty());
    for (index, components) in [(0, 3), (1, 1), (2, 1), (3, 1), (4, 1), (5, 1)] {
        for component in components..4 {
            effect.values[index][component] = f32::NAN;
        }
    }
    let mut output = Vec::new();
    render(&scene, &effect, &mut output).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn finite_inputs_cannot_publish_overflowing_sprites_and_the_prior_cause_wins() {
    let (scene, mut effect) = occluded_scene();
    let mut expected = Vec::new();
    render(&scene, &effect, &mut expected).unwrap();
    assert!(!expected.is_empty());
    let values = effect.values;
    let settings = effect.scene.clone();
    for index in [1, 5] {
        effect.values = values;
        effect.scene = settings.clone();
        effect.values[index][0] = f32::MAX;
        if index == 1 {
            effect.scene.as_mut().unwrap().elements[0].intensity = 32.;
        } else {
            effect.scene.as_mut().unwrap().elements[0].size[0] = 8192.;
        }
        let mut output = expected.clone();
        let error = render(&scene, &effect, &mut output).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("generated sprite exceeds numeric range"),
            "{error}"
        );
        assert_eq!(output, expected);
    }
    effect.scene.as_mut().unwrap().source_layer = Some(0);
    let mut output = expected.clone();
    let error = render(&scene, &effect, &mut output).unwrap_err();
    assert!(native_effects_error(&error).is_some());
    assert!(!error
        .to_string()
        .contains("generated sprite exceeds numeric range"));
    assert_eq!(output, expected);
    effect.values = values;
    effect.scene = settings;
    output.clear();
    render(&scene, &effect, &mut output).unwrap();
    assert_eq!(output, expected);
}
