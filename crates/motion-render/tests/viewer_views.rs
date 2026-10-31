use motion_core::{Camera, Content, Ease, Layer, LayerTimeline, Observer, Project};
use motion_render::{ModelSourceCatalog, Renderer, Scene, ViewCamera};

#[path = "support/imported_camera.rs"]
mod imported;

fn camera(id: u64, distance: f32) -> Layer {
    let mut layer = Layer::camera(id, "View camera", 96, 96);
    if let Content::Camera { camera } = &mut layer.content {
        camera.position.value = [48., 48., -distance];
    }
    layer
}
fn fixture() -> Project {
    let mut project = Project::new(96, 96, 30, 90).unwrap();
    project.background = [0., 0., 0., 1.];
    let mut shape = Layer::solid(7, "Spatial", [30., 22.], [55., 48., 0.], [1., 0., 0., 1.]);
    shape.three_d = true;
    shape.transform.rotation.value[1] = 15.;
    project.layers = vec![camera(1, 100.), camera(2, 220.), shape];
    project.validate().unwrap();
    project
}
fn sampled(project: &Project, frame: f64, view: ViewCamera<'_>) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample_view(project, frame, view, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn assert_same_camera(a: &Scene, b: &Scene) {
    assert_eq!(a.camera.view_projection, b.camera.view_projection);
    assert_eq!(a.camera.eye, b.camera.eye);
    assert_eq!(a.camera_viewport, b.camera_viewport);
    assert_eq!(
        a.native_camera().view_projection,
        b.native_camera().view_projection
    );
}

#[test]
fn explicit_camera_views_and_observers_are_independent_and_share_unchanged_project_data() {
    let project = fixture();
    let before = project.clone();
    let mut observer = Observer::new(96, 96);
    observer.orbit(34., -22.).unwrap();
    let active = sampled(&project, 12.5, ViewCamera::Composition);
    let near = sampled(&project, 12.5, ViewCamera::Camera(1));
    let far = sampled(&project, 12.5, ViewCamera::Camera(2));
    let free = sampled(&project, 12.5, ViewCamera::Observer(&observer));
    assert_same_camera(&active, &far);
    assert_ne!(near.camera.view_projection, far.camera.view_projection);
    assert_ne!(free.camera.view_projection, active.camera.view_projection);
    let retained = [near.camera.view_projection, far.camera.view_projection];
    observer.orbit(12., 8.).unwrap();
    let updated = sampled(&project, 12.5, ViewCamera::Observer(&observer));
    assert_ne!(updated.camera.view_projection, free.camera.view_projection);
    assert_eq!(
        [near.camera.view_projection, far.camera.view_projection],
        retained
    );
    for scene in [&active, &near, &far, &free, &updated] {
        assert_eq!(scene.frame, 12.5);
        assert_eq!(scene.world_matrix(7), near.world_matrix(7));
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(
            scene.layers[0].view_projection,
            scene.camera.view_projection
        );
    }
    assert_eq!(project, before);
    let mut legacy = Scene::new(&project).unwrap();
    legacy
        .sample(
            &project,
            12.5,
            Some(&observer),
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    assert_same_camera(&legacy, &updated);
    legacy
        .sample(&project, 12.5, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert_same_camera(&legacy, &active);
}

#[test]
fn named_camera_selection_respects_parented_fractional_local_time_and_ignores_edit_locks() {
    let mut project = fixture();
    let mut parent = Layer::solid(8, "Camera parent", [1.; 2], [48., 48., 0.], [0.; 4]);
    parent.content = Content::Null;
    parent.transform.position.value = [58., 48., 0.];
    project.layers.push(parent);
    project.layers[0].parent = Some(motion_core::ParentLink {
        object: Some(8),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    project.layers[0].timeline = Some(LayerTimeline {
        in_frame: 20,
        out_frame: 80,
        offset_frame: 5,
    });
    project.layers[0].visible = false;
    project.layers[0].locked = true;
    if let Content::Camera { camera } = &mut project.layers[0].content {
        camera
            .position
            .upsert(0, [48., 48., -100.], Ease::Linear)
            .unwrap();
        camera
            .position
            .upsert(60, [48., 48., -160.], Ease::Linear)
            .unwrap();
    }
    project.validate().unwrap();
    for frame in [12.5, 30.25, 80.5] {
        let actual = sampled(&project, frame, ViewCamera::Camera(1));
        let mut expected = project.clone();
        expected.camera = expected.layers[0].camera_data().unwrap().clone();
        // The legacy camera has a composition clock, so explicitly translate
        // this independently constructed oracle's animation keys into it.
        expected.camera.position.value = [48., 48., -100. - (frame - 5.).clamp(0., 60.) as f32];
        expected.camera.position.keys.clear();
        expected.camera.parent = project.layers[0].parent.clone();
        expected.camera.created = true;
        for layer in &mut expected.layers {
            if layer.camera_data().is_some() {
                layer.visible = false;
            }
        }
        let control = sampled(&expected, frame, ViewCamera::Composition);
        assert_same_camera(&actual, &control);
        assert_ne!(
            actual.camera.view_projection,
            sampled(&project, frame, ViewCamera::Composition)
                .camera
                .view_projection
        );
    }
}

#[test]
fn deleted_non_camera_and_uncreated_legacy_bindings_fail_without_substituting_another_camera() {
    let project = fixture();
    for (id, reason) in [
        (999, "camera layer does not exist"),
        (7, "object is not a camera"),
        (0, "authored camera does not exist"),
    ] {
        let error = Scene::new(&project)
            .unwrap()
            .sample_view(
                &project,
                12.5,
                ViewCamera::Camera(id),
                &motion_core::ExpressionEvaluator,
            )
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(&format!("object={id}"))
                && error.contains("phase=resolve")
                && error.contains("frame=12.5")
                && error.contains(reason),
            "{error}"
        );
    }
    let mut authored = project.clone();
    authored.camera.created = true;
    authored.camera.position.value = [48., 48., -150.];
    assert_ne!(
        sampled(&authored, 0., ViewCamera::Camera(0))
            .camera
            .view_projection,
        sampled(&authored, 0., ViewCamera::Composition)
            .camera
            .view_projection
    );
    authored.layers.retain(|l| l.id != 1);
    assert!(Scene::new(&authored)
        .unwrap()
        .sample_view(
            &authored,
            0.,
            ViewCamera::Camera(1),
            &motion_core::ExpressionEvaluator
        )
        .is_err());
    // Invalid time remains a sampling error for every explicit view.
    for frame in [-1., 90., f64::NAN] {
        assert!(Scene::new(&authored)
            .unwrap()
            .sample_view(
                &authored,
                frame,
                ViewCamera::Camera(2),
                &motion_core::ExpressionEvaluator
            )
            .is_err());
    }
}

#[test]
fn explicit_parent_view_keeps_child_camera_selection_and_repeated_ids_local() {
    let mut project = fixture();
    let mut child = motion_core::Composition {
        id: "comp-child-view".into(),
        name: "Child view".into(),
        width: project.width,
        height: project.height,
        fps: project.fps,
        frames: project.frames,
        work_area: project.work_area,
        guides: vec![],
        background: project.background,
        camera: project.camera.clone(),
        layers: project.layers.clone(),
        expressions: vec![],
    };
    if let Content::Camera { camera } = &mut child.layers[1].content {
        camera.position.value = [48., 48., -175.];
    }
    project.compositions.push(child);
    let mut reference = Layer::solid(9, "Child", [96.; 2], [48., 48., 0.], [1.; 4]);
    reference.content = Content::Composition {
        clip: motion_core::CompositionClip::new("comp-child-view".into()),
    };
    project.layers.push(reference);
    project.validate().unwrap();
    let active = sampled(&project, 12.5, ViewCamera::Composition);
    let near = sampled(&project, 12.5, ViewCamera::Camera(1));
    assert_ne!(active.camera.view_projection, near.camera.view_projection);
    assert_eq!(active.nested.len(), 1);
    assert_same_camera(&active.nested[0].scene, &near.nested[0].scene);
    assert_ne!(
        near.camera.view_projection,
        near.nested[0].scene.camera.view_projection
    );
    assert_eq!(near.nested[0].scene.frame, 12.5);
}

#[test]
fn imported_named_camera_retains_its_source_clock_projection_and_letterbox() {
    let root = tempfile::tempdir().unwrap();
    let source = imported::project(root.path(), true);
    let catalog = ModelSourceCatalog::prepare(&source, root.path(), &Default::default()).unwrap();
    let mut project = source.clone();
    let mut imported = Layer::camera(9, "Imported view", project.width, project.height);
    imported.content = Content::Camera {
        camera: Box::new(project.camera.clone()),
    };
    project.camera = Camera::new(project.width, project.height);
    project.layers.push(imported);
    project.layers.push(Layer::camera(
        10,
        "Other active view",
        project.width,
        project.height,
    ));
    project.validate().unwrap();
    for frame in [0., 15.5, 60., 0.] {
        let mut original = Scene::new(&source).unwrap();
        original.set_model_source_catalog(catalog.clone());
        original
            .sample(&source, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let mut actual = Scene::new(&project).unwrap();
        actual.set_model_source_catalog(catalog.clone());
        actual
            .sample_view(
                &project,
                frame,
                ViewCamera::Camera(9),
                &motion_core::ExpressionEvaluator,
            )
            .unwrap();
        assert_same_camera(&actual, &original);
        assert_eq!(actual.camera_viewport, [16., 0., 64., 64.]);
        assert!(actual.hit_candidates([0., 32.]).unwrap().is_empty());
    }
    let error = Scene::new(&project)
        .unwrap()
        .sample_view(
            &project,
            0.,
            ViewCamera::Camera(9),
            &motion_core::ExpressionEvaluator,
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("source=7") && error.contains("not configured"),
        "{error}"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_gpu_views_match_explicit_camera_oracles_and_picking_uses_the_same_projection() {
    let project = fixture();
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), &project).unwrap();
    let opened = motion_core::storage::load(root.path()).unwrap();
    assert_eq!(opened, project);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let capture = |renderer: &mut Renderer, scene: &Scene| {
        renderer.retain_video_instances(scene);
        let target = renderer.capture_target(96, 96).unwrap();
        renderer.capture(scene, &target).unwrap().0
    };
    let near = sampled(&opened, 12.5, ViewCamera::Camera(1));
    let far = sampled(&opened, 12.5, ViewCamera::Camera(2));
    let near_pixels = capture(&mut renderer, &near);
    let far_pixels = capture(&mut renderer, &far);
    assert_ne!(
        near_pixels, far_pixels,
        "fixture must detect a substituted camera"
    );
    for (id, actual) in [(1, &near_pixels), (2, &far_pixels)] {
        let mut oracle = opened.clone();
        oracle.camera = oracle.camera_for(id).unwrap().clone();
        oracle.camera.created = true;
        for layer in &mut oracle.layers {
            if layer.camera_data().is_some() {
                layer.visible = false;
            }
        }
        oracle.validate().unwrap();
        let expected = sampled(&oracle, 12.5, ViewCamera::Composition);
        assert_eq!(*actual, capture(&mut renderer, &expected));
    }
    assert_eq!(
        capture(&mut renderer, &near),
        near_pixels,
        "rendering another view cannot replace the retained scene's camera"
    );
    let mut different_hits = 0;
    for y in 1..95 {
        for x in 1..95 {
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            let near_hit = near
                .hit_candidates(point)
                .unwrap()
                .iter()
                .any(|hit| hit.id == 7);
            let far_hit = far
                .hit_candidates(point)
                .unwrap()
                .iter()
                .any(|hit| hit.id == 7);
            let offset = (y * 96 + x) * 4;
            let near_red = near_pixels[offset] > 240;
            let far_red = far_pixels[offset] > 240;
            // Check interiors away from raster edge antialiasing. Every
            // strongly red pixel must pick the geometry from that same view.
            if near_red {
                assert!(near_hit, "near pick {point:?}");
            }
            if far_red {
                assert!(far_hit, "far pick {point:?}");
            }
            if near_hit != far_hit {
                different_hits += 1;
            }
        }
    }
    assert!(
        different_hits > 100,
        "fixture must reject shared-camera picking"
    );
    if let Some(path) = std::env::var_os("MOTION_MULTIVIEW_SCENE_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        for (name, pixels) in [
            ("camera-near.png", &near_pixels),
            ("camera-far.png", &far_pixels),
        ] {
            image::save_buffer(path.join(name), pixels, 96, 96, image::ColorType::Rgba8).unwrap();
        }
    }
    assert_eq!(opened, project);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn opposite_views_reverse_spatial_occlusion_and_picking_but_keep_flat_layers_fixed() {
    let mut project = fixture();
    if let Content::Camera { camera } = &mut project.layers[1].content {
        camera.position.value = [48., 48., 100.];
    }
    project.layers.retain(|layer| layer.camera_data().is_some());
    for (id, z, color) in [(7, -20., [1., 0., 0., 1.]), (8, 20., [0., 0., 1., 1.])] {
        let mut layer = Layer::solid(id, "Depth plane", [30.; 2], [48., 48., z], color);
        layer.three_d = true;
        project.layers.push(layer);
    }
    project.layers.push(Layer::solid(
        9,
        "Flat overlay",
        [6.; 2],
        [10., 10., 0.],
        [1., 1., 0., 1.],
    ));
    project.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(96, 96).unwrap();
    for (camera, front, color) in [(1, 7, [255, 0, 0, 255]), (2, 8, [0, 0, 255, 255])] {
        let scene = sampled(&project, 12.5, ViewCamera::Camera(camera));
        let hits = scene.hit_candidates([48., 48.]).unwrap();
        assert_eq!(hits.first().unwrap().id, front);
        renderer.retain_video_instances(&scene);
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        assert_eq!(&pixels[(48 * 96 + 48) * 4..(48 * 96 + 48) * 4 + 4], &color);
        assert_eq!(scene.hit_candidates([10., 10.]).unwrap()[0].id, 9);
        assert_eq!(
            &pixels[(10 * 96 + 10) * 4..(10 * 96 + 10) * 4 + 4],
            &[255, 255, 0, 255]
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_selected_view_matches_native_pixels_offline_and_rejects_wrong_camera_substitution() {
    let root = tempfile::tempdir().unwrap();
    let source = imported::project(root.path(), true);
    let catalog = ModelSourceCatalog::prepare(&source, root.path(), &Default::default()).unwrap();
    let mut project = source.clone();
    let mut imported = Layer::camera(9, "Imported view", project.width, project.height);
    imported.content = Content::Camera {
        camera: Box::new(project.camera.clone()),
    };
    project.camera = Camera::new(project.width, project.height);
    project.camera.created = false;
    project.layers.push(imported);
    project.layers.push(Layer::camera(
        10,
        "Other active view",
        project.width,
        project.height,
    ));
    project.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer
        .configure_assets_with_model_catalog(&project, root.path(), &catalog)
        .unwrap();
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    let target = renderer.capture_target(96, 64).unwrap();
    let mut previous = None;
    for frame in [0., 30., 0.] {
        let mut original = Scene::new(&source).unwrap();
        original.set_model_source_catalog(catalog.clone());
        original
            .sample(&source, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let mut actual = Scene::new(&project).unwrap();
        actual.set_model_source_catalog(catalog.clone());
        actual
            .sample_view(
                &project,
                frame,
                ViewCamera::Camera(9),
                &motion_core::ExpressionEvaluator,
            )
            .unwrap();
        renderer.retain_video_instances(&original);
        let expected_pixels = renderer.capture(&original, &target).unwrap().0;
        renderer.retain_video_instances(&actual);
        let actual_pixels = renderer.capture(&actual, &target).unwrap().0;
        assert_eq!(actual_pixels, expected_pixels);
        assert!(
            actual_pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[0] > 240)
                .count()
                > 50
        );
        if frame == 0. {
            if let Some(previous) = &previous {
                assert_eq!(&actual_pixels, previous);
            }
            previous = Some(actual_pixels.clone());
            actual
                .sample_view(
                    &project,
                    frame,
                    ViewCamera::Camera(10),
                    &motion_core::ExpressionEvaluator,
                )
                .unwrap();
            renderer.retain_video_instances(&actual);
            assert_ne!(
                actual_pixels,
                renderer.capture(&actual, &target).unwrap().0,
                "fixture must detect replacing the imported selection with the active camera"
            );
        }
    }
}

fn moving_camera_fixture() -> Project {
    let mut project = fixture();
    if let Content::Camera { camera } = &mut project.layers[0].content {
        camera
            .position
            .upsert(0, [48., 48., -100.], Ease::Linear)
            .unwrap();
        camera
            .position
            .upsert(60, [48., 48., -160.], Ease::Linear)
            .unwrap();
    }
    project
        .layers
        .iter_mut()
        .find(|layer| layer.id == 7)
        .unwrap()
        .sampling
        .motion_blur = Some(motion_core::render_sampling::Shutter {
        frames: 2.,
        samples: 4,
    });
    project.validate().unwrap();
    project
}

#[test]
fn exposure_samples_keep_the_selected_camera_at_each_fractional_receiver_time() {
    use motion_render::exposure_scene::{ExposureDocument, ExposureSceneStep, ExposureScenes};
    let project = std::sync::Arc::new(moving_camera_fixture());
    let before = project.as_ref().clone();
    let mut exposure = ExposureScenes::new_for_view(
        ExposureDocument::new(project.clone()).unwrap(),
        &project.composition_id,
        30.,
        false,
        ViewCamera::Camera(1),
        motion_core::ExpressionEvaluator,
    )
    .unwrap();
    assert_same_camera(
        exposure.center_scene().unwrap(),
        &sampled(&project, 30., ViewCamera::Camera(1)),
    );
    let mut times = vec![];
    loop {
        match exposure.current().unwrap() {
            ExposureSceneStep::Receiver(receiver) => {
                assert_eq!(receiver.object, 7);
                let scene = receiver.scene.unwrap();
                assert_same_camera(
                    scene,
                    &sampled(&project, receiver.frame, ViewCamera::Camera(1)),
                );
                assert_ne!(
                    scene.camera.view_projection,
                    sampled(&project, receiver.frame, ViewCamera::Composition)
                        .camera
                        .view_projection
                );
                times.push(receiver.frame);
                exposure.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { .. } => exposure.finish_phase().unwrap(),
            ExposureSceneStep::Complete => break,
        }
    }
    assert_eq!(times, [29.25, 29.75, 30.25, 30.75]);
    assert_same_camera(
        exposure.center_scene().unwrap(),
        &sampled(&project, 30., ViewCamera::Camera(1)),
    );
    assert_eq!(project.as_ref(), &before);
}

#[test]
fn exposure_freezes_observer_input_and_failed_selection_never_yields_a_fallback_scene() {
    use motion_render::exposure_scene::{ExposureDocument, ExposureScenes};
    let project = std::sync::Arc::new(moving_camera_fixture());
    let document = ExposureDocument::new(project.clone()).unwrap();
    let mut observer = Observer::new(96, 96);
    observer.orbit(18., -14.).unwrap();
    let expected = observer.pose(96, 96).unwrap();
    let mut exposure = ExposureScenes::new_for_view(
        document.clone(),
        &project.composition_id,
        30.,
        false,
        ViewCamera::Observer(&observer),
        motion_core::ExpressionEvaluator,
    )
    .unwrap();
    observer.orbit(65., 22.).unwrap();
    assert_eq!(
        exposure.center_scene().unwrap().camera.view_projection,
        expected.view_projection
    );
    assert_ne!(
        expected.view_projection,
        observer.pose(96, 96).unwrap().view_projection
    );
    let mut failed = ExposureScenes::new_for_view(
        document,
        &project.composition_id,
        30.,
        false,
        ViewCamera::Camera(999),
        motion_core::ExpressionEvaluator,
    )
    .unwrap();
    let error = failed.center_scene().unwrap_err();
    let motion_core::Error::Composition(detail) = error else {
        panic!("missing exposure error context");
    };
    assert!(
        detail.message.contains("viewer_camera object=999")
            && detail.message.contains("phase=resolve")
    );
    assert_eq!(detail.code, "sample_failed");
    assert_eq!(detail.details["phase"], "exposure_scene");
    assert_eq!(detail.details["sample_frame"], 30.);
    assert!(failed
        .center_scene()
        .unwrap_err()
        .to_string()
        .contains("evaluation has failed"));
    assert!(failed.current().is_err());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn selected_camera_exposure_pixels_match_an_independent_legacy_camera_task() {
    use motion_render::exposure_scene::{ExposureDocument, ExposureScenes};
    use motion_render::{ExposureFrame, ExposurePoll, OutputTransform};
    let project = moving_camera_fixture();
    let mut oracle = project.clone();
    oracle.camera = oracle.camera_for(1).unwrap().clone();
    oracle.camera.created = true;
    for layer in &mut oracle.layers {
        if layer.camera_data().is_some() {
            layer.visible = false;
        }
    }
    oracle.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut capture = |project: &Project, view: ViewCamera<'_>| {
        let scenes = ExposureScenes::new_for_view(
            ExposureDocument::new(std::sync::Arc::new(project.clone())).unwrap(),
            &project.composition_id,
            30.,
            false,
            view,
            motion_core::ExpressionEvaluator,
        )
        .unwrap();
        let mut task = ExposureFrame::new(&mut renderer, scenes, 96, 96).unwrap();
        let mut ready = false;
        for _ in 0..32 {
            if matches!(
                task.poll(
                    &mut renderer,
                    std::time::Duration::from_secs(1),
                    |renderer, scene| {
                        renderer.retain_video_instances(scene);
                        Ok(true)
                    }
                )
                .unwrap(),
                ExposurePoll::Ready(_)
            ) {
                ready = true;
                break;
            }
        }
        assert!(
            ready,
            "selected exposure task must finish its finite schedule"
        );
        let target = renderer
            .sdr_capture_target(96, 96, OutputTransform::SDR_CLIP)
            .unwrap();
        renderer
            .capture_exposure_sdr(task.target(), &target)
            .unwrap()
            .pixels
    };
    let actual = capture(&project, ViewCamera::Camera(1));
    assert_eq!(actual, capture(&oracle, ViewCamera::Composition));
    assert_ne!(
        actual,
        capture(&project, ViewCamera::Composition),
        "fixture must reject switching to the active camera during shutter sampling"
    );
}
