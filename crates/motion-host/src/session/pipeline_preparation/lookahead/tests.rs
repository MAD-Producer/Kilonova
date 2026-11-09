use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_frame_conversion_overflow_preserves_rate_error_and_historical_diagnostic() {
    let project = Project::new(16, 16, 30, 60).unwrap();
    let scene = motion_render::Scene::new(&project).unwrap();
    let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    let job = renderer.prepare_effect_pipeline_job(vec![scene], 16, 16, Context {
        document_generation: 26, project_revision: 3,
    }).unwrap();
    let ticket = job.ticket();
    let layer = Layer::solid(1, "clock conversion", [16.; 2], [8., 8., 0.], [1.; 4]);
    let link = Link { layer: &layer, parent_fps: FrameRate::new(1_000_000, 1).unwrap(),
        child_fps: FrameRate::new(1, 1).unwrap(), frames: 60, source_start: 0 };
    let mut scan = Scan { ticket: &ticket, work: MAX_WORK, events: vec![], begin: 0., end: 60. };
    let error = match scan.event(&[link], f64::MAX) {
        Err(Failure::Failed(error)) => error,
        _ => panic!("actual frame conversion overflow did not fail"),
    };
    let native = motion_core::RateError::NumericRange;
    assert_eq!(error.to_string(), motion_render::RenderError::Invalid(native.to_string()).to_string());
    let motion_render::RenderError::Diagnostic { cause, .. } = &error else { panic!("missing diagnostic") };
    assert!(matches!(cause.as_ref(), motion_render::RenderError::Model(motion_core::Error::FrameRate(value)) if *value == native));
    assert!(std::error::Error::source(&error).is_some());
    assert!(scan.events.is_empty());
    assert!(!ticket.status().cancelled);
    scan.event(&[link], 0.000_001).unwrap_or_else(|_| panic!("valid conversion did not recover"));
    assert_eq!(scan.events, [1.]);
}
