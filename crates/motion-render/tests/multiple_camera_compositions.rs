use motion_core::{
    Camera, ClipRange, Composition, CompositionClip, Content, Ease, Layer, LayerTimeline, Project,
};
use motion_render::{Renderer, Scene};

const CHILD: &str = "comp-camera-child";

fn camera(id: u64, distance: f32) -> Layer {
    let mut layer = Layer::camera(id, "Camera", 96, 96);
    let Content::Camera { camera } = &mut layer.content else {
        unreachable!()
    };
    camera.position.value = [48., 48., -distance];
    layer
}

fn fixture() -> Project {
    let mut project = Project::new(96, 96, 30, 90).unwrap();
    project.version = 19; // Authored camera layers use the established format gate.
    project.background = [0.; 4];
    let mut root_top = camera(2, 200.);
    root_top.timeline = Some(LayerTimeline {
        in_frame: 8,
        out_frame: 20,
        offset_frame: 0,
    });
    root_top.subframe_range = Some(ClipRange {
        in_frame: 8.,
        out_frame: 20.,
    });
    let mut child_top = camera(2, 150.);
    child_top.locked = true; // Locking protects edits, rather than disabling a view.
    child_top.subframe_range = Some(ClipRange {
        in_frame: 18.5,
        out_frame: 42.5,
    });
    child_top.timeline = Some(LayerTimeline {
        in_frame: 18,
        out_frame: 43,
        offset_frame: 5,
    });
    let Content::Camera {
        camera: child_camera,
    } = &mut child_top.content
    else {
        unreachable!()
    };
    child_camera
        .position
        .upsert(0, [48., 48., -150.], Ease::Linear)
        .unwrap();
    child_camera
        .position
        .upsert(60, [48., 48., -210.], Ease::Linear)
        .unwrap();
    let mut child_shape = Layer::solid(
        3,
        "Child green",
        [28., 38.],
        [36., 48., 0.],
        [0., 1., 0., 1.],
    );
    child_shape.three_d = true;
    child_shape.transform.rotation.value[1] = 20.;
    let mut legacy = Camera::new(96, 96);
    legacy.created = false;
    project.compositions.push(Composition {
        id: CHILD.into(),
        name: "Camera child".into(),
        width: 96,
        height: 96,
        fps: 60,
        frames: 180,
        background: [0.; 4],
        camera: legacy,
        layers: vec![camera(1, 100.), child_top, child_shape],
        expressions: vec![],
    });
    let mut reference = Layer::solid(3, "Child reference", [96.; 2], [48., 48., 0.], [1.; 4]);
    reference.content = Content::Composition {
        clip: CompositionClip {
            source_start_frame: 12,
            ..CompositionClip::new(CHILD.into())
        },
    };
    reference.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 90,
        offset_frame: 3,
    });
    let mut root_shape = Layer::solid(4, "Root red", [18., 30.], [75., 48., 0.], [1., 0., 0., 1.]);
    root_shape.three_d = true;
    project.layers = vec![camera(1, 120.), root_top, reference, root_shape];
    project.validate().unwrap();
    project
}

// The expected choices and time mapping are independent of active_camera_layer.
fn expected_camera(frame: f64, child: bool) -> Camera {
    let mut camera = Camera::new(96, 96);
    let distance = if child {
        if (18.5..42.5).contains(&frame) {
            150. + (frame - 5.) as f32
        } else {
            100.
        }
    } else if (8. ..20.).contains(&frame) {
        200.
    } else {
        120.
    };
    camera.position.value = [48., 48., -distance];
    camera
}

fn assert_pose(scene: &Scene, expected: &Camera, frame: f64) {
    let expected = expected.pose(frame, 96, 96);
    assert!(
        scene
            .camera
            .view_projection
            .abs_diff_eq(expected.view_projection, 1e-6),
        "camera differs at frame {frame}: actual={:?} expected={:?}",
        scene.camera.eye,
        expected.eye
    );
}

#[test]
fn repeated_camera_ids_are_local_to_each_composition_at_fractional_clip_boundaries() {
    let project = fixture();
    let mut scene = Scene::new(&project);
    for frame in [0., 6.249, 6.25, 8., 18.249, 18.25, 20., 8., 0., 18.25] {
        scene
            .sample(&project, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let source = 12. + (frame - 3.) * 2.;
        assert_pose(&scene, &expected_camera(frame, false), frame);
        assert_eq!(scene.nested.len(), 1);
        let child = &scene.nested[0].scene;
        assert_eq!(child.frame, source);
        assert_pose(child, &expected_camera(source, true), source);
        assert_eq!(
            child.layers.len(),
            1,
            "camera layers must not become pixels"
        );
    }
}

fn capture(renderer: &mut Renderer, scene: &Scene) -> Vec<u8> {
    renderer.retain_video_instances(scene);
    let target = renderer.capture_target(96, 96).unwrap();
    renderer.capture(scene, &target).unwrap().0
}

fn sampled(project: &Project, frame: f64) -> Scene {
    let mut scene = Scene::new(project);
    scene
        .sample(project, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
fn nested_camera_pixels_match_independent_legacy_views_after_save_and_random_seek() {
    let project = fixture();
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), &project).unwrap();
    let frozen = motion_core::storage::load(directory.path()).unwrap();
    assert_eq!(frozen, project);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut plans = motion_render::effect_plan::PlanBuilder::new(
        motion_effects::Registry::new_with_builtins().unwrap(),
    )
    .unwrap();
    let mut reused = Scene::new(&frozen);
    for frame in [0., 6.249, 6.25, 8., 18.249, 18.25, 20., 8., 0.] {
        reused
            .sample(&frozen, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let actual = capture(&mut renderer, &reused);
        let mut oracle = frozen.clone();
        oracle.camera = expected_camera(frame, false);
        // Preserve object/order identities in the export protocol while forcing
        // independently selected legacy views in each composition.
        for layer in &mut oracle.layers {
            if matches!(layer.content, Content::Camera { .. }) {
                layer.visible = false;
            }
        }
        let source = 12. + (frame - 3.) * 2.;
        oracle.compositions[0].camera = expected_camera(source, true);
        for layer in &mut oracle.compositions[0].layers {
            if matches!(layer.content, Content::Camera { .. }) {
                layer.visible = false;
            }
        }
        oracle.validate().unwrap();
        let oracle_scene = sampled(&oracle, frame);
        assert_eq!(
            actual,
            capture(&mut renderer, &oracle_scene),
            "frame {frame}"
        );
        let mut actual_plan = Vec::new();
        let mut expected_plan = Vec::new();
        motion_render::composition_plan::build(
            &mut plans,
            &reused,
            &frozen,
            &[0],
            &mut actual_plan,
        )
        .unwrap();
        motion_render::composition_plan::build(
            &mut plans,
            &oracle_scene,
            &oracle,
            &[0],
            &mut expected_plan,
        )
        .unwrap();
        assert_eq!(
            actual_plan, expected_plan,
            "export plan differs at frame {frame}"
        );
        if frame == 8. {
            let mut wrong_child = oracle.clone();
            wrong_child.compositions[0].camera = expected_camera(0., true);
            assert_ne!(
                actual,
                capture(&mut renderer, &sampled(&wrong_child, frame)),
                "child fixture must detect the wrong camera"
            );
            oracle.camera = expected_camera(0., false);
            assert_ne!(
                actual,
                capture(&mut renderer, &sampled(&oracle, frame)),
                "root fixture must detect the wrong camera"
            );
        }
    }
    assert_eq!(
        frozen, project,
        "sampling must not mutate the saved snapshot"
    );
}

#[test]
fn child_legacy_and_implicit_fallbacks_do_not_replace_the_parent_camera() {
    let mut project = fixture();
    for layer in &mut project.compositions[0].layers {
        if matches!(layer.content, Content::Camera { .. }) {
            layer.visible = false;
        }
    }
    project.compositions[0].camera.position.value = [48., 48., -70.];
    for legacy_created in [true, false, true] {
        project.compositions[0].camera.created = legacy_created;
        project.validate().unwrap();
        let scene = sampled(&project, 8.);
        assert_pose(&scene, &expected_camera(8., false), 8.);
        let child = &scene.nested[0].scene;
        let expected = if legacy_created {
            project.compositions[0].camera.clone()
        } else {
            Camera::new(96, 96)
        };
        assert_pose(child, &expected, 22.);
    }
}
