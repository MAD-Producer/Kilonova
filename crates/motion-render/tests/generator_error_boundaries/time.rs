use super::*;
use scene_generator::{GeneratorScratch, Sprite};

#[path = "time/history.rs"]
mod history;

fn world_emitter() -> (Project, Scene, SampledEffect) {
    let package = scene_fixtures::particle_package().unwrap();
    let mut project = Project::new(128, 128, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "World emitter", [128.; 2], [64., 64., 0.], [0.; 4]);
    let mut effect = EffectInstance::new(
        11,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [128.; 2],
    );
    for (id, value) in [
        ("rate", 3.),
        ("lifetime", 2.),
        ("speed", 0.),
        ("spread", 0.),
        ("inherit_velocity", 0.),
        ("drag", 0.),
        ("fade", 0.),
        ("size", 8.),
        ("end_size", 8.),
    ] {
        effect.params.get_mut(id).unwrap().track.value = [value, 0., 0., 0.];
    }
    for id in ["extent", "gravity", "wind"] {
        effect.params.get_mut(id).unwrap().track.value = [0.; 4];
    }
    for id in ["color", "end_color"] {
        effect.params.get_mut(id).unwrap().track.value = [1.; 4];
    }
    layer.effects.push(effect);
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 17.5, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let effect = scene.effects[0].clone();
    assert!(effect.particle_history.is_some());
    (project, scene, effect)
}

fn run(
    scene: &Scene,
    effect: &SampledEffect,
    kind: RendererKind,
    output: &mut Vec<Sprite>,
    scratch: &mut GeneratorScratch,
) -> Result<(), RenderError> {
    scene_generator::generate(
        scene,
        &scene.layers[0],
        effect,
        kind,
        &HashMap::new(),
        output,
        scratch,
    )
    .map(|_| ())
}

#[test]
fn both_particle_paths_reject_extreme_time_without_panicking_and_restore_the_same_sprites() {
    let (_, legacy, _) = sampled();
    let legacy_effect = legacy.effects[0].clone();
    let (_, world, world_effect) = world_emitter();
    for (mut scene, mut effect, kind) in [
        (legacy, legacy_effect, RendererKind::Particles),
        (world, world_effect, RendererKind::ParticleEmitter),
    ] {
        let mut scratch = GeneratorScratch::default();
        let frame = effect.local_frame;
        let mut expected = Vec::new();
        run(&scene, &effect, kind, &mut expected, &mut scratch).unwrap();
        assert!(!expected.is_empty());
        for bad in [f64::MAX, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            effect.local_frame = bad;
            let mut output = expected.clone();
            let failure = run(&scene, &effect, kind, &mut output, &mut scratch).unwrap_err();
            assert!(
                failure.to_string().contains(if bad.is_finite() {
                    "birth index exceeds signed range"
                } else {
                    "frame is not finite"
                }),
                "{failure}"
            );
            assert_eq!(effect.local_frame.to_bits(), bad.to_bits());
            assert_eq!(output, expected);
            effect.local_frame = frame;
            let mut recovered = Vec::new();
            run(&scene, &effect, kind, &mut recovered, &mut scratch).unwrap();
            assert_eq!(recovered, expected);
        }
        let fps = scene.fps;
        scene.fps = 0;
        let mut output = expected.clone();
        let failure = run(&scene, &effect, kind, &mut output, &mut scratch).unwrap_err();
        assert!(failure.to_string().contains("frame rate must be positive"));
        assert_eq!(output, expected);
        scene.fps = fps;
        effect.local_frame = -1.;
        output.clear();
        run(&scene, &effect, kind, &mut output, &mut scratch).unwrap();
        assert!(output.is_empty());
        effect.local_frame = frame;
        run(&scene, &effect, kind, &mut output, &mut scratch).unwrap();
        assert_eq!(output, expected);
    }
}

fn model_cause<'a>(error: &'a (dyn Error + 'static)) -> Option<&'a motion_model::Error> {
    if let Some(error) = error.downcast_ref() {
        return Some(error);
    }
    error.source().and_then(model_cause)
}

#[test]
fn actual_history_parameter_failure_retains_native_model_identity_and_recovers_its_cache() {
    let (project, scene, mut effect) = world_emitter();
    let before = serde_json::to_value(&project).unwrap();
    let mut scratch = GeneratorScratch::default();
    let mut output = Vec::new();
    run(
        &scene,
        &effect,
        RendererKind::ParticleEmitter,
        &mut output,
        &mut scratch,
    )
    .unwrap();
    let expected = output.clone();
    assert!(!expected.is_empty());
    let healthy = effect.particle_history.clone();
    let mut malformed = project.layers[0].effects[0].clone();
    malformed.params.remove("gravity");
    effect.particle_history = Some(
        motion_model::particle_history::ParticleHistory::capture(
            &project,
            &project.layers[0],
            &malformed,
            None,
        )
        .unwrap(),
    );
    let failure = run(
        &scene,
        &effect,
        RendererKind::ParticleEmitter,
        &mut output,
        &mut scratch,
    )
    .unwrap_err();
    assert!(
        matches!(model_cause(&failure), Some(motion_model::Error::Invalid(message)) if message.contains("particle parameter gravity missing"))
    );
    assert!(failure
        .to_string()
        .contains("layer 7, effect 11, generator generate at frame 17.5"));
    assert_eq!(output, expected);
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    assert!(!malformed.params.contains_key("gravity"));
    effect.particle_history = healthy;
    output.clear();
    run(
        &scene,
        &effect,
        RendererKind::ParticleEmitter,
        &mut output,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(output, expected);
}
