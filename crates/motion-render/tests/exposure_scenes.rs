use motion_core::{
    composition::MAIN_COMPOSITION, render_sampling::Shutter, Camera, Composition, CompositionClip,
    Content, Ease, EffectInstance, Layer, LayerTimeline, Observer, ParentLink, Project, Result,
};
use motion_model::FrameEvaluator;
use motion_render::{
    effect_plan::PlanBuilder,
    exposure_scene::{ExposureDocument, ExposureSceneStep, ExposureScenes},
    Renderer, Scene,
};
use std::{borrow::Cow, cell::RefCell, rc::Rc, sync::Arc};

fn exposure<E: FrameEvaluator>(
    document: Arc<Project>,
    composition: &str,
    frame: f64,
    preview: bool,
    observer: Option<Observer>,
    evaluator: E,
) -> Result<ExposureScenes<E>> {
    ExposureScenes::new(
        ExposureDocument::new(document)?,
        composition,
        frame,
        preview,
        observer,
        evaluator,
    )
}

#[derive(Clone, Default)]
struct Recorded(Rc<RefCell<Vec<(String, f64)>>>);
impl FrameEvaluator for Recorded {
    fn evaluate<'a>(&self, p: &'a Project, frame: f64) -> Result<Cow<'a, Project>> {
        self.0.borrow_mut().push((p.composition_id.clone(), frame));
        motion_core::ExpressionEvaluator.evaluate(p, frame)
    }
}
fn solid(id: u64) -> Layer {
    Layer::solid(id, "authored", [16.; 2], [64., 64., 0.], [1.; 4])
}
fn base() -> Project {
    let mut p = Project::new(128, 128, 30, 60).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.background = [0.; 4];
    p
}
fn blur(layer: &mut Layer, frames: f64, samples: u8) {
    layer.sampling.motion_blur = Some(Shutter { frames, samples });
}
fn effect(id: u64, kind: &str) -> EffectInstance {
    let package = motion_effects::builtin::package().unwrap();
    let def = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == kind)
        .unwrap();
    EffectInstance::new(
        id,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        def,
        [16.; 2],
    )
}
fn timeline(offset: i32) -> LayerTimeline {
    LayerTimeline {
        in_frame: 0,
        out_frame: 60,
        offset_frame: offset,
    }
}

#[test]
fn receiver_time_drives_parent_camera_matte_and_effect_input_without_changing_unblurred_time() {
    use motion_core::{
        compositing::{MatteMode, TrackMatte},
        EffectImageInput, EffectImageStage,
    };
    let mut p = base();
    let mut parent = solid(10);
    parent.content = Content::Null;
    parent.timeline = Some(timeline(1));
    parent
        .transform
        .position
        .upsert(0, [64., 64., 0.], Ease::Linear)
        .unwrap();
    parent
        .transform
        .position
        .upsert(20, [84., 64., 0.], Ease::Linear)
        .unwrap();
    let mut receiver = solid(1);
    receiver.three_d = true;
    receiver.parent = Some(ParentLink {
        object: Some(10),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    receiver.timeline = Some(timeline(2));
    blur(&mut receiver, 1., 2);
    let mut brightness = effect(1, "brightness_contrast");
    brightness
        .params
        .get_mut("p0001")
        .unwrap()
        .track
        .upsert(0, [0.; 4], Ease::Linear)
        .unwrap();
    brightness
        .params
        .get_mut("p0001")
        .unwrap()
        .track
        .upsert(20, [0.4, 0., 0., 0.], Ease::Linear)
        .unwrap();
    let mut displacement = effect(2, "displacement_map");
    displacement.image_input = Some(EffectImageInput::Layer {
        layer: 3,
        stage: EffectImageStage::Effects,
    });
    receiver.effects = vec![brightness.clone(), displacement];
    receiver.track_matte = Some(TrackMatte {
        source: 4,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    let mut unblurred = solid(2);
    unblurred.three_d = true;
    unblurred.parent = receiver.parent.clone();
    let mut source = solid(3);
    source.visible = false;
    source.timeline = Some(timeline(3));
    source.effects = vec![brightness];
    blur(&mut source, 2., 3);
    let mut matte = solid(4);
    matte.visible = false;
    matte
        .transform
        .opacity
        .upsert(0, 0.2, Ease::Linear)
        .unwrap();
    matte
        .transform
        .opacity
        .upsert(20, 0.8, Ease::Linear)
        .unwrap();
    let camera_start = p.camera.position.value;
    p.camera.created = true;
    let mut camera_end = camera_start;
    camera_end[0] += 20.;
    p.camera
        .position
        .upsert(0, camera_start, Ease::Linear)
        .unwrap();
    p.camera
        .position
        .upsert(20, camera_end, Ease::Linear)
        .unwrap();
    p.layers = vec![receiver, unblurred, source, matte, parent];
    p.expressions.push(motion_model::PropertyExpression {
        target: motion_model::ExpressionTarget::Property {
            object: 10,
            property: motion_model::Property::Position,
            axis: Some(motion_model::Axis::X),
        },
        source: "value + time".into(),
        enabled: true,
        seed: 0,
        profile: motion_model::EXPRESSION_PROFILE.into(),
    });
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let p = Arc::new(p);
    let mut task = exposure(
        p.clone(),
        MAIN_COMPOSITION,
        10.,
        false,
        None,
        Recorded::default(),
    )
    .unwrap();
    let mut builder =
        PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    let mut times = [Vec::new(), Vec::new()];
    loop {
        match task.current().unwrap() {
            ExposureSceneStep::Receiver(sample) => {
                if sample.object == 1 || sample.object == 2 {
                    let t = sample.frame;
                    times[(sample.object - 1) as usize].push(t);
                    let scene = sample.scene.unwrap();
                    let layer = sample.layer().unwrap();
                    // Raw track value uses the parent's local offset; the
                    // expression's `time` is composition seconds by contract.
                    let parent_x = (t as f32 - 1.) + t as f32 / 30.;
                    assert!((layer.model.w_axis.x - parent_x).abs() < 1e-5);
                    assert_eq!(
                        scene.camera.view_projection,
                        p.camera.pose(t, 128, 128).unwrap().view_projection
                    );
                    assert_eq!(layer.view_projection, scene.camera.view_projection);
                    let source_effect = scene.effects.iter().find(|e| e.layer == 3).unwrap();
                    assert_eq!(source_effect.local_frame, t - 3.);
                    let index = source_effect
                        .param_ids
                        .iter()
                        .position(|id| id == "p0001")
                        .unwrap();
                    assert!((source_effect.values[index][0] - (t as f32 - 3.) * 0.02).abs() < 1e-6);
                    let matte = scene.layers.iter().find(|l| l.id == 4).unwrap();
                    assert!((matte.opacity - (0.2 + 0.03 * t as f32)).abs() < 1e-6);
                    let target = scene.effects.iter().find(|e| e.layer == 1).unwrap();
                    assert_eq!(target.local_frame, t - 2.);
                    let plan = builder.build(scene, &[0], 128, 128, true).unwrap();
                    assert!(plan
                        .passes
                        .iter()
                        .any(|pass| pass.resource_input > 0 && pass.uniform.params[30][1] > 0.5));
                    for pass in plan.passes.iter().filter(|pass| pass.resource_input > 0) {
                        assert!((pass.uniform.clock[1] - (t as f32 - 2.)).abs() < 1e-5);
                    }
                }
                task.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { .. } => task.finish_phase().unwrap(),
            ExposureSceneStep::Complete => break,
        }
    }
    assert!(times[0].iter().all(|t| *t == 9.75 || *t == 10.25));
    assert!(times[0].contains(&9.75) && times[0].contains(&10.25));
    assert!(times[1].iter().all(|t| *t == 10.));
}

#[test]
fn pending_polls_and_equal_times_reuse_context_and_phase_requires_explicit_completion() {
    let mut p = base();
    p.layers = vec![solid(1), solid(2), solid(3)];
    for layer in &mut p.layers {
        blur(layer, 1., 2);
    }
    let recorded = Recorded::default();
    let mut task = exposure(
        Arc::new(p),
        MAIN_COMPOSITION,
        10.,
        false,
        None,
        recorded.clone(),
    )
    .unwrap();
    assert!(task.finish_receiver().is_err());
    assert!(task.finish_phase().is_err());
    for phase in 0..2 {
        for object in 1..=3 {
            for _ in 0..60 {
                let ExposureSceneStep::Receiver(sample) = task.current().unwrap() else {
                    panic!("receiver missing")
                };
                assert_eq!(sample.object, object);
                assert_eq!(sample.frame, 9.75 + phase as f64 * 0.5);
                assert!(sample.layer().is_some());
            }
            assert_eq!(recorded.0.borrow().len(), phase + 1);
            task.finish_receiver().unwrap();
        }
        for _ in 0..60 {
            let ExposureSceneStep::PhaseReady { index, weight } = task.current().unwrap() else {
                panic!("phase missing")
            };
            assert_eq!(index, phase);
            assert_eq!(weight, 0.5);
        }
        assert_eq!(task.phase(), phase);
        task.finish_phase().unwrap();
    }
    assert!(matches!(
        task.current().unwrap(),
        ExposureSceneStep::Complete
    ));
    assert!(task.finish_phase().is_err());
    assert_eq!(recorded.0.borrow().len(), 2);
}

#[test]
fn boundaries_preserve_transparent_weights_and_preview_changes_only_sample_count() {
    for (preview, expected_phases) in [(true, 4), (false, 8)] {
        for frame in [0., 59.99] {
            let mut p = base();
            p.layers = vec![solid(1), solid(2)];
            blur(&mut p.layers[0], 1., 8);
            let mut task = exposure(
                Arc::new(p),
                MAIN_COMPOSITION,
                frame,
                preview,
                None,
                Recorded::default(),
            )
            .unwrap();
            assert_eq!(task.phase_count(), expected_phases);
            let (mut visible, mut transparent, mut fixed) = (0, 0, 0);
            let mut weight = 0.;
            loop {
                match task.current().unwrap() {
                    ExposureSceneStep::Receiver(sample) => {
                        if sample.object == 1 {
                            if !(0. ..60.).contains(&sample.frame) {
                                assert!(sample.scene.is_none());
                                transparent += 1;
                            } else {
                                assert!(sample.layer().is_some());
                                visible += 1;
                            }
                        } else {
                            assert_eq!(sample.frame, frame);
                            assert!(sample.layer().is_some());
                            fixed += 1;
                        }
                        task.finish_receiver().unwrap();
                    }
                    ExposureSceneStep::PhaseReady { weight: w, .. } => {
                        weight += w;
                        task.finish_phase().unwrap();
                    }
                    ExposureSceneStep::Complete => break,
                }
            }
            assert_eq!(visible, expected_phases / 2);
            assert_eq!(transparent, visible);
            assert_eq!(fixed, expected_phases);
            assert_eq!(weight, 1.);
        }
    }
    let p = Arc::new(base());
    let mut empty = exposure(
        p.clone(),
        MAIN_COMPOSITION,
        0.,
        true,
        None,
        Recorded::default(),
    )
    .unwrap();
    assert!(matches!(
        empty.current().unwrap(),
        ExposureSceneStep::PhaseReady { weight: 1., .. }
    ));
    empty.finish_phase().unwrap();
    assert!(matches!(
        empty.current().unwrap(),
        ExposureSceneStep::Complete
    ));
    for frame in [f64::NAN, -1., 60., f64::INFINITY] {
        assert!(exposure(
            p.clone(),
            MAIN_COMPOSITION,
            frame,
            false,
            None,
            Recorded::default()
        )
        .is_err());
    }
}

#[test]
fn validated_snapshot_is_immutable_and_signed_zero_contexts_are_distinct() {
    let mut p = base();
    p.layers = vec![solid(1), solid(2)];
    blur(&mut p.layers[0], 1., 3);
    let mut source = Arc::new(p);
    let snapshot = ExposureDocument::new(source.clone()).unwrap();
    let mut task = ExposureScenes::new(
        snapshot.clone(),
        MAIN_COMPOSITION,
        -0.,
        false,
        None,
        Recorded::default(),
    )
    .unwrap();
    // Editing the caller's next revision cannot alter the retained task.
    Arc::make_mut(&mut source).layers[0]
        .transform
        .position
        .value[0] = 100.;
    assert_eq!(snapshot.layers[0].transform.position.value[0], 64.);
    let (mut negative_zero, mut positive_zero) = (false, false);
    loop {
        match task.current().unwrap() {
            ExposureSceneStep::Receiver(sample) => {
                if let Some(scene) = sample.scene {
                    assert_eq!(scene.frame.to_bits(), sample.frame.to_bits());
                    assert_eq!(sample.layer().unwrap().model.w_axis.x, 0.);
                    if sample.frame == 0. {
                        negative_zero |= sample.frame.is_sign_negative();
                        positive_zero |= sample.frame.is_sign_positive();
                    }
                }
                task.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { .. } => task.finish_phase().unwrap(),
            ExposureSceneStep::Complete => break,
        }
    }
    assert!(negative_zero && positive_zero);
    let mut changed = ExposureScenes::new(
        ExposureDocument::new(source).unwrap(),
        MAIN_COMPOSITION,
        0.,
        false,
        None,
        Recorded::default(),
    )
    .unwrap();
    loop {
        match changed.current().unwrap() {
            ExposureSceneStep::Receiver(sample) => {
                if sample.object == 1 && sample.scene.is_some() {
                    assert_eq!(sample.layer().unwrap().model.w_axis.x, 36.);
                    break;
                }
                changed.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { .. } => changed.finish_phase().unwrap(),
            ExposureSceneStep::Complete => panic!("changed revision was never sampled"),
        }
    }
}

fn reference(id: u64, source: &str, source_start: i32) -> Layer {
    let mut layer = solid(id);
    let mut clip = CompositionClip::new(source.into());
    clip.source_start_frame = source_start;
    layer.content = Content::Composition { clip };
    layer
}
fn composition(id: &str, fps: u32, layers: Vec<Layer>) -> Composition {
    Composition {
        id: id.into(),
        name: id.into(),
        width: 128,
        height: 128,
        fps,
        frames: 300,
        background: [0.; 4],
        camera: Camera::new(128, 128),
        layers,
        expressions: vec![],
    }
}

#[test]
fn nested_composition_root_resolves_owning_document_and_fractional_rates_with_stable_aliases() {
    let mut p = base();
    p.layers = vec![reference(1, "comp-child", 0)];
    let mut nested = reference(7, "comp-leaf", 3);
    blur(&mut nested, 1., 2);
    p.compositions
        .push(composition("comp-child", 60, vec![nested]));
    p.compositions
        .push(composition("comp-leaf", 24, vec![solid(11)]));
    p.validate().unwrap();
    let p = Arc::new(p);
    let recorded = Recorded::default();
    let mut task = exposure(p.clone(), "comp-child", 10., false, None, recorded.clone()).unwrap();
    let mut alias = None;
    let mut frames = vec![];
    loop {
        match task.current().unwrap() {
            ExposureSceneStep::Receiver(sample) => {
                let child = &sample.scene.unwrap().nested[0].scene;
                let expected = sample.frame * 24. / 60. + 3.;
                assert!((child.frame - expected).abs() < 1e-12);
                assert_eq!(child.source_object(child.layers[0].id), 11);
                if let Some(old) = alias {
                    assert_eq!(old, child.layers[0].id);
                }
                alias = Some(child.layers[0].id);
                frames.push(expected);
                task.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { .. } => task.finish_phase().unwrap(),
            ExposureSceneStep::Complete => break,
        }
    }
    assert_eq!(
        recorded
            .0
            .borrow()
            .iter()
            .filter(|(id, _)| id == "comp-leaf")
            .map(|(_, f)| *f)
            .collect::<Vec<_>>(),
        frames
    );
    assert_eq!(recorded.0.borrow().len(), 4);
    // The original root/library are unchanged and still valid after scoped sampling.
    assert_eq!(p.composition_id, MAIN_COMPOSITION);
    p.validate().unwrap();
}

#[test]
fn evaluator_failure_is_contextual_and_terminal_while_drop_cancels_snapshot_ownership() {
    struct Broken(Recorded, serde_json::Value);
    impl FrameEvaluator for Broken {
        fn evaluate<'a>(&self, p: &'a Project, frame: f64) -> Result<Cow<'a, Project>> {
            self.0
                 .0
                .borrow_mut()
                .push((p.composition_id.clone(), frame));
            motion_core::composition::fail(
                "source",
                "source_fault",
                "original controlled cause",
                self.1.clone(),
            )
        }
    }
    let mut p = base();
    p.layers = vec![solid(1)];
    blur(&mut p.layers[0], 1., 2);
    let p = Arc::new(p);
    let recorded = Recorded::default();
    let mut task = exposure(
        p.clone(),
        MAIN_COMPOSITION,
        10.,
        false,
        None,
        Broken(
            recorded.clone(),
            serde_json::json!({"phase":"controlled_evaluator","source":9}),
        ),
    )
    .unwrap();
    assert_eq!(Arc::strong_count(&p), 2);
    let Err(motion_core::Error::Composition(error)) = task.current() else {
        panic!("original failure missing")
    };
    assert_eq!(error.code, "source_fault");
    assert_eq!(error.composition, "source");
    assert_eq!(error.message, "original controlled cause");
    assert_eq!(error.details["phase"], "controlled_evaluator");
    assert_eq!(error.details["exposure"]["receiver"], 1);
    assert_eq!(error.details["exposure"]["sample_frame"], 9.75);
    for _ in 0..60 {
        assert!(task.current().is_err());
    }
    assert_eq!(recorded.0.borrow().len(), 1);
    assert_eq!(task.phase(), 0);
    assert!(task.finish_receiver().is_err());
    assert!(task.finish_phase().is_err());
    drop(task);
    assert_eq!(Arc::strong_count(&p), 1);
    for details in [
        serde_json::json!(null),
        serde_json::json!([9]),
        serde_json::json!("source marker"),
        serde_json::json!({"exposure":{"receiver":77},"source":9}),
    ] {
        let mut task = exposure(
            p.clone(),
            MAIN_COMPOSITION,
            10.,
            false,
            None,
            Broken(Recorded::default(), details.clone()),
        )
        .unwrap();
        let Err(motion_core::Error::Composition(error)) = task.current() else {
            panic!("provider failure missing")
        };
        assert_eq!(error.code, "source_fault");
        assert_eq!(error.details["source_details"], details);
        assert_eq!(error.details["exposure"]["receiver"], 1);
    }
    let mut fresh = exposure(p, MAIN_COMPOSITION, 10., false, None, Recorded::default()).unwrap();
    assert!(matches!(
        fresh.current().unwrap(),
        ExposureSceneStep::Receiver(_)
    ));
}

fn moving_project() -> Project {
    let mut p = base();
    let mut moving = Layer::solid(
        1,
        "blurred rectangle",
        [12., 16.],
        [20., 40., 0.],
        [1., 0.2, 0.1, 0.7],
    );
    moving
        .transform
        .position
        .upsert(0, [20., 40., 0.], Ease::Linear)
        .unwrap();
    moving
        .transform
        .position
        .upsert(1, [108., 40., 0.], Ease::Linear)
        .unwrap();
    blur(&mut moving, 1., 8);
    let mut unblurred = Layer::solid(
        2,
        "sharp moving reference",
        [12., 16.],
        [20., 96., 0.],
        [0.1, 0.8, 0.3, 0.7],
    );
    unblurred
        .transform
        .position
        .upsert(0, [20., 96., 0.], Ease::Linear)
        .unwrap();
    unblurred
        .transform
        .position
        .upsert(1, [108., 96., 0.], Ease::Linear)
        .unwrap();
    p.layers = vec![moving, unblurred];
    p
}

#[test]
fn actual_sampled_project_layers_reach_gpu_and_keep_the_unblurred_moving_reference_sharp() {
    let p = Arc::new(moving_project());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut center = Scene::new(&p).unwrap();
    center
        .sample(&p, 0.5, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let target = renderer.capture_target(128, 128).unwrap();
    let sharp = renderer.capture(&center, &target).unwrap().0;
    let clock = motion_core::render_sampling::ExposureClock::new(
        0.5,
        &[(1, p.layers[0].sampling.motion_blur.unwrap())],
        false,
    )
    .unwrap();
    let weights: Vec<_> = clock.phases().iter().map(|p| p.weight).collect();
    let mut output = renderer.exposure_target(128, 128, &weights).unwrap();
    let recorded = Recorded::default();
    let mut task = exposure(
        p.clone(),
        MAIN_COMPOSITION,
        0.5,
        false,
        None,
        recorded.clone(),
    )
    .unwrap();
    let mut phase = center.clone();
    phase.layers.clear();
    let started = std::time::Instant::now();
    loop {
        match task.current().unwrap() {
            ExposureSceneStep::Receiver(sample) => {
                if let Some(layer) = sample.layer() {
                    phase.layers.push(layer.clone());
                }
                task.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { index, .. } => {
                assert_eq!(index, output.phase_index());
                phase.layers.sort_by_key(|l| l.order);
                renderer
                    .accumulate_exposure(&mut output, Some(&phase), false)
                    .unwrap();
                phase.layers.clear();
                task.finish_phase().unwrap();
            }
            ExposureSceneStep::Complete => break,
        }
    }
    let actual = renderer.capture_exposure(&output).unwrap();
    let capture_us = started.elapsed().as_micros();
    assert_eq!(
        recorded.0.borrow().len(),
        9,
        "eight shutter contexts plus one retained center context"
    );
    assert_ne!(actual, sharp);
    for row in 88..104 {
        assert_eq!(
            &actual[row * 128 * 4..(row + 1) * 128 * 4],
            &sharp[row * 128 * 4..(row + 1) * 128 * 4]
        );
    }
    let alpha = |pixels: &[u8], y0: usize, y1: usize| {
        pixels[y0 * 128 * 4..y1 * 128 * 4]
            .chunks_exact(4)
            .map(|p| u64::from(p[3]))
            .sum::<u64>()
    };
    assert!(alpha(&actual, 32, 48).abs_diff(alpha(&sharp, 32, 48)) < alpha(&sharp, 32, 48) / 50);
    let support = |pixels: &[u8]| {
        pixels[40 * 128 * 4..41 * 128 * 4]
            .chunks_exact(4)
            .filter(|p| p[3] > 0)
            .count()
    };
    assert!(support(&actual) > support(&sharp) * 4);
    renderer.check_health().unwrap();
    if let Some(folder) = std::env::var_os("MOTION_SAMPLING_ARTIFACT_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (name, pixels) in [
            ("receiver-scene-sharp.png", sharp),
            ("receiver-scene-exposure.png", actual),
        ] {
            image::RgbaImage::from_raw(128, 128, pixels)
                .unwrap()
                .save(folder.join(name))
                .unwrap();
        }
        std::fs::write(folder.join("receiver-scene-cost.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "capture_us":capture_us,"scene_evaluations":recorded.0.borrow().len(),
            "phases":task.phase_count(),"exposure_texture_bytes":output.texture_bytes(),
            "scope":"Production receiver evaluator with authored animated solid layers, actual GPU composition and accumulation. Includes capture/readback. This fixture has no effect textures or nested exposure; it does not implement host playback, media pending, frozen export, or general local-raster assembly."
        })).unwrap()).unwrap();
    }
}

#[test]
fn observer_pose_is_frozen_for_every_receiver_and_subframe_absence_is_transparent() {
    let mut p = base();
    p.layers = vec![solid(1)];
    p.layers[0].three_d = true;
    p.layers[0]
        .set_interval(
            60,
            motion_core::ClipRange {
                in_frame: 10.0,
                out_frame: 10.5,
            },
        )
        .unwrap();
    blur(&mut p.layers[0], 1., 2);
    let observer = Observer::new(128, 128);
    let expected = observer.pose(128, 128).unwrap();
    let mut task = exposure(
        Arc::new(p),
        MAIN_COMPOSITION,
        10.,
        false,
        Some(observer),
        Recorded::default(),
    )
    .unwrap();
    let mut present = vec![];
    loop {
        match task.current().unwrap() {
            ExposureSceneStep::Receiver(sample) => {
                let scene = sample.scene.unwrap();
                assert_eq!(scene.camera.view_projection, expected.view_projection);
                present.push(sample.layer().is_some());
                task.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { weight, .. } => {
                assert_eq!(weight, 0.5);
                task.finish_phase().unwrap();
            }
            ExposureSceneStep::Complete => break,
        }
    }
    assert_eq!(present, vec![false, true]);
}
