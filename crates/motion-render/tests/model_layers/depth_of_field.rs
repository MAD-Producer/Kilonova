use super::*;
use motion_model::{DepthOfField, Track};
#[path = "../support/imported_camera.rs"]
mod imported;

#[test]
fn imported_aperture_uses_selected_camera_local_clock_and_observer_replaces_it() {
    let root = tempfile::tempdir().unwrap();
    let mut p = imported::project(root.path(), false);
    let mut focus = Track::constant(60.);
    focus.set_animated(0, true).unwrap();
    focus.set_at(10, 120.).unwrap();
    let mut selected = p.camera.clone();
    selected.depth_of_field = Some(DepthOfField {
        focus_distance: focus,
        aperture_diameter: Track::constant(10.),
        max_radius_pixels: 8.,
    });
    let mut camera = Layer::camera(90, "Independent aperture clock", 96, 64);
    camera.content = Content::Camera {
        camera: Box::new(selected),
    };
    camera.timeline = Some(motion_core::LayerTimeline {
        in_frame: 10,
        out_frame: 61,
        offset_frame: 10,
    });
    p.layers.insert(0, camera);
    p.validate().unwrap();
    let catalog =
        motion_render::ModelSourceCatalog::prepare(&p, root.path(), &Default::default()).unwrap();
    let restored: Project = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    let mut scene = Scene::new(&restored).unwrap();
    scene.set_model_source_catalog(catalog);
    scene
        .sample(&restored, 5., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert!(scene.native_depth_of_field().is_none());
    for (frame, focus) in [(10., 60.), (20., 120.)] {
        scene
            .sample(&restored, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        assert_eq!(scene.native_depth_of_field().unwrap().focus_distance, focus);
    }
    let observer = motion_core::Observer::new(96, 64);
    scene
        .sample(
            &restored,
            20.,
            Some(&observer),
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    assert!(scene.native_depth_of_field().is_none());
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn a_single_owned_model_uses_saved_camera_aperture_and_disabled_aperture_is_exact() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    p.camera.created = true;
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&p, root.path()).unwrap();
    let target = renderer.capture_target(96, 64).unwrap();
    let sample = |p: &Project| {
        let mut s = Scene::new(p).unwrap();
        s.sample(p, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        s
    };
    let sharp = renderer.capture(&sample(&p), &target).unwrap().0;
    assert!(sharp.iter().any(|&v| v != 0));
    p.camera.depth_of_field = Some(DepthOfField {
        focus_distance: Track::constant(200.),
        aperture_diameter: Track::constant(100.),
        max_radius_pixels: 8.,
    });
    p.validate().unwrap();
    let blurred = renderer.capture(&sample(&p), &target).unwrap().0;
    assert_ne!(
        blurred, sharp,
        "a one-model scene must not bypass the aperture pass"
    );
    p.camera
        .depth_of_field
        .as_mut()
        .unwrap()
        .aperture_diameter
        .value = 0.;
    assert_eq!(renderer.capture(&sample(&p), &target).unwrap().0, sharp);
}
