use super::*;
use motion_core::{Composition, CompositionClip, Content, Layer, Project};
use std::error::Error;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_nested_exposure_admission_keeps_composition_error_and_recovers_pixels() {
    let mut project = Project::new(8, 8, 30, 60).unwrap();
    project.background = [0.; 4];
    let mut reference = Layer::solid(1, "Child", [8.; 2], [4., 4., 0.], [1.; 4]);
    reference.content = Content::Composition {
        clip: CompositionClip::new("comp-child".into()),
    };
    project.layers.push(reference);
    project.compositions.push(Composition {
        id: "comp-child".into(),
        name: "Child".into(),
        width: 8,
        height: 8,
        fps: project.fps,
        frames: 60,
        work_area: motion_core::WorkArea::full(60),
        guides: vec![],
        background: [0., 1., 0., 1.],
        camera: motion_core::Camera::new(8, 8),
        layers: vec![],
        expressions: vec![],
    });
    project.validate().unwrap();
    let project = Arc::new(project);
    let before = serde_json::to_vec(project.as_ref()).unwrap();
    let document = ExposureDocument::new(project.clone()).unwrap();
    let scenes = ExposureScenes::new(
        document.clone(),
        &project.composition_id,
        1.,
        false,
        None,
        motion_core::ExpressionEvaluator,
    )
    .unwrap();
    let factory = scenes.child_factory();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 1., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert_eq!(scene.nested.len(), 1);
    // The renderer accepts caller-owned sampled Scenes. Keep the frozen valid
    // document, but submit an inconsistent child identity to the real boundary.
    scene.nested[0].composition = "comp-missing".into();
    let mut nested = NestedPreparation::new(&scene, [8, 8]);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut budget = PollBudget {
        started: Instant::now(),
        slice: Duration::from_secs(1),
        remaining: MAX_WORK_UNITS_PER_POLL,
    };
    let mut progress = ExposureProgress {
        stage: ExposureStage::Receiver,
        phase: 0,
        phases: 1,
        receiver: None,
        sample_frame: None,
        work_units: 0,
    };
    let error = nested
        .poll(
            &mut renderer,
            &factory,
            false,
            &mut budget,
            &mut progress,
            &mut |_, _, _| panic!("invalid child must be rejected before media preparation"),
        )
        .unwrap_err();
    let RenderError::Diagnostic { message, cause } = &error else {
        panic!("{error:?}");
    };
    assert!(message.contains("composition=comp-missing"));
    assert!(message.contains("reference=1"));
    assert!(message.contains("sample_frame=1"));
    assert!(message.contains("phase=nested_exposure"));
    let RenderError::Diagnostic { cause, .. } = cause.as_ref() else {
        panic!("{cause:?}");
    };
    let RenderError::Model(native) = cause.as_ref() else {
        panic!("nested error lost original model: {cause:?}");
    };
    let motion_core::Error::Composition(detail) = native else {
        panic!("{native:?}");
    };
    assert_eq!(detail.composition, "comp-missing");
    assert!(cause.source().unwrap().is::<motion_core::Error>());
    assert!(error.resource_rejection().is_none());
    assert_eq!(progress.work_units, 0);
    assert_eq!(serde_json::to_vec(project.as_ref()).unwrap(), before);
    drop(nested);
    let mut recovered = ExposureFrame::new(&mut renderer, scenes, 8, 8).unwrap();
    let mut ready = false;
    for _ in 0..1000 {
        if matches!(
            recovered
                .poll(&mut renderer, Duration::from_secs(1), |_, _| Ok(true))
                .unwrap(),
            ExposurePoll::Ready(_)
        ) {
            ready = true;
            break;
        }
    }
    assert!(ready, "healthy replacement must finish");
    let pixels = renderer.capture_exposure(recovered.target()).unwrap();
    assert_eq!(pixels, [0, 255, 0, 255].repeat(8 * 8));
    assert_eq!(serde_json::to_vec(project.as_ref()).unwrap(), before);
    renderer.check_health().unwrap();
}
