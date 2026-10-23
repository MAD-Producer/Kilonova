use super::*;
use motion_render::RenderError;

fn io_cause(error: &RenderError) -> Option<&std::io::Error> {
    match error {
        RenderError::Io(cause) => Some(cause),
        RenderError::Context { cause, .. } | RenderError::Diagnostic { cause, .. } => {
            io_cause(cause)
        }
        RenderError::Shared(cause) => io_cause(cause),
        _ => None,
    }
}

fn model_cause(error: &RenderError) -> Option<&motion_model::Error> {
    match error {
        RenderError::Model(cause) => Some(cause),
        RenderError::Context { cause, .. } | RenderError::Diagnostic { cause, .. } => {
            model_cause(cause)
        }
        RenderError::Shared(cause) => model_cause(cause),
        _ => None,
    }
}

fn shared_cause(error: &RenderError) -> &Arc<RenderError> {
    match error {
        RenderError::Shared(cause) => cause,
        RenderError::Context { cause, .. } | RenderError::Diagnostic { cause, .. } => {
            shared_cause(cause)
        }
        _ => panic!("original shared failure missing: {error:?}"),
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn missing_file_failure_replays_the_same_native_io_without_retry_then_releases_and_recovers() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing-source.bin");
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let project = project(false);
    let before = serde_json::to_value(&project).unwrap();
    let recorded = Recorded::default();
    let mut failed = task(&mut renderer, &project, recorded.clone());
    let mut calls = 0;
    let first = failed
        .poll(&mut renderer, Duration::from_secs(1), |_, scene| {
            calls += 1;
            if scene.frame == 1. {
                std::fs::File::open(&missing).map_err(RenderError::from)?;
            }
            Ok(true)
        })
        .unwrap_err();
    let native = io_cause(&first).expect("first failure lost native IO");
    assert_eq!(native.kind(), std::io::ErrorKind::NotFound);
    let original_pointer = native as *const std::io::Error;
    let os_code = native.raw_os_error();
    assert!(
        os_code.is_some(),
        "actual file access must retain its OS error code"
    );
    let evaluations = recorded.frames.borrow().len();
    let calls_before = calls;
    assert!(
        calls_before > 1,
        "failure must follow completed local materialization"
    );
    assert_eq!(failed.progress().stage, ExposureStage::Failed);
    assert_eq!(
        renderer.exposure_texture_bytes(),
        failed.target().texture_bytes()
    );
    for _ in 0..60 {
        let replay = failed
            .poll(&mut renderer, Duration::from_secs(1), |_, _| {
                calls += 1;
                panic!("failed IO task retried preparation")
            })
            .unwrap_err();
        let cause = io_cause(&replay).expect("replayed failure lost native IO");
        assert_eq!(cause.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(cause.raw_os_error(), os_code);
        assert_eq!(cause as *const std::io::Error, original_pointer);
        assert!(replay.to_string().chars().count() <= 2080);
    }
    assert_eq!(calls, calls_before);
    assert_eq!(recorded.frames.borrow().len(), evaluations);
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    let retained = Arc::downgrade(shared_cause(&first));
    drop(first);
    assert!(
        retained.upgrade().is_some(),
        "failed task must retain its cause"
    );
    drop(failed);
    assert!(
        retained.upgrade().is_none(),
        "dropping the task must release its cause"
    );
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    let mut recovered = task(&mut renderer, &project, Recorded::default());
    let mut completed = false;
    for _ in 0..64 {
        if matches!(
            recovered
                .poll(&mut renderer, Duration::from_secs(1), |_, _| Ok(true))
                .unwrap(),
            ExposurePoll::Ready(_)
        ) {
            completed = true;
            break;
        }
    }
    assert!(completed);
    equal(
        &renderer.capture_exposure(recovered.target()).unwrap(),
        &oracle(&mut renderer, &project),
    );
    drop(recovered);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn evaluator_refusals_keep_the_native_composition_in_constructor_and_terminal_replay() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let project = project(false);
    let document = ExposureDocument::new(Arc::new(project.clone())).unwrap();
    let scenes = ExposureScenes::new(
        document.clone(),
        &project.composition_id,
        1.,
        false,
        None,
        Recorded {
            fail: Some(1.),
            ..Recorded::default()
        },
    )
    .unwrap();
    let error = match ExposureFrame::new(&mut renderer, scenes, 32, 32) {
        Ok(_) => panic!("center evaluation must fail"),
        Err(error) => error,
    };
    assert!(
        matches!(model_cause(&error), Some(motion_model::Error::Composition(detail))
        if detail.code == "sample_failed" && detail.message == "controlled center failure")
    );
    assert!(error
        .to_string()
        .starts_with("GPU validation: composition_error:"));
    assert_eq!(renderer.exposure_texture_bytes(), 0);

    let recorded = Recorded {
        fail: Some(0.25),
        ..Recorded::default()
    };
    let mut failed = task(&mut renderer, &project, recorded.clone());
    let error = failed
        .poll(&mut renderer, Duration::from_secs(1), |_, _| {
            panic!("evaluation failure reached media")
        })
        .unwrap_err();
    let model = model_cause(&error).expect("receiver evaluation lost original model failure");
    assert!(matches!(model, motion_model::Error::Composition(detail)
        if detail.code == "sample_failed" && detail.message == "controlled center failure"));
    let original_pointer = model as *const motion_model::Error;
    let evaluations = recorded.frames.borrow().len();
    for _ in 0..60 {
        let replay = failed
            .poll(&mut renderer, Duration::from_secs(1), |_, _| {
                panic!("failed evaluation retried")
            })
            .unwrap_err();
        assert_eq!(
            model_cause(&replay).unwrap() as *const motion_model::Error,
            original_pointer
        );
        assert!(Arc::ptr_eq(shared_cause(&error), shared_cause(&replay)));
    }
    assert_eq!(recorded.frames.borrow().len(), evaluations);
    let retained = Arc::downgrade(shared_cause(&error));
    drop(error);
    drop(failed);
    assert!(retained.upgrade().is_none());
    assert_eq!(renderer.exposure_texture_bytes(), 0);
}
