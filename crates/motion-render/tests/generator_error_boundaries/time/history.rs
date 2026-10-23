use super::*;
use std::sync::Arc;

fn has_original_cause(error: &(dyn Error + 'static), original: &motion_model::Error) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if error
            .downcast_ref::<motion_model::Error>()
            .is_some_and(|cause| std::ptr::eq(cause, original))
        {
            return true;
        }
        current = error.source();
    }
    false
}

#[test]
fn sampled_history_failure_keeps_native_identity_until_generation_then_recovers_and_retires() {
    let (project, mut scene, effect) = world_emitter();
    let authored = serde_json::to_value(&project).unwrap();
    let mut scratch = GeneratorScratch::default();
    let mut expected = Vec::new();
    run(
        &scene,
        &effect,
        RendererKind::ParticleEmitter,
        &mut expected,
        &mut scratch,
    )
    .unwrap();
    assert!(!expected.is_empty());

    let mut malformed = project.clone();
    malformed.layers[0].effects[0]
        .scene
        .as_mut()
        .unwrap()
        .source_layer = Some(999);
    let malformed_before = serde_json::to_value(&malformed).unwrap();
    // Capture remains deferred: a sampled but unused failure is not a render refusal.
    scene
        .sample(&malformed, 17.5, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let retained = scene.effects[0].clone();
    assert!(retained.particle_history.is_none());
    let cause = retained.particle_history_error.as_ref().unwrap().clone();
    assert!(matches!(cause.as_ref(), motion_model::Error::Missing(999)));
    assert!(Arc::ptr_eq(
        &cause,
        scene.effects[0].particle_history_error.as_ref().unwrap()
    ));
    let weak = Arc::downgrade(&cause);
    let refusal =
        scene_generator::validate_settings(&retained, RendererKind::ParticleEmitter).unwrap_err();
    assert_eq!(refusal.to_string(), "layer 7, effect 11, generator settings at frame 17.5: GPU validation: object 999 does not exist");
    assert!(has_original_cause(&refusal, &cause));
    let mut output = expected.clone();
    let generated = run(
        &scene,
        &retained,
        RendererKind::ParticleEmitter,
        &mut output,
        &mut scratch,
    )
    .unwrap_err();
    assert!(has_original_cause(&generated, &cause));
    assert_eq!(generated.to_string(), "layer 7, effect 11, generator generate at frame 17.5: layer 7, effect 11, generator settings at frame 17.5: GPU validation: object 999 does not exist");
    assert_eq!(output, expected);
    assert_eq!(serde_json::to_value(&malformed).unwrap(), malformed_before);

    scene
        .sample(&project, 17.5, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert!(scene.effects[0].particle_history_error.is_none());
    assert!(scene.effects[0].particle_history.is_some());
    output.clear();
    run(
        &scene,
        &scene.effects[0],
        RendererKind::ParticleEmitter,
        &mut output,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(output, expected);
    assert_eq!(serde_json::to_value(&project).unwrap(), authored);
    assert!(Arc::ptr_eq(
        &cause,
        retained.particle_history_error.as_ref().unwrap()
    ));
    assert!(has_original_cause(&refusal, &cause));
    drop(cause);
    drop(retained);
    assert!(
        weak.upgrade().is_some(),
        "returned refusals must retain their cause"
    );
    drop(refusal);
    drop(generated);
    assert!(
        weak.upgrade().is_none(),
        "recovered scenes must not retain old failures"
    );
}

#[test]
fn cloned_history_io_cause_keeps_the_original_os_error_and_retires() {
    let (_, scene, mut effect) = world_emitter();
    // Controlled cache input exercises non-Clone native errors. Capture itself
    // currently reports model validation errors rather than filesystem IO.
    let cause = Arc::new(motion_model::Error::Io(std::io::Error::from_raw_os_error(
        2,
    )));
    let weak = Arc::downgrade(&cause);
    effect.particle_history = None;
    effect.particle_history_error = Some(cause.clone());
    let cloned = effect.clone();
    let failure =
        scene_generator::validate_settings(&cloned, RendererKind::ParticleEmitter).unwrap_err();
    assert!(has_original_cause(&failure, &cause));
    assert!(
        matches!(cause.as_ref(), motion_model::Error::Io(error) if error.raw_os_error() == Some(2))
    );
    let mut output = Vec::new();
    let generated = run(
        &scene,
        &cloned,
        RendererKind::ParticleEmitter,
        &mut output,
        &mut GeneratorScratch::default(),
    )
    .unwrap_err();
    assert!(has_original_cause(&generated, &cause));
    assert!(output.is_empty());
    drop(effect);
    drop(cloned);
    drop(cause);
    assert!(weak.upgrade().is_some());
    drop(failure);
    drop(generated);
    assert!(weak.upgrade().is_none());
}
