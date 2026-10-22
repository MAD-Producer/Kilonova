use motion_core::{Composition, CompositionClip, Content, Error, Layer, Project};
use motion_render::{image_resources::upcoming_assets, Scene};

fn fixture() -> Project {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    let mut reference = Layer::solid(1, "Reference", [64.; 2], [0.; 3], [1.; 4]);
    reference.content = Content::Composition {
        clip: CompositionClip::new("comp-child".into()),
    };
    project.layers.push(reference);
    project.compositions.push(Composition {
        id: "comp-child".into(),
        name: "Child".into(),
        width: 64,
        height: 64,
        fps: 60,
        frames: 240,
        background: [0.; 4],
        camera: project.camera.clone(),
        layers: vec![Layer::solid(1, "Child solid", [32.; 2], [0.; 3], [1.; 4])],
        expressions: vec![],
    });
    project.validate().unwrap();
    project
}

fn check(error: Error, composition: &str, phase: &str, reference: bool) {
    let Error::Composition(error) = error else {
        panic!("expected composition error: {error}")
    };
    assert_eq!(error.code, "invalid_frame_rate");
    assert_eq!(error.composition, composition);
    assert_eq!(error.details["phase"], phase);
    if reference {
        assert_eq!(error.details["parent_composition"], "comp-main");
        assert_eq!(error.details["reference"], 1);
    } else {
        assert!(error.details["reference"].is_null());
    }
}

#[test]
fn sampling_and_prefetch_propagate_rate_errors_and_recover_after_correction() {
    for parent_invalid in [false, true] {
        let mut project = fixture();
        let mut scene = Scene::new(&project).unwrap();
        if parent_invalid {
            project.fps = 0;
        } else {
            project.compositions[0].fps = 0;
        }
        check(
            scene
                .sample(&project, 5., None, &motion_core::ExpressionEvaluator)
                .unwrap_err(),
            "comp-child",
            "scene_sample",
            true,
        );
        check(
            upcoming_assets(&project, 5.).unwrap_err(),
            if parent_invalid {
                "comp-main"
            } else {
                "comp-child"
            },
            "image_prefetch",
            !parent_invalid,
        );
        project.fps = 30;
        project.compositions[0].fps = 60;
        scene
            .sample(&project, 5., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        assert_eq!(scene.nested.len(), 1);
        assert_eq!(scene.nested[0].scene.frame, 10.);
        assert_eq!(scene.nested[0].scene.layers.len(), 1);
        assert!(upcoming_assets(&project, 5.).unwrap().is_empty());
    }
}

#[test]
fn valid_out_of_range_clips_and_non_finite_prefetch_queries_stay_normal_misses() {
    let mut project = fixture();
    let Content::Composition { clip } = &mut project.layers[0].content else {
        unreachable!()
    };
    clip.source_start_frame = -120;
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert!(scene.nested.is_empty());
    assert!(upcoming_assets(&project, 0.).unwrap().is_empty());
    for frame in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.] {
        assert!(upcoming_assets(&project, frame).unwrap().is_empty());
    }
}
