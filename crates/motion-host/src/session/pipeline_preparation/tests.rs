use super::*;
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, LayerTimeline};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[cfg(windows)]
mod native;

fn fixture() -> (Project, Scene) {
    let mut project = Project::new(16, 16, 30, 120).unwrap();
    let package = motion_effects::builtin::package().unwrap();
    for (index, (name, start)) in [("tint", 0), ("invert", 30)].into_iter().enumerate() {
        let mut layer = Layer::solid(
            index as u64 + 1,
            "native preparation",
            [16.; 2],
            [8., 8., 0.],
            [0.2, 0.4, 0.6, 1.],
        );
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|effect| effect.id == name)
            .unwrap();
        layer.effects.push(EffectInstance::new(
            index as u64 + 1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            layer.size,
        ));
        layer.timeline = Some(LayerTimeline {
            in_frame: start,
            out_frame: 120,
            offset_frame: 0,
        });
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &ExpressionEvaluator)
        .unwrap();
    (project, scene)
}
fn key() -> Key {
    Key {
        context: Context {
            document_generation: 9,
            project_revision: 2,
        },
        surface: 1,
        view: 0,
        width: 16,
        height: 16,
        preview: false,
        window: 0,
        exposure: None,
    }
}
fn finish(state: &mut State, renderer: &mut Renderer, project: &Project, scene: &Scene, key: Key) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if state.ensure(renderer, project, scene, None, key.clone()).unwrap() {
            return;
        }
        assert!(Instant::now() < deadline, "{}", state.status());
        std::thread::yield_now();
    }
}

#[test]
fn exposure_readiness_requires_the_exact_sample_and_instance_scope() {
    let ordinary = key();
    let mut later = ordinary.clone();
    later.window += 1;
    assert!(ordinary.same_revision(&later));
    let mut exposure = ordinary.clone();
    exposure.exposure = Some(ExposureScope {
        composition: "nested".into(), frame: 0.25f64.to_bits(), layers: vec![4, 9],
    });
    assert!(!ordinary.same_revision(&exposure));
    assert!(exposure.same_revision(&exposure));
    let mut changed = exposure.clone();
    changed.exposure.as_mut().unwrap().frame = 0.75f64.to_bits();
    assert!(!exposure.same_revision(&changed));
    changed = exposure.clone();
    changed.exposure.as_mut().unwrap().composition = "other".into();
    assert!(!exposure.same_revision(&changed));
    changed = exposure.clone();
    changed.exposure.as_mut().unwrap().layers[1] = 10;
    assert!(!exposure.same_revision(&changed));
    changed = exposure.clone();
    changed.width /= 2;
    assert!(!exposure.same_revision(&changed));
}

#[test]
fn root_transition_sampling_is_bounded_ordered_and_uses_fractional_intervals() {
    let (mut project, _) = fixture();
    project.layers[0].timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 20,
        offset_frame: 0,
    });
    assert_eq!(upcoming(&project, 0.), vec![20., 30.]);
    assert_eq!(upcoming(&project, 30.), vec![90.]);
    assert!(upcoming(&project, 119.).is_empty());
    project.layers[1].subframe_range = Some(motion_core::ClipRange {
        in_frame: 30.5,
        out_frame: 35.25,
    });
    assert_eq!(upcoming(&project, 20.), vec![30.5, 35.25]);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn real_host_worker_prepares_current_and_upcoming_pixels_without_foreground_compilation() {
    let (project, mut scene) = fixture();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut state = State::default();
    assert!(!state
        .ensure(&mut renderer, &project, &scene, None, key())
        .unwrap());
    assert_eq!(state.status()["preparing"], true);
    finish(&mut state, &mut renderer, &project, &scene, key());
    assert_eq!(state.status()["preparing"], false);
    let builds = renderer.effect_pipeline_builds();
    assert!(builds > 0);
    let target = renderer.capture_target(16, 16).unwrap();
    let current = renderer.capture(&scene, &target).unwrap().0;
    scene
        .sample(&project, 30., None, &ExpressionEvaluator)
        .unwrap();
    let future = renderer.capture(&scene, &target).unwrap().0;
    assert_eq!(
        renderer.effect_pipeline_builds(),
        builds,
        "current or upcoming capture compiled in foreground"
    );
    assert_ne!(current, future);
    let mut cold = pollster::block_on(Renderer::headless()).unwrap();
    let target = cold.capture_target(16, 16).unwrap();
    assert_eq!(cold.capture(&scene, &target).unwrap().0, future);
    let worker = state.worker.as_ref().unwrap().thread.thread().id();
    assert_ne!(worker, std::thread::current().id());
    let mut next = key();
    next.window = 1;
    assert!(
        state
            .ensure(&mut renderer, &project, &scene, None, next)
            .unwrap(),
        "later lookahead must not block an installed revision"
    );
    assert_eq!(state.worker.as_ref().unwrap().thread.thread().id(), worker);
    state.cancel();
    assert!(state.key.is_none());
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn blocked_host_worker_retains_only_latest_pending_request_and_never_installs_old_revision() {
    let (project, scene) = fixture();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut state = State::default();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    state.before_sample = Some(Box::new(move || {
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }));
    assert!(!state
        .ensure(&mut renderer, &project, &scene, None, key())
        .unwrap());
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let old = state.ticket.as_ref().unwrap().clone();
    assert_eq!(
        old.status().phase,
        motion_render::PipelinePreparationPhase::Planning
    );
    let worker = state.worker.as_ref().unwrap().thread.thread().id();
    let began = Instant::now();
    let mut newest = key();
    for revision in 3..35 {
        newest.context.project_revision = revision;
        assert!(!state
            .ensure(&mut renderer, &project, &scene, None, newest.clone())
            .unwrap());
    }
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "enqueue waited for the blocked compiler"
    );
    assert!(old.status().cancelled);
    assert!(state.pending.is_some());
    assert_eq!(renderer.effect_pipeline_cache_info().previous_entries, 0);
    assert_eq!(state.worker.as_ref().unwrap().thread.thread().id(), worker);
    release_tx.send(()).unwrap();
    finish(&mut state, &mut renderer, &project, &scene, newest.clone());
    assert_eq!(
        state.ticket.as_ref().unwrap().status().context,
        newest.context
    );
    assert!(!state.ticket.as_ref().unwrap().status().cancelled);
    assert_eq!(state.status()["pendingEnqueue"], false);
    assert!(renderer.effect_pipeline_cache_info().previous_entries > 0);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unavailable_future_effect_retains_its_failure_without_rejecting_current_pixels() {
    let (mut project, scene) = fixture();
    project.layers[1].effects[0].hash = "0".repeat(64);
    project.rebuild_plugin_dependencies();
    let mut future = scene.clone();
    future
        .sample(&project, 30., None, &ExpressionEvaluator)
        .unwrap();
    let mut cold = pollster::block_on(Renderer::headless()).unwrap();
    let expected = cold
        .preflight_effects(&future, 16, 16)
        .unwrap_err()
        .to_string();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut state = State::default();
    finish(&mut state, &mut renderer, &project, &scene, key());
    assert_eq!(state.status()["preparing"], false);
    assert_eq!(state.lookahead_failures.len(), 1);
    assert_eq!(state.lookahead_failures[0].frame, 30.);
    assert_eq!(state.lookahead_failures[0].cause.to_string(), expected);
    let target = renderer.capture_target(16, 16).unwrap();
    assert!(renderer
        .capture(&scene, &target)
        .unwrap()
        .0
        .chunks_exact(4)
        .any(|pixel| pixel[3] != 0));
    assert_eq!(
        renderer.capture(&future, &target).unwrap_err().to_string(),
        expected
    );
    assert!(renderer
        .capture(&scene, &target)
        .unwrap()
        .0
        .chunks_exact(4)
        .any(|pixel| pixel[3] != 0));
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn worker_failure_is_retained_without_poll_retries_and_explicit_revision_recovers() {
    let (project, scene) = fixture();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut state = State::default();
    state.before_sample = Some(Box::new(|| panic!("controlled host sampler panic")));
    assert!(!state
        .ensure(&mut renderer, &project, &scene, None, key())
        .unwrap());
    let ticket = state.ticket.as_ref().unwrap().clone();
    let worker = state.worker.as_ref().unwrap().thread.thread().id();
    let deadline = Instant::now() + Duration::from_secs(60);
    let error = loop {
        match state.ensure(&mut renderer, &project, &scene, None, key()) {
            Err(error) => break error,
            Ok(ready) => assert!(!ready, "panicked work was reported ready"),
        }
        assert!(Instant::now() < deadline, "{}", state.status());
        std::thread::yield_now();
    };
    let crate::Error::PipelinePreparation(original) = error else {
        panic!("native worker error was flattened")
    };
    assert!(
        matches!(original.as_ref(),Error::Render(motion_render::RenderError::PipelinePreparation(
        motion_render::PipelinePreparationError::Panicked {reason})) if reason=="controlled host sampler panic")
    );
    for _ in 0..65 {
        let error = state
            .ensure(&mut renderer, &project, &scene, None, key())
            .unwrap_err();
        let crate::Error::PipelinePreparation(repeated) = error else {
            panic!("worker failure identity was lost")
        };
        assert!(Arc::ptr_eq(&original, &repeated));
        assert!(state.ticket.as_ref().unwrap().same_request(&ticket));
    }
    assert_eq!(renderer.effect_pipeline_builds(), 0);
    let mut next = key();
    next.context.project_revision += 1;
    finish(&mut state, &mut renderer, &project, &scene, next);
    assert_eq!(state.worker.as_ref().unwrap().thread.thread().id(), worker);
    assert!(renderer.effect_pipeline_builds() > 0);
    assert!(!state.ticket.as_ref().unwrap().same_request(&ticket));
    renderer.check_health().unwrap();
}
