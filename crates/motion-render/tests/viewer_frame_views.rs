//! One evaluated frame, independent view projections, retained child ownership.
use motion_core::{Content, Layer, ObservationView, Observer, Project};
use motion_model::FrameEvaluator;
use motion_render::{ModelSourceCatalog, Renderer, Scene, ViewCamera};
use std::{borrow::Cow, cell::Cell, sync::Arc};

#[path = "support/imported_camera.rs"]
mod imported;

fn fixture() -> Project {
    let mut project = Project::new(96, 96, 30, 90).unwrap();
    project.background = [0., 0., 0., 1.];
    for (id, z) in [(1, -100.), (2, 100.)] {
        let mut layer = Layer::camera(id, "Camera", 96, 96);
        if let Content::Camera { camera } = &mut layer.content {
            camera.position.value = [48., 48., z];
        }
        project.layers.push(layer);
    }
    for (id, z, color) in [(7, -20., [1., 0., 0., 1.]), (8, 20., [0., 0., 1., 1.])] {
        let mut layer = Layer::solid(id, "Plane", [30.; 2], [48., 48., z], color);
        layer.three_d = true;
        project.layers.push(layer);
    }
    project.layers.push(Layer::solid(
        9,
        "Flat",
        [6.; 2],
        [10., 10., 0.],
        [1., 1., 0., 1.],
    ));
    project.validate().unwrap();
    project
}

fn nested_fixture() -> Project {
    let mut project = fixture();
    let mut child = motion_core::Composition {
        id: "comp-retained-child".into(),
        name: "Child".into(),
        width: 96,
        height: 96,
        fps: 30,
        frames: 90,
        work_area: project.work_area,
        guides: vec![],
        background: project.background,
        camera: project.camera.clone(),
        layers: project.layers.clone(),
        expressions: vec![],
    };
    child.layers[2]
        .transform
        .position
        .upsert(0, [30., 48., -20.], motion_core::Ease::Linear)
        .unwrap();
    child.layers[2]
        .transform
        .position
        .upsert(60, [66., 48., -20.], motion_core::Ease::Linear)
        .unwrap();
    project.compositions.push(child);
    for (id, x) in [(10, 24.), (11, 72.)] {
        let mut layer = Layer::solid(id, "Child instance", [96.; 2], [x, 48., 0.], [1.; 4]);
        layer.content = Content::Composition {
            clip: motion_core::CompositionClip::new("comp-retained-child".into()),
        };
        project.layers.push(layer);
    }
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

fn assert_projection(actual: &Scene, expected: &Scene) {
    assert_eq!(actual.frame, expected.frame);
    assert_eq!(actual.composition_id, expected.composition_id);
    assert_eq!(
        actual.camera.view_projection,
        expected.camera.view_projection
    );
    assert_eq!(actual.camera.eye, expected.camera.eye);
    assert_eq!(actual.camera.target, expected.camera.target);
    assert_eq!(actual.camera_viewport, expected.camera_viewport);
    assert_eq!(
        actual.native_camera().view_projection,
        expected.native_camera().view_projection
    );
    assert_eq!(actual.layers.len(), expected.layers.len());
    for (a, b) in actual.layers.iter().zip(&expected.layers) {
        assert_eq!(
            (a.id, a.order, a.depth, a.three_d, a.screen_overlay),
            (b.id, b.order, b.depth, b.three_d, b.screen_overlay)
        );
        assert_eq!(
            (a.model, a.view_projection, a.size, a.source_rect, a.color),
            (b.model, b.view_projection, b.size, b.source_rect, b.color)
        );
        assert_eq!(actual.world_matrix(a.id), expected.world_matrix(b.id));
    }
}

struct CountingEvaluator(Cell<usize>);
impl FrameEvaluator for CountingEvaluator {
    fn evaluate<'a>(
        &self,
        project: &'a Project,
        _frame: f64,
    ) -> motion_core::Result<Cow<'a, Project>> {
        let count = self.0.get() + 1;
        self.0.set(count);
        let mut project = project.clone();
        project
            .layers
            .iter_mut()
            .find(|layer| layer.id == 7)
            .unwrap()
            .transform
            .position
            .value[0] += count as f32;
        Ok(Cow::Owned(project))
    }
}

#[test]
fn one_sample_supplies_one_two_and_four_views_without_repeating_evaluation() {
    let project = fixture();
    let before = project.clone();
    let evaluator = CountingEvaluator(Cell::new(0));
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample_view(&project, 12.5, ViewCamera::Camera(1), &evaluator)
        .unwrap();
    assert_eq!(evaluator.0.get(), 1);
    let observer = Observer::new(96, 96);
    let choices = [
        ViewCamera::Composition,
        ViewCamera::Camera(1),
        ViewCamera::Camera(2),
        ViewCamera::Observer(&observer),
    ];
    for count in [1, 2, 4] {
        let views = scene.fork_views(&choices[..count]).unwrap();
        assert_eq!(views.len(), count);
        assert_eq!(evaluator.0.get(), 1);
        for view in &views {
            assert_eq!(view.world_matrix(7), scene.world_matrix(7));
            assert!(std::ptr::eq(
                view.sampled_project(&project),
                scene.sampled_project(&project)
            ));
            assert_eq!(
                view.sampled_project(&project).layers[2]
                    .transform
                    .position
                    .value[0],
                49.
            );
        }
    }
    let mut independent = Scene::new(&project).unwrap();
    independent
        .sample_view(&project, 12.5, ViewCamera::Composition, &evaluator)
        .unwrap();
    assert_eq!(evaluator.0.get(), 2);
    assert_ne!(
        independent.world_matrix(7),
        scene.world_matrix(7),
        "negative control must detect repeated non-idempotent evaluation"
    );
    assert_eq!(project, before);
}

#[test]
fn forked_named_active_and_orthographic_views_match_independent_sampling() {
    let project = fixture();
    let mut free = Observer::new(96, 96);
    free.orbit(22., -17.).unwrap();
    let mut top = free.clone();
    top.view = ObservationView::Top;
    let mut side = free.clone();
    side.view = ObservationView::Side;
    let choices = [
        ViewCamera::Composition,
        ViewCamera::Camera(1),
        ViewCamera::Camera(2),
        ViewCamera::Observer(&free),
        ViewCamera::Observer(&top),
        ViewCamera::Observer(&side),
    ];
    for initial in [ViewCamera::Camera(1), ViewCamera::Observer(&free)] {
        let base = sampled(&project, 12.5, initial);
        let retained = base.camera.view_projection;
        for choice in choices {
            let fork = base.fork_view(choice).unwrap();
            assert_projection(&fork, &sampled(&project, 12.5, choice));
            for point in [[48., 48.], [10., 10.], [40., 50.], [70., 50.]] {
                assert_eq!(
                    serde_json::to_value(fork.hit_candidates(point).unwrap()).unwrap(),
                    serde_json::to_value(
                        sampled(&project, 12.5, choice)
                            .hit_candidates(point)
                            .unwrap()
                    )
                    .unwrap()
                );
            }
        }
        assert_eq!(base.camera.view_projection, retained);
    }
}

#[test]
fn child_scenes_are_shared_until_resampling_and_keep_local_camera_bindings() {
    let project = nested_fixture();
    let evaluator = CountingEvaluator(Cell::new(0));
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample_view(&project, 12.5, ViewCamera::Composition, &evaluator)
        .unwrap();
    assert_eq!(
        evaluator.0.get(),
        3,
        "root and two actual composition instances"
    );
    let views = scene
        .fork_views(&[ViewCamera::Camera(1), ViewCamera::Camera(2)])
        .unwrap();
    assert_eq!(evaluator.0.get(), 3);
    let old = &views[0];
    for i in 0..2 {
        assert!(Arc::ptr_eq(&scene.nested[i].scene, &old.nested[i].scene));
        assert!(Arc::ptr_eq(&views[1].nested[i].scene, &old.nested[i].scene));
        let child = &old.nested[i].scene;
        let local = child.fork_view(ViewCamera::Camera(1)).unwrap();
        let authored_child = project.composition_frame_view("comp-retained-child").unwrap();
        // The evaluator changed geometry, but not cameras. Aliased child IDs
        // must still resolve the selected authored camera, rather than root 1.
        let oracle = sampled(&authored_child, 12.5, ViewCamera::Camera(1));
        assert_eq!(local.camera.view_projection, oracle.camera.view_projection);
        assert_ne!(local.camera.view_projection, child.camera.view_projection);
    }
    assert_ne!(
        old.nested[0].scene.layers[0].id,
        old.nested[1].scene.layers[0].id
    );
    let retained: Vec<_> = old
        .nested
        .iter()
        .map(|child| {
            (
                child.scene.frame,
                child.scene.layers[0].model,
                child.scene.camera.view_projection,
            )
        })
        .collect();
    scene
        .sample_view(&project, 30.25, ViewCamera::Composition, &evaluator)
        .unwrap();
    assert_eq!(evaluator.0.get(), 6);
    for i in 0..2 {
        assert!(!Arc::ptr_eq(&scene.nested[i].scene, &old.nested[i].scene));
        assert_eq!(
            (
                old.nested[i].scene.frame,
                old.nested[i].scene.layers[0].model,
                old.nested[i].scene.camera.view_projection
            ),
            retained[i]
        );
        assert_eq!(scene.nested[i].scene.frame, 30.25);
    }
    assert!(std::ptr::eq(
        old.sampled_project(&project),
        views[1].sampled_project(&project)
    ));
    assert!(!std::ptr::eq(
        old.sampled_project(&project),
        scene.sampled_project(&project)
    ));
}

#[test]
fn failed_samples_and_invalid_batches_cannot_publish_partial_or_stale_views() {
    let project = fixture();
    let mut scene = Scene::new(&project).unwrap();
    assert!(scene
        .fork_view(ViewCamera::Composition)
        .unwrap_err()
        .to_string()
        .contains("no successfully sampled frame"));
    scene
        .sample_view(
            &project,
            12.5,
            ViewCamera::Composition,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    let retained = scene.fork_view(ViewCamera::Camera(1)).unwrap();
    for count in [0, 3, 5] {
        assert!(scene
            .fork_views(&vec![ViewCamera::Composition; count])
            .is_err());
    }
    for id in [0, 7, 999] {
        let error = scene
            .fork_views(&[ViewCamera::Camera(1), ViewCamera::Camera(id)])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(&format!("object={id}"))
                && error.contains("frame=12.5")
                && error.contains("phase=resolve"),
            "{error}"
        );
        assert_projection(&retained, &scene.fork_view(ViewCamera::Camera(1)).unwrap());
    }
    scene
        .sample_view(
            &project,
            90.,
            ViewCamera::Composition,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap_err();
    assert!(scene.fork_view(ViewCamera::Camera(1)).is_err());
    assert_projection(&retained, &sampled(&project, 12.5, ViewCamera::Camera(1)));
    scene
        .sample_view(
            &project,
            20.,
            ViewCamera::Camera(999),
            &motion_core::ExpressionEvaluator,
        )
        .unwrap_err();
    assert!(scene.fork_view(ViewCamera::Composition).is_err());
    scene
        .sample_view(
            &project,
            21.,
            ViewCamera::Composition,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    assert_eq!(scene.fork_view(ViewCamera::Camera(1)).unwrap().frame, 21.);
}

#[test]
fn malformed_orthographic_observer_fails_without_changing_retained_views() {
    let project = fixture();
    let scene = sampled(&project, 12.5, ViewCamera::Composition);
    let retained = scene.fork_view(ViewCamera::Composition).unwrap();
    for distance in [0., f32::NAN, f32::INFINITY] {
        let mut observer = Observer::new(96, 96);
        observer.view = ObservationView::Top;
        observer.camera.radius.value = distance;
        let error = scene
            .fork_view(ViewCamera::Observer(&observer))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("phase=fork") && error.contains("invalid view camera pose"),
            "{error}"
        );
        assert_projection(
            &retained,
            &scene.fork_view(ViewCamera::Composition).unwrap(),
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_frame_views_render_and_pick_opposite_occlusion_without_moving_flat_layers() {
    let project = fixture();
    let base = sampled(&project, 12.5, ViewCamera::Composition);
    let views = base
        .fork_views(&[ViewCamera::Camera(1), ViewCamera::Camera(2)])
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(96, 96).unwrap();
    let mut captures = vec![];
    for ((id, front, color), view) in [(1, 7, [255, 0, 0, 255]), (2, 8, [0, 0, 255, 255])]
        .into_iter()
        .zip(&views)
    {
        renderer.retain_video_instances(view);
        let pixels = renderer.capture(view, &target).unwrap().0;
        assert_eq!(view.hit_candidates([48., 48.]).unwrap()[0].id, front);
        let center = (48 * 96 + 48) * 4;
        assert_eq!(&pixels[center..center + 4], &color);
        assert_eq!(view.hit_candidates([10., 10.]).unwrap()[0].id, 9);
        let flat = (10 * 96 + 10) * 4;
        assert_eq!(&pixels[flat..flat + 4], &[255, 255, 0, 255]);
        let oracle = sampled(&project, 12.5, ViewCamera::Camera(id));
        renderer.retain_video_instances(&oracle);
        assert_eq!(pixels, renderer.capture(&oracle, &target).unwrap().0);
        captures.push(pixels);
    }
    assert_ne!(
        captures[0], captures[1],
        "negative control rejects camera substitution"
    );
    renderer.retain_video_instances(&views[0]);
    assert_eq!(renderer.capture(&views[0], &target).unwrap().0, captures[0]);
    if let Some(path) = std::env::var_os("MOTION_MULTIVIEW_FRAME_ARTIFACTS") {
        let root = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&root).unwrap();
        for (name, pixels) in ["fork-front.png", "fork-back.png"]
            .into_iter()
            .zip(captures)
        {
            image::save_buffer(root.join(name), &pixels, 96, 96, image::ColorType::Rgba8).unwrap();
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn retained_nested_frame_pixels_survive_source_scene_resampling() {
    let project = nested_fixture();
    let mut base = sampled(&project, 0., ViewCamera::Composition);
    let retained = base.fork_view(ViewCamera::Camera(1)).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(96, 96).unwrap();
    renderer.retain_video_instances(&retained);
    let before = renderer.capture(&retained, &target).unwrap().0;
    base.sample_view(
        &project,
        60.,
        ViewCamera::Composition,
        &motion_core::ExpressionEvaluator,
    )
    .unwrap();
    let updated = base.fork_view(ViewCamera::Camera(1)).unwrap();
    renderer.retain_video_instances(&updated);
    let after = renderer.capture(&updated, &target).unwrap().0;
    assert_ne!(
        before, after,
        "negative control must detect replacing retained child frames"
    );
    renderer.retain_video_instances(&retained);
    assert_eq!(renderer.capture(&retained, &target).unwrap().0, before);
    let oracle = sampled(&project, 0., ViewCamera::Camera(1));
    renderer.retain_video_instances(&oracle);
    assert_eq!(renderer.capture(&oracle, &target).unwrap().0, before);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_fork_keeps_source_camera_letterbox_without_reopening_sources() {
    let root = tempfile::tempdir().unwrap();
    let source = imported::project(root.path(), true);
    let catalog = ModelSourceCatalog::prepare(&source, root.path(), &Default::default()).unwrap();
    let mut project = source.clone();
    let mut layer = Layer::camera(9, "Imported", project.width, project.height);
    layer.content = Content::Camera {
        camera: Box::new(project.camera.clone()),
    };
    project.camera = motion_core::Camera::new(project.width, project.height);
    project.layers.push(layer);
    project
        .layers
        .push(Layer::camera(10, "Other", project.width, project.height));
    project.validate().unwrap();
    let mut base = Scene::new(&project).unwrap();
    base.set_model_source_catalog(catalog.clone());
    base.sample_view(
        &project,
        15.5,
        ViewCamera::Composition,
        &motion_core::ExpressionEvaluator,
    )
    .unwrap();
    let mut oracle = Scene::new(&source).unwrap();
    oracle.set_model_source_catalog(catalog.clone());
    oracle
        .sample(&source, 15.5, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets_with_model_catalog(&project, root.path(), &catalog).unwrap();
    // Source files are deliberately unavailable before creating the new view.
    root.close().unwrap();
    let fork = base.fork_view(ViewCamera::Camera(9)).unwrap();
    assert_eq!(fork.camera_viewport, [16., 0., 64., 64.]);
    assert_eq!(fork.camera.view_projection, oracle.camera.view_projection);
    assert_eq!(
        fork.native_camera().view_projection,
        oracle.native_camera().view_projection
    );
    assert!(fork.hit_candidates([0., 32.]).unwrap().is_empty());
    let target = renderer
        .capture_target(project.width, project.height)
        .unwrap();
    renderer.retain_video_instances(&fork);
    let pixels = renderer.capture(&fork, &target).unwrap().0;
    assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 240).count() > 50,
        "the source fixture must render visible geometry");
    renderer.retain_video_instances(&oracle);
    assert_eq!(pixels, renderer.capture(&oracle, &target).unwrap().0);
    renderer.retain_video_instances(&base);
    assert_ne!(
        pixels,
        renderer.capture(&base, &target).unwrap().0,
        "negative control must detect substituting the active camera"
    );
}
