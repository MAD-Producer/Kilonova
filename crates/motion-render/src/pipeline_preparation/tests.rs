use super::*;
use crate::{PipelinePreparationInstall, Renderer};
use std::sync::mpsc;
use std::time::Duration;

const CONTEXT: PipelinePreparationContext = PipelinePreparationContext {
    document_generation: 7,
    project_revision: 11,
};

fn scene() -> Scene {
    let package = motion_effects::builtin::package().unwrap();
    let mut project = motion_model::Project::new(16, 16, 30, 60).unwrap();
    let mut layer = motion_model::Layer::solid(
        1,
        "preparation",
        [16.; 2],
        [8., 8., 0.],
        [0.2, 0.5, 0.8, 0.5],
    );
    for (index, name) in ["invert", "tint"].into_iter().enumerate() {
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|x| x.id == name)
            .unwrap();
        layer.effects.push(motion_model::EffectInstance::new(
            index as u64 + 1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            layer.size,
        ));
    }
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0.25, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

fn run(job: PipelinePreparationJob) -> Result<PreparedEffectPipelines, RenderError> {
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let _ = tx.send(job.run());
    });
    let result = rx
        .recv_timeout(Duration::from_secs(60))
        .expect("native preparation did not return");
    worker
        .join()
        .expect("preparation escaped its panic boundary");
    result
}

#[test]
fn saturated_request_ordinals_never_authorize_a_superseded_ticket() {
    let mut state = PreparationState::default();
    state.next_request = u64::MAX - 1;
    let first = state.begin(CONTEXT);
    assert!(state.accepts(&first));
    let second = state.begin(CONTEXT);
    assert_eq!(first.status().request, second.status().request);
    assert!(first.status().request_saturated && second.status().request_saturated);
    assert!(first.cancelled());
    assert!(!state.accepts(&first));
    assert!(state.accepts(&second));
    drop(state);
    assert!(second.cancelled());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn worker_preparation_preserves_pixels_and_avoids_foreground_native_compiles() {
    let input = scene();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_pointwise_fusion(true);
    let job = renderer
        .prepare_effect_pipeline_job(vec![input], 16, 16, CONTEXT)
        .unwrap();
    let ticket = job.ticket();
    assert_eq!(ticket.status().phase, PipelinePreparationPhase::Queued);
    let prepared = run(job).unwrap();
    let prepared_count = prepared.pipeline_count();
    assert!(prepared_count > 0 && prepared_count <= MAX_PREPARED_PIPELINES);
    assert!(prepared.wgsl_bytes() > 0 && prepared.wgsl_bytes() <= MAX_PREPARED_WGSL_BYTES);
    assert!(prepared.native_compile_nanos() > 0);
    assert_eq!(ticket.status().phase, PipelinePreparationPhase::Ready);
    assert_eq!(ticket.status().compiled_pipelines, prepared_count as u64);
    assert_eq!(renderer.effect_pipeline_builds(), prepared_count as u64);
    assert_eq!(
        renderer
            .install_prepared_effect_pipelines(prepared, CONTEXT)
            .unwrap(),
        PipelinePreparationInstall::Installed {
            pipelines: prepared_count,
            limited: false
        }
    );
    assert_eq!(ticket.status().phase, PipelinePreparationPhase::Installed);
    assert!(!ticket.cancelled());
    let builds = renderer.effect_pipeline_builds();
    let reuses = renderer.effect_pipeline_cache_info().reuses;
    let input = scene();
    let target = renderer.capture_target(16, 16).unwrap();
    let pixels = renderer.capture(&input, &target).unwrap().0;
    assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
    assert_eq!(
        renderer.effect_pipeline_builds(),
        builds,
        "foreground capture compiled a supposedly warmed pipeline"
    );
    assert!(renderer.effect_pipeline_cache_info().reuses > reuses);
    let mut reference = pollster::block_on(Renderer::headless()).unwrap();
    reference.set_pointwise_fusion(true);
    let target = reference.capture_target(16, 16).unwrap();
    assert_eq!(reference.capture(&input, &target).unwrap().0, pixels);
    assert!(reference.effect_pipeline_builds() > 0);
    renderer.check_health().unwrap();
    reference.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn owner_epoch_foreign_and_dropped_jobs_cannot_install_pipelines() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    let owner_ticket = job.ticket();
    assert!(matches!(
        job.run(),
        Err(RenderError::PipelinePreparation(
            PipelinePreparationError::OwnerThread
        ))
    ));
    assert_eq!(
        owner_ticket.status().phase,
        PipelinePreparationPhase::Failed
    );

    let job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    let dropped = job.ticket();
    drop(job);
    assert!(dropped.cancelled());
    assert_eq!(dropped.status().phase, PipelinePreparationPhase::Cancelled);

    let old_job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    let old = run(old_job).unwrap();
    let old_ticket = old.ticket.clone();
    let new_job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    assert!(old_ticket.cancelled());
    assert_eq!(
        renderer
            .install_prepared_effect_pipelines(old, CONTEXT)
            .unwrap(),
        PipelinePreparationInstall::Superseded
    );
    assert_eq!(renderer.effect_pipeline_cache_info().previous_entries, 0);
    drop(new_job);

    let job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    let prepared = run(job).unwrap();
    let wrong_revision = PipelinePreparationContext {
        project_revision: CONTEXT.project_revision + 1,
        ..CONTEXT
    };
    assert_eq!(
        renderer
            .install_prepared_effect_pipelines(prepared, wrong_revision)
            .unwrap(),
        PipelinePreparationInstall::Superseded
    );
    assert_eq!(renderer.effect_pipeline_cache_info().previous_entries, 0);

    let job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    let prepared = run(job).unwrap();
    let ticket = prepared.ticket.clone();
    drop(prepared);
    assert!(ticket.cancelled());
    assert_eq!(ticket.status().phase, PipelinePreparationPhase::Cancelled);

    let job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    let prepared = run(job).unwrap();
    let mut foreign = pollster::block_on(Renderer::headless()).unwrap();
    assert!(matches!(
        foreign.install_prepared_effect_pipelines(prepared, CONTEXT),
        Err(RenderError::PipelinePreparation(
            PipelinePreparationError::ForeignRenderer
        ))
    ));
    assert_eq!(foreign.effect_pipeline_cache_info().previous_entries, 0);
    assert_eq!(renderer.effect_pipeline_cache_info().previous_entries, 0);
    renderer.check_health().unwrap();
    foreign.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn planning_state_precedes_blocking_and_cancelled_work_never_reaches_gpu() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    let ticket = job.ticket();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    job.before_plan = Some(Box::new(move || {
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }));
    let worker = std::thread::spawn(move || job.run());
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(ticket.status().phase, PipelinePreparationPhase::Planning);
    assert_eq!(ticket.status().context, CONTEXT);
    assert_eq!(ticket.status().compiled_pipelines, 0);
    renderer.cancel_effect_pipeline_preparation();
    release_tx.send(()).unwrap();
    let prepared = worker.join().unwrap().unwrap();
    assert!(prepared.cancelled());
    assert_eq!(prepared.pipeline_count(), 0);
    assert_eq!(ticket.status().phase, PipelinePreparationPhase::Cancelled);
    assert_eq!(renderer.effect_pipeline_builds(), 0);
    assert_eq!(
        renderer
            .install_prepared_effect_pipelines(prepared, CONTEXT)
            .unwrap(),
        PipelinePreparationInstall::Superseded
    );

    let mut job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    let ticket = job.ticket();
    job.before_plan = Some(Box::new(|| panic!("controlled planning panic")));
    let failure = match run(job) {
        Err(error) => error,
        Ok(_) => panic!("planning panic was hidden"),
    };
    assert!(
        matches!(&failure, RenderError::PipelinePreparation(PipelinePreparationError::Panicked { reason }) if reason == "controlled planning panic")
    );
    assert_eq!(ticket.status().phase, PipelinePreparationPhase::Failed);
    assert_eq!(renderer.effect_pipeline_builds(), 0);
    // A new request still performs real GPU compilation and can install.
    let job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    let prepared = run(job).unwrap();
    assert!(prepared.pipeline_count() > 0);
    assert!(matches!(
        renderer
            .install_prepared_effect_pipelines(prepared, CONTEXT)
            .unwrap(),
        PipelinePreparationInstall::Installed { .. }
    ));
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn admission_rejects_unbounded_inputs_without_cancelling_valid_work() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let job = renderer
        .prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT)
        .unwrap();
    let ticket = job.ticket();
    assert!(matches!(
        renderer.prepare_effect_pipeline_job(Vec::new(), 16, 16, CONTEXT),
        Err(RenderError::PipelinePreparation(
            PipelinePreparationError::SceneCount
        ))
    ));
    assert!(matches!(
        renderer.prepare_effect_pipeline_job((0..4).map(|_| scene()).collect(), 16, 16, CONTEXT),
        Err(RenderError::PipelinePreparation(
            PipelinePreparationError::SceneCount
        ))
    ));
    assert!(!ticket.cancelled());
    // The renderer normally owns a white fallback alpha image as well. Keep
    // this boundary fixture to exactly one controlled allocation.
    renderer.effect_gpu.builder.alpha_images.clear();
    renderer.effect_gpu.builder.alpha_images.insert(
        777,
        crate::scene_generator::AlphaImage {
            width: 1,
            height: 1,
            pixels: vec![0; MAX_ALPHA_SNAPSHOT_BYTES + 1],
        },
    );
    assert!(
        matches!(renderer.prepare_effect_pipeline_job(vec![scene()], 16, 16, CONTEXT),
        Err(RenderError::PipelinePreparation(PipelinePreparationError::SnapshotBudget { bytes }) ) if bytes == MAX_ALPHA_SNAPSHOT_BYTES + 1)
    );
    assert!(!ticket.cancelled());
    renderer.effect_gpu.builder.alpha_images.clear();
    let mut prepared = run(job).unwrap();
    assert!(prepared.pipeline_count() > 0);
    let first = &prepared.entries[0];
    let pipeline = first.pipeline.clone();
    let format = first.format;
    let sprite = first.sprite;
    let additive = first.additive;
    // Exercise the install boundary's real byte accounting, not caller metadata.
    prepared.entries.push(PreparedPipeline {
        pipeline,
        format,
        sprite,
        additive,
        wgsl: Arc::from(" ".repeat(MAX_PREPARED_WGSL_BYTES)),
    });
    prepared.wgsl_bytes = 0;
    assert!(renderer
        .install_prepared_effect_pipelines(prepared, CONTEXT)
        .is_err());
    assert_eq!(renderer.effect_pipeline_cache_info().previous_entries, 0);
    renderer.check_health().unwrap();
}
