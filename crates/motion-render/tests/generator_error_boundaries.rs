#[path = "../../motion-effects/tests/support/scene_packages.rs"]
mod scene_fixtures;

use motion_core::{EffectInstance, Layer, Project, SampledEffect};
use motion_effects::{LensElement, LensShape, RendererKind, SceneSettings, MAX_PARAMS};
use motion_render::{effect_plan::PlanBuilder, scene_generator, RenderError, Scene};
use std::{collections::HashMap, error::Error};

fn effect() -> SampledEffect {
    SampledEffect {
        layer: 7,
        local_frame: 5.5,
        instance: 11,
        plugin: "probe".into(),
        effect: "probe".into(),
        version: "1".into(),
        hash: String::new(),
        enabled: true,
        seed: 1,
        image_input: None,
        param_ids: vec!["rate".into(), "lifetime".into()],
        values: [[1., 0., 0., 0.]; MAX_PARAMS],
        lut: None,
        scene: Some(SceneSettings::default()),
        particle_history: None,
        particle_history_error: None,
    }
}

fn native_effects_error<'a>(error: &'a (dyn Error + 'static)) -> Option<&'a motion_effects::Error> {
    if let Some(error) = error.downcast_ref() {
        return Some(error);
    }
    error.source().and_then(native_effects_error)
}

fn sampled() -> (Project, Scene, PlanBuilder) {
    let package = scene_fixtures::scene_package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "starfield")
        .unwrap();
    let mut project = Project::new(128, 128, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Generator", [128., 128.], [64., 64., 0.], [0.; 4]);
    layer.effects.push(EffectInstance::new(
        11,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [128., 128.],
    ));
    let parameters = &mut layer.effects[0].params;
    for (name, value) in [
        ("rate", 3.),
        ("lifetime", 2.),
        ("speed", 0.),
        ("spread", 0.),
        ("fade", 0.),
        ("prewarm", 0.),
        ("size", 8.),
        ("end_size", 8.),
    ] {
        parameters.get_mut(name).unwrap().track.value = [value, 0., 0., 0.];
    }
    parameters.get_mut("extent").unwrap().track.value = [0.; 4];
    parameters.get_mut("gravity").unwrap().track.value = [0.; 4];
    parameters.get_mut("color").unwrap().track.value = [1.; 4];
    parameters.get_mut("end_color").unwrap().track.value = [1.; 4];
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 17.5, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let builder = PlanBuilder::new(scene_fixtures::registry().unwrap()).unwrap();
    (project, scene, builder)
}

#[test]
fn public_generator_parameter_indices_return_context_instead_of_panicking() {
    let healthy = effect();
    scene_generator::validate_settings(&healthy, RendererKind::Particles).unwrap();
    for index in MAX_PARAMS..MAX_PARAMS + 4 {
        let mut malformed = effect();
        malformed.param_ids = vec!["unused".into(); index];
        malformed.param_ids.push("rate".into());
        let error =
            scene_generator::validate_settings(&malformed, RendererKind::Particles).unwrap_err();
        assert!(matches!(&error, RenderError::Context { cause, .. }
            if matches!(cause.as_ref(), RenderError::Invalid(_))));
        let message = error.to_string();
        for expected in [
            "layer 7",
            "effect 11",
            "settings",
            "frame 5.5",
            "parameter rate",
            &format!("index {index}"),
        ] {
            assert!(message.contains(expected), "{message}");
        }
        assert_eq!(malformed.param_ids.len(), index + 1);
        assert_eq!(malformed.values, healthy.values);
        scene_generator::validate_settings(&healthy, RendererKind::Particles).unwrap();
    }
}

#[test]
fn planner_keeps_the_native_settings_cause_and_recovers_identical_sprites() {
    let (project, mut scene, mut builder) = sampled();
    let before = serde_json::to_value(&project).unwrap();
    builder.build(&scene, &[0], 128, 128, true).unwrap();
    let expected = builder.frame.sprites.clone();
    assert!(!expected.is_empty());
    let settings = scene.effects[0].scene.clone();
    scene.effects[0].scene.as_mut().unwrap().source_layer = Some(0);
    let error = builder.build(&scene, &[0], 128, 128, true).unwrap_err();
    assert!(matches!(
        native_effects_error(&error),
        Some(motion_effects::Error::Invalid(_))
    ));
    let message = error.to_string();
    for expected in ["layer 7", "effect 11", "frame 17.5", "invalid scene source"] {
        assert!(message.contains(expected), "{message}");
    }
    assert_eq!(
        scene.effects[0].scene.as_ref().unwrap().source_layer,
        Some(0)
    );
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    scene.effects[0].scene = settings;
    builder.build(&scene, &[0], 128, 128, true).unwrap();
    assert_eq!(builder.frame.sprites, expected);
}

#[test]
fn generator_failure_retains_prior_output_and_recovers_the_exact_generated_sprites() {
    let (_, mut scene, _) = sampled();
    let layer = &scene.layers[0];
    let mut scratch = scene_generator::GeneratorScratch::default();
    let alpha = HashMap::new();
    let mut output = Vec::new();
    scene_generator::generate(
        &scene,
        layer,
        &scene.effects[0],
        RendererKind::Particles,
        &alpha,
        &mut output,
        &mut scratch,
    )
    .unwrap();
    let expected = output.clone();
    assert!(!expected.is_empty());
    scene.effects[0].scene.as_mut().unwrap().source_layer = Some(0);
    let error = scene_generator::generate(
        &scene,
        &scene.layers[0],
        &scene.effects[0],
        RendererKind::Particles,
        &alpha,
        &mut output,
        &mut scratch,
    )
    .unwrap_err();
    assert!(native_effects_error(&error).is_some());
    assert!(error
        .to_string()
        .contains("generator generate at frame 17.5"));
    assert_eq!(output, expected);
    scene.effects[0].scene.as_mut().unwrap().source_layer = None;
    output.clear();
    scene_generator::generate(
        &scene,
        &scene.layers[0],
        &scene.effects[0],
        RendererKind::Particles,
        &alpha,
        &mut output,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(output, expected);
}

fn occluded_scene() -> (Scene, SampledEffect) {
    let mut project = Project::new(128, 128, 30, 120).unwrap();
    for id in [7, 8] {
        project.layers.push(Layer::solid(
            id,
            "Occlusion",
            [128.; 2],
            [64., 64., 0.],
            [1.; 4],
        ));
    }
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let blocker = scene.layers.iter_mut().find(|layer| layer.id == 8).unwrap();
    blocker.model.w_axis.z = 0.5;
    blocker.asset = Some(99);
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
        occlusion: true,
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
    (scene, effect)
}

#[test]
fn malformed_occlusion_images_return_native_errors_keep_output_and_recover_identical_sprites() {
    use scene_generator::{AlphaImage, GeneratorScratch};
    let (scene, effect) = occluded_scene();
    let layer = scene.layers.iter().find(|layer| layer.id == 7).unwrap();
    let mut alpha = HashMap::from([(
        99,
        AlphaImage {
            width: 2,
            height: 2,
            pixels: vec![0, 64, 128, 255],
        },
    )]);
    let mut output = Vec::new();
    let mut scratch = GeneratorScratch::default();
    let run =
        |alpha: &HashMap<u64, AlphaImage>, output: &mut Vec<_>, scratch: &mut GeneratorScratch| {
            scene_generator::generate(
                &scene,
                layer,
                &effect,
                RendererKind::LensFlare,
                alpha,
                output,
                scratch,
            )
        };
    run(&alpha, &mut output, &mut scratch).unwrap();
    let expected = output.clone();
    assert!(!expected.is_empty());
    for (width, height, pixels) in [
        (2, 2, vec![]),
        (2, 2, vec![255]),
        (0, 2, vec![]),
        (2, 0, vec![]),
        (u32::MAX, u32::MAX, vec![255]),
    ] {
        alpha.insert(
            99,
            AlphaImage {
                width,
                height,
                pixels: pixels.clone(),
            },
        );
        let error = run(&alpha, &mut output, &mut scratch).unwrap_err();
        assert!(matches!(&error, RenderError::Context { cause, .. }
            if matches!(cause.as_ref(), RenderError::Context { cause, .. }
                if matches!(cause.as_ref(), RenderError::Invalid(_)))));
        let message = error.to_string();
        for context in [
            "layer 7",
            "effect 11",
            "generator generate",
            "frame 5.5",
            "asset 99",
            "alpha",
        ] {
            assert!(message.contains(context), "{message}");
        }
        assert_eq!(output, expected);
        assert_eq!(alpha[&99].pixels, pixels);
        assert_eq!((alpha[&99].width, alpha[&99].height), (width, height));
        alpha.insert(
            99,
            AlphaImage {
                width: 2,
                height: 2,
                pixels: vec![0, 64, 128, 255],
            },
        );
        let mut recovered = Vec::new();
        run(&alpha, &mut recovered, &mut scratch).unwrap();
        assert_eq!(recovered, expected);
    }
    // Extra backing bytes were already accepted; they do not change visible pixels.
    alpha
        .get_mut(&99)
        .unwrap()
        .pixels
        .extend_from_slice(&[17, 29]);
    output.clear();
    run(&alpha, &mut output, &mut scratch).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn nonfinite_occlusion_coordinates_fail_but_unintersected_sources_remain_normal_misses() {
    use scene_generator::{AlphaImage, GeneratorScratch};
    let (mut scene, effect) = occluded_scene();
    let mut alpha = HashMap::from([(
        99,
        AlphaImage {
            width: 2,
            height: 2,
            pixels: vec![0, 64, 128, 255],
        },
    )]);
    let mut output = Vec::new();
    let mut scratch = GeneratorScratch::default();
    scene_generator::generate(
        &scene,
        &scene.layers[0],
        &effect,
        RendererKind::LensFlare,
        &alpha,
        &mut output,
        &mut scratch,
    )
    .unwrap();
    let expected = output.clone();
    assert!(!expected.is_empty());
    let blocker = scene.layers.iter_mut().find(|layer| layer.id == 8).unwrap();
    let size = blocker.size;
    blocker.size[0] = 0.;
    let error = scene_generator::generate(
        &scene,
        &scene.layers[0],
        &effect,
        RendererKind::LensFlare,
        &alpha,
        &mut output,
        &mut scratch,
    )
    .unwrap_err();
    for context in [
        "asset 99",
        "coordinates are not finite",
        "layer 7",
        "frame 5.5",
    ] {
        assert!(error.to_string().contains(context), "{error}");
    }
    assert_eq!(output, expected);
    let blocker = scene.layers.iter_mut().find(|layer| layer.id == 8).unwrap();
    blocker.size = size;
    output.clear();
    scene_generator::generate(
        &scene,
        &scene.layers[0],
        &effect,
        RendererKind::LensFlare,
        &alpha,
        &mut output,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(output, expected);
    let blocker = scene.layers.iter_mut().find(|layer| layer.id == 8).unwrap();
    blocker.model.w_axis.x = 10000.;
    alpha.clear();
    output.clear();
    scene_generator::generate(
        &scene,
        &scene.layers[0],
        &effect,
        RendererKind::LensFlare,
        &alpha,
        &mut output,
        &mut scratch,
    )
    .unwrap();
    assert!(!output.is_empty());
}
