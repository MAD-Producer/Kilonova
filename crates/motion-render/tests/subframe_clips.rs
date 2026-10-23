use motion_core::{
    Camera, ClipRange, Composition, CompositionClip, Content, ExpressionEvaluator, Layer, Project,
};
use motion_render::{Renderer, Scene};
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn nested_fractional_visibility_uses_exact_bounds_during_random_seek() {
    let mut p = Project::new(64, 64, 24, 48).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.background = [0., 0., 1., 1.];
    p.camera.created = false;
    let mut leaf = Layer::solid(1, "red", [64.; 2], [32., 32., 0.], [1., 0., 0., 1.]);
    leaf.set_interval(
        96,
        ClipRange {
            in_frame: 0.25,
            out_frame: 0.75,
        },
    )
    .unwrap();
    let mut camera = Camera::new(64, 64);
    camera.created = false;
    p.compositions.push(Composition {
        id: "comp-child".into(),
        name: "child".into(),
        width: 64,
        height: 64,
        fps: 48,
        frames: 96,
        background: [0.; 4],
        camera,
        layers: vec![leaf],
        expressions: vec![],
    });
    let mut parent = Layer::solid(2, "reference", [64.; 2], [32., 32., 0.], [1.; 4]);
    parent.content = Content::Composition {
        clip: CompositionClip::new("comp-child".into()),
    };
    p.layers.push(parent);
    p.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    let mut first = None;
    for time in [0.125, 0.375, 0.1249, 0.3749, 0.125] {
        let mut scene = Scene::new(&p).unwrap();
        scene.sample(&p, time, None, &ExpressionEvaluator).unwrap();
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        let expected = if (0.125..0.375).contains(&time) {
            [255, 0, 0, 255]
        } else {
            [0, 0, 255, 255]
        };
        assert_eq!(
            &pixels[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4],
            &expected
        );
        if time == 0.125 {
            if let Some(old) = &first {
                assert_eq!(old, &pixels);
            }
            first = Some(pixels);
        }
    }
}
