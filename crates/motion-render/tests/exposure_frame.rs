use motion_core::{
    render_sampling::Shutter, Content, EffectInstance, ExpressionEvaluator, Layer, Project, Result,
};
use motion_model::FrameEvaluator;
use motion_render::{
    exposure_scene::{ExposureDocument, ExposureScenes},
    ExposureFrame, ExposurePoll, ExposureStage, Renderer, ResourceBudgetScope, ResourceRejection,
    Scene,
};
use std::{borrow::Cow, cell::RefCell, rc::Rc, sync::Arc, time::Duration};

#[path = "exposure_frame/native_causes.rs"]
mod native_causes;

#[derive(Clone, Default)]
struct Recorded {
    frames: Rc<RefCell<Vec<f64>>>,
    fail: Option<f64>,
}
impl FrameEvaluator for Recorded {
    fn evaluate<'a>(&self, p: &'a Project, frame: f64) -> Result<Cow<'a, Project>> {
        self.frames.borrow_mut().push(frame);
        if self.fail == Some(frame) {
            return Err(motion_core::Error::Invalid(
                "controlled center failure".into(),
            ));
        }
        ExpressionEvaluator.evaluate(p, frame)
    }
}
fn project(adjustment: bool) -> Project {
    let mut p = Project::new(32, 32, 30, 60).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.background = [0.; 4];
    for (id, color, y) in [(1, [1., 0., 0., 1.], 8.), (2, [0., 1., 0., 1.], 24.)] {
        let mut l = Layer::solid(id, "persistent receiver", [4.; 2], [16., y, 0.], color);
        for (f, x) in [(0, 8.), (2, 24.)] {
            l.transform
                .position
                .upsert(f, [x, y, 0.], motion_core::Ease::Linear)
                .unwrap();
        }
        if id == 1 {
            l.sampling.motion_blur = Some(Shutter {
                frames: 2.,
                samples: 4,
            });
        }
        p.layers.push(l);
    }
    if adjustment {
        let package = motion_effects::builtin::package().unwrap();
        let d = package
            .manifest
            .effects
            .iter()
            .find(|d| d.id == "tint")
            .unwrap();
        let l = &mut p.layers[1];
        l.content = Content::Adjustment;
        l.size = [32.; 2];
        l.transform.position = motion_core::Track::constant([16., 16., 0.]);
        l.effects.push(EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            d,
            [32.; 2],
        ));
        p.rebuild_plugin_dependencies();
    }
    p.validate().unwrap();
    p
}
fn task(r: &mut Renderer, p: &Project, e: Recorded) -> ExposureFrame<Recorded> {
    let document = ExposureDocument::new(Arc::new(p.clone())).unwrap();
    let scenes = ExposureScenes::new(document, &p.composition_id, 1., false, None, e).unwrap();
    ExposureFrame::new(r, scenes, 32, 32).unwrap()
}
fn sample(p: &Project, frame: f64) -> Scene {
    let mut s = Scene::new(p).unwrap();
    s.sample(p, frame, None, &ExpressionEvaluator).unwrap();
    s
}
fn oracle(r: &mut Renderer, p: &Project) -> Vec<u8> {
    let center = sample(p, 1.);
    let mut target = r.exposure_target(32, 32, &[0.25; 4]).unwrap();
    for frame in [0.25, 0.75, 1.25, 1.75] {
        let s = sample(p, frame);
        let mut phase = center.clone();
        phase.layers[0] = s.layers[0].clone();
        r.accumulate_exposure(&mut target, Some(&phase), false)
            .unwrap();
    }
    r.capture_exposure(&target).unwrap()
}
fn equal(a: &[u8], b: &[u8]) {
    assert_eq!(a.len(), b.len());
    let max = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(max <= 1, "max pixel difference={max}");
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn pending_receivers_keep_completed_images_phases_and_evaluation_cache() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    r.set_submission_tracking(true);
    let p = project(false);
    let recorded = Recorded::default();
    let mut task = task(&mut r, &p, recorded.clone());
    let mut calls = Vec::new();
    let poll =
        |task: &mut ExposureFrame<Recorded>, r: &mut Renderer, calls: &mut Vec<f64>, gate: u8| {
            task.poll(r, Duration::from_secs(1), |_, s| {
                calls.push(s.frame);
                Ok(!((gate == 0 && s.frame == 1.) || (gate == 1 && s.frame == 1.25)))
            })
            .unwrap()
        };
    assert!(matches!(
        poll(&mut task, &mut r, &mut calls, 0),
        ExposurePoll::Pending(_)
    ));
    let first = task.progress();
    assert_eq!(first.receiver, Some(2));
    assert_eq!(first.sample_frame, Some(1.));
    assert_eq!(task.target().phase_index(), 0);
    let bytes = r.exposure_texture_bytes();
    assert_eq!(bytes, task.target().texture_bytes() + 4);
    let evaluations = recorded.frames.borrow().len();
    for _ in 0..60 {
        assert!(matches!(
            poll(&mut task, &mut r, &mut calls, 0),
            ExposurePoll::Pending(_)
        ));
        assert_eq!(task.progress(), first);
        assert_eq!(r.exposure_texture_bytes(), bytes);
    }
    assert_eq!(recorded.frames.borrow().len(), evaluations);
    assert_eq!(calls.iter().filter(|f| **f == 0.25).count(), 1);
    assert!(r.capture_exposure(task.target()).is_err());
    assert!(matches!(
        poll(&mut task, &mut r, &mut calls, 1),
        ExposurePoll::Pending(_)
    ));
    assert_eq!(task.target().phase_index(), 2);
    assert_eq!(task.progress().receiver, Some(1));
    let evaluations = recorded.frames.borrow().len();
    for _ in 0..60 {
        poll(&mut task, &mut r, &mut calls, 1);
    }
    assert_eq!(task.target().phase_index(), 2);
    assert_eq!(recorded.frames.borrow().len(), evaluations);
    assert!(matches!(
        poll(&mut task, &mut r, &mut calls, 2),
        ExposurePoll::Ready(_)
    ));
    assert_eq!(task.progress().stage, ExposureStage::Complete);
    assert_eq!(task.progress().receiver, None);
    let actual = r.capture_exposure(task.target()).unwrap();
    equal(&actual, &oracle(&mut r, &p));
    let before = r
        .submission_memory_info(&r.texture_resource_ledger())
        .submitted;
    for _ in 0..3 {
        assert!(matches!(
            task.poll(&mut r, Duration::from_secs(1), |_, _| panic!(
                "completed task prepared again"
            ))
            .unwrap(),
            ExposurePoll::Ready(_)
        ));
    }
    assert_eq!(
        r.submission_memory_info(&r.texture_resource_ledger())
            .submitted,
        before
    );
    drop(task);
    assert_eq!(r.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn center_media_pending_preserves_phase_images_before_adjustment_and_never_restarts() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let p = project(true);
    let mut task = task(&mut r, &p, Recorded::default());
    for _ in 0..60 {
        let state = task
            .poll(&mut r, Duration::from_secs(1), |_, s| Ok(s.frame != 1.))
            .unwrap();
        assert!(matches!(state, ExposurePoll::Pending(_)));
        assert_eq!(task.progress().stage, ExposureStage::Center);
        assert_eq!(task.target().phase_index(), 0);
        assert_eq!(
            r.exposure_texture_bytes(),
            task.target().texture_bytes() + 4
        );
    }
    assert!(matches!(
        task.poll(&mut r, Duration::from_secs(1), |_, _| Ok(true))
            .unwrap(),
        ExposurePoll::Ready(_)
    ));
    equal(
        &r.capture_exposure(task.target()).unwrap(),
        &oracle(&mut r, &p),
    );
    assert_eq!(r.exposure_texture_bytes(), task.target().texture_bytes());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn real_upload_refusal_is_terminal_preserves_typed_cause_and_drop_cancels_owned_targets() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let p = project(false);
    let mut task = task(&mut r, &p, Recorded::default());
    let original = r.memory_policy();
    let mut low = original;
    low.source_texture_budget_bytes = 64;
    r.set_memory_policy(low).unwrap();
    let error = task
        .poll(&mut r, Duration::from_secs(1), |r, _| {
            r.upload_image(9, 32, 32, &[255; 32 * 32 * 4])?;
            Ok(true)
        })
        .unwrap_err();
    let refusal = error.resource_rejection().copied().unwrap();
    assert!(
        matches!(
            refusal,
            ResourceRejection::Budget {
                scope: ResourceBudgetScope::SourceTextures,
                ..
            }
        ),
        "{error}"
    );
    assert_eq!(task.progress().stage, ExposureStage::Failed);
    assert_eq!(task.target().phase_index(), 0);
    r.set_memory_policy(original).unwrap();
    for _ in 0..60 {
        let again = task
            .poll(&mut r, Duration::from_secs(1), |_, _| {
                panic!("failed task retried")
            })
            .unwrap_err();
        assert_eq!(again.resource_rejection(), Some(&refusal));
        assert!(
            again.to_string().contains("source texture budget"),
            "{again}"
        );
    }
    assert!(r.capture_exposure(task.target()).is_err());
    drop(task);
    assert_eq!(r.exposure_texture_bytes(), 0);
    let mut fresh = self::task(&mut r, &p, Recorded::default());
    assert!(matches!(
        fresh
            .poll(&mut r, Duration::from_secs(1), |_, _| Ok(true))
            .unwrap(),
        ExposurePoll::Ready(_)
    ));
    equal(
        &r.capture_exposure(fresh.target()).unwrap(),
        &oracle(&mut r, &p),
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn finite_slices_yield_and_wrong_owner_or_zero_slice_do_not_invoke_media_or_poison_task() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let mut other = pollster::block_on(Renderer::headless()).unwrap();
    let p = project(false);
    let mut task = task(&mut r, &p, Recorded::default());
    assert!(task
        .poll(&mut r, Duration::ZERO, |_, _| panic!("invalid slice ran"))
        .is_err());
    assert!(task
        .poll(&mut other, Duration::from_secs(1), |_, _| panic!(
            "foreign renderer prepared"
        ))
        .unwrap_err()
        .to_string()
        .contains("another renderer"));
    assert_eq!(task.progress().stage, ExposureStage::Receiver);
    let mut pending = 0;
    loop {
        match task
            .poll(&mut r, Duration::from_nanos(1), |_, _| Ok(true))
            .unwrap()
        {
            ExposurePoll::Pending(_) => pending += 1,
            ExposurePoll::Ready(_) => break,
        }
        assert!(pending < 20);
    }
    assert!(pending >= 8, "tiny slice did not yield between work units");
    equal(
        &r.capture_exposure(task.target()).unwrap(),
        &oracle(&mut r, &p),
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn center_evaluation_failure_is_contextual_and_allocates_no_exposure_gpu_targets() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let p = project(false);
    let e = Recorded {
        fail: Some(1.),
        ..Default::default()
    };
    let document = ExposureDocument::new(Arc::new(p.clone())).unwrap();
    let scenes = ExposureScenes::new(document, &p.composition_id, 1., false, None, e).unwrap();
    let error = ExposureFrame::new(&mut r, scenes, 32, 32)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("controlled center failure") && error.contains("exposure_scene"),
        "{error}"
    );
    assert_eq!(r.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn authored_child_shutter_outside_its_whole_exposure_is_transparent_and_unused_children_are_allowed(
) {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = project(false);
    let mut child = p.layers[0].clone();
    child.id = 3;
    child.timeline = Some(motion_core::LayerTimeline {
        in_frame: 10,
        out_frame: 30,
        offset_frame: 0,
    });
    p.layers.truncate(1);
    p.layers[0].sampling = Default::default();
    p.layers[0].content = Content::Composition {
        clip: motion_core::CompositionClip::new("comp-shutter-child".into()),
    };
    p.compositions.push(motion_core::Composition {
        id: "comp-shutter-child".into(),
        name: "independent shutter".into(),
        width: 32,
        height: 32,
        fps: 30,
        frames: 60,
        work_area: motion_core::WorkArea::full(60),
        guides: Vec::new(),
        background: [0.; 4],
        camera: motion_core::Camera::new(32, 32),
        layers: vec![child],
        expressions: vec![],
    });
    assert!(sample(&p, 1.).nested[0].scene.layers.is_empty());
    let document = ExposureDocument::new(Arc::new(p.clone())).unwrap();
    let scenes = ExposureScenes::new(
        document,
        &p.composition_id,
        1.,
        false,
        None,
        Recorded::default(),
    )
    .unwrap();
    let mut nested = ExposureFrame::new(&mut r, scenes, 32, 32).unwrap();
    for _ in 0..100 {
        if matches!(
            nested
                .poll(&mut r, Duration::from_secs(1), |_, _| Ok(true))
                .unwrap(),
            ExposurePoll::Ready(_)
        ) {
            break;
        }
    }
    assert_eq!(nested.progress().stage, ExposureStage::Complete);
    assert!(r
        .capture_exposure(nested.target())
        .unwrap()
        .iter()
        .all(|byte| *byte == 0));
    drop(nested);
    assert_eq!(r.exposure_texture_bytes(), 0);
    p.layers[0].content = Content::Solid {
        color: [1., 0., 0., 1.],
    };
    let mut task = task(&mut r, &p, Recorded::default());
    assert!(matches!(
        task.poll(&mut r, Duration::from_secs(1), |_, _| Ok(true))
            .unwrap(),
        ExposurePoll::Ready(_)
    ));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn task_initialization_rejects_partly_consumed_scene_schedule_before_gpu_allocation() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let p = project(false);
    let document = ExposureDocument::new(Arc::new(p.clone())).unwrap();
    let mut scenes = ExposureScenes::new(
        document,
        &p.composition_id,
        1.,
        false,
        None,
        Recorded::default(),
    )
    .unwrap();
    scenes.current().unwrap();
    scenes.finish_receiver().unwrap();
    let error = ExposureFrame::new(&mut r, scenes, 32, 32)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("already advanced"), "{error}");
    assert_eq!(r.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn failure_after_partial_materialization_releases_images_and_bounds_retained_cause() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let p = project(false);
    let mut task = task(&mut r, &p, Recorded::default());
    let cause = format!("controlled media cause {}", "x".repeat(16384));
    let error = task
        .poll(&mut r, Duration::from_secs(1), |_, scene| {
            if scene.frame == 1. {
                Err(motion_render::RenderError::Invalid(cause.clone()))
            } else {
                Ok(true)
            }
        })
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(&cause),
        "first failure lost its original cause"
    );
    assert_eq!(task.progress().stage, ExposureStage::Failed);
    assert_eq!(r.exposure_texture_bytes(), task.target().texture_bytes());
    for _ in 0..60 {
        let retained = task
            .poll(&mut r, Duration::from_secs(1), |_, _| {
                panic!("failed media retried")
            })
            .unwrap_err()
            .to_string();
        assert!(retained.contains("controlled media cause"));
        assert!(
            retained.chars().count() <= 2080,
            "unbounded retained failure"
        );
    }
    assert_eq!(task.target().phase_index(), 0);
    assert!(r.capture_exposure(task.target()).is_err());
    drop(task);
    assert_eq!(r.exposure_texture_bytes(), 0);
}
