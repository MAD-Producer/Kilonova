//! Deterministic filesystem change between host-style validation and renderer
//! preparation, with an actual GPU catalog/texture that must survive failure.
use motion_core::{Asset, Command, Content, Engine, Error, Layer, Project, Property};
use motion_render::{image_resources::Resolution, Renderer, Scene};

#[test]
fn resource_resolution_failure_keeps_gpu_pixels_history_and_gesture() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let path = root.path().join("assets/card.png");
    image::save_buffer(
        &path,
        &[255, 0, 0, 255].repeat(16 * 16),
        16,
        16,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let source = std::fs::read(&path).unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.assets.push(Asset {
        id: 1,
        path: "assets/card.png".into(),
        width: 16,
        height: 16,
    });
    let mut card = Layer::solid(1, "Image", [32.; 2], [32., 32., 0.], [1.; 4]);
    card.content = Content::Image { asset: 1 };
    project.layers.push(card);
    let mut engine = Engine::new(project).unwrap();
    let initial = engine.snapshot();
    engine
        .apply(Command::SetVector {
            object: 1,
            property: Property::Position,
            frame: 0,
            value: [40., 32., 0.],
        })
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer
        .configure_assets(engine.project(), root.path())
        .unwrap();
    let mut scene = Scene::new(engine.project());
    scene
        .sample(
            engine.project(),
            0.,
            None,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    renderer
        .prepare_scene_assets(&scene, Resolution::Full, false)
        .unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    let pixels = renderer.capture(&scene, &target).unwrap().0;
    assert!(pixels
        .chunks_exact(4)
        .any(|pixel| pixel == [255, 0, 0, 255]));
    let bytes = renderer.texture_bytes();
    let catalog = renderer.asset_ids().to_vec();
    // Resource no-op: the previous undo must not be consumed on failure.
    let content = Command::Content {
        object: 1,
        content: Content::Image { asset: 1 },
        size: [32.; 2],
    };
    for gesture in [false, true] {
        if gesture {
            engine.begin_gesture().unwrap();
        }
        let before = engine.snapshot();
        let revision = engine.revision();
        let result = engine.apply_batch_with_prepare(vec![content.clone()], |candidate| {
            motion_core::storage::validate_assets(root.path(), candidate)?;
            std::fs::remove_file(&path)?; // Reproduce a real TOCTOU, deterministically.
            renderer
                .configure_assets(candidate, root.path())
                .map_err(|error| Error::Invalid(error.to_string()))
        });
        assert!(result.is_err());
        assert_eq!(before, engine.snapshot());
        assert_eq!(revision, engine.revision());
        assert_eq!(engine.gesture_active(), gesture);
        assert_eq!(renderer.asset_ids(), catalog.as_slice());
        assert_eq!(renderer.texture_bytes(), bytes);
        assert_eq!(renderer.image_dimensions(1), Some((16, 16)));
        // Even with the source gone, the already loaded valid texture survives.
        assert_eq!(renderer.capture(&scene, &target).unwrap().0, pixels);
        std::fs::write(&path, &source).unwrap();
        if gesture {
            engine.end_gesture(false).unwrap();
        }
    }
    engine.undo().unwrap();
    assert_eq!(initial, engine.snapshot());
    engine.redo().unwrap();
    assert_eq!(engine.project().layers[0].transform.position.value[0], 40.);
    engine
        .apply_batch_with_prepare(
            vec![Command::SetVector {
                object: 1,
                property: Property::Position,
                frame: 0,
                value: [45., 32., 0.],
            }],
            |candidate| {
                motion_core::storage::validate_assets(root.path(), candidate)?;
                renderer
                    .configure_assets(candidate, root.path())
                    .map_err(|error| Error::Invalid(error.to_string()))
            },
        )
        .unwrap();
    engine.undo().unwrap();
    assert_eq!(engine.project().layers[0].transform.position.value[0], 40.);
}
