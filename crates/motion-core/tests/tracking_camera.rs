use motion_core::*;
#[path = "../../motion-model/tests/support/tracking_camera.rs"]
mod fixture;
#[test]
fn independent_camera_binding_reaches_real_scene_and_atomic_undo_without_baking() {
    let p = fixture::camera_fixture();
    let binding = p.camera.tracking.clone().unwrap();
    let mut initial = p.clone();
    initial.camera.tracking = None;
    let mut engine = Engine::new(initial.clone()).unwrap();
    engine
        .apply(Command::SetTrackingCamera {
            object: 0,
            binding: Some(binding.clone()),
        })
        .unwrap();
    let bound = engine.project().clone();
    let mut scene = motion_render::Scene::new(&bound).unwrap();
    scene
        .sample(&bound, 1., None, &ExpressionEvaluator)
        .unwrap();
    assert!((scene.camera.eye.x - 10.).abs() < 0.0001);
    assert!((scene.camera.eye.y + 5.).abs() < 0.0001);
    let mut bad = binding;
    bad.track = "absent".into();
    let revision = engine.revision();
    let err = engine
        .apply(Command::SetTrackingCamera {
            object: 0,
            binding: Some(bad),
        })
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("phase=binding object=0 asset=2 track=absent"),
        "{err}"
    );
    assert_eq!(engine.project(), &bound);
    assert_eq!(engine.revision(), revision);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &initial);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &bound);
    assert!(bound.camera.position.keys.is_empty());
}
#[test]
fn camera_binding_requires_explicit_nullable_payload_in_catalogue() {
    let v = CommandRegistry::catalogue();
    let s = &v["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "set_tracking_camera")
        .unwrap()["inputSchema"];
    assert!(s["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "binding"));
    let b = s["properties"]["binding"]["anyOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["type"] == "object")
        .unwrap();
    assert_eq!(b["additionalProperties"], false);
    assert!(b["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "correction_reference"));
    assert_eq!(
        CommandRegistry::parse(r#"{"op":"set_tracking_camera","object":0,"binding":null}"#)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn inactive_tracking_camera_keeps_its_failure_for_explicit_views_and_recovers() {
    let mut p = fixture::camera_fixture();
    let mut layer = Layer::camera(7, "tracked camera", p.width, p.height);
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 2,
        offset_frame: 0,
    });
    if let Content::Camera { camera } = &mut layer.content {
        **camera = p.camera.clone();
    }
    p.camera = Camera::new(p.width, p.height);
    p.layers.push(layer);
    p.validate().unwrap();
    let mut scene = motion_render::Scene::new(&p).unwrap();
    scene.sample(&p, 0., None, &ExpressionEvaluator).unwrap();
    let tracked = scene
        .fork_view(motion_render::ViewCamera::Camera(7))
        .unwrap();
    assert!((tracked.camera.eye.z - 500.).abs() < 0.0001);
    // The cut returns to the ordinary composition camera. A now unavailable
    // inactive source must neither fail this shot nor publish an identity pose.
    scene.sample(&p, 80., None, &ExpressionEvaluator).unwrap();
    assert!(scene.world_matrix(7).is_none());
    assert!(scene.node_position(7).is_none());
    let error = scene
        .fork_view(motion_render::ViewCamera::Camera(7))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("viewer_camera object=7 phase=selection"),
        "{error}"
    );
    assert!(
        error.contains("phase=sample object=7 asset=2 track=feature local_frame=80"),
        "{error}"
    );
    assert!(error.contains("outside source PTS range"), "{error}");
    // Returning to a valid frame clears the retained cause and resolves again.
    scene.sample(&p, 1., None, &ExpressionEvaluator).unwrap();
    assert!(scene.world_matrix(7).is_some());
    assert!(
        (scene
            .fork_view(motion_render::ViewCamera::Camera(7))
            .unwrap()
            .camera
            .eye
            .x
            - 10.)
            .abs()
            < 0.0001
    );
    // A camera actually selected for the shot still propagates its failure.
    p.layers[0].timeline.as_mut().unwrap().out_frame = 90;
    let error = scene
        .sample(&p, 80., None, &ExpressionEvaluator)
        .unwrap_err()
        .to_string();
    assert!(error.contains("phase=sample object=7 asset=2"), "{error}");
}

#[test]
fn an_inactive_tracking_camera_used_as_a_spatial_parent_remains_strict() {
    let mut p = fixture::camera_fixture();
    let mut camera = Layer::camera(7, "parent camera", p.width, p.height);
    camera.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 2,
        offset_frame: 0,
    });
    if let Content::Camera { camera } = &mut camera.content {
        **camera = p.camera.clone();
    }
    p.camera = Camera::new(p.width, p.height);
    let mut child = Layer::solid(8, "child", [12., 12.], [320., 240., 0.], [1.; 4]);
    child.three_d = true;
    child.parent = Some(ParentLink {
        object: Some(7),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    p.layers = vec![camera, child];
    p.validate().unwrap();
    let mut scene = motion_render::Scene::new(&p).unwrap();
    let error = scene
        .sample(&p, 80., None, &ExpressionEvaluator)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("phase=sample object=7 asset=2 track=feature local_frame=80"),
        "{error}"
    );
}
