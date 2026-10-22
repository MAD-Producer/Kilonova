use motion_core::{ExpressionEvaluator, ProjectExpressions};
use motion_model::{
    Camera, Composition, CompositionClip, Content, ExpressionTarget, FrameEvaluator, Layer,
    LayerTimeline, Project, Property, PropertyExpression, Result, EXPRESSION_PROFILE,
};
use motion_render::Scene;
use std::{borrow::Cow, cell::RefCell, sync::OnceLock};

#[test]
fn borrowed_evaluation_cache_keeps_snapshot_values_equal_to_rendered_geometry() {
    static CACHE: OnceLock<Project> = OnceLock::new();
    struct CacheEvaluator(&'static Project);
    impl FrameEvaluator for CacheEvaluator {
        fn evaluate<'a>(&self, _: &'a Project, _: f64) -> Result<Cow<'a, Project>> {
            Ok(Cow::Borrowed(self.0))
        }
    }
    let mut original = Project::new(100, 100, 30, 120).unwrap();
    original
        .layers
        .push(Layer::solid(1, "Cached", [10.; 2], [20., 50., 0.], [1.; 4]));
    let cached = CACHE.get_or_init(|| {
        let mut cached = original.clone();
        cached.layers[0].transform.position.value[0] = 80.;
        cached
    });
    let mut scene = Scene::new(&original).unwrap();
    scene
        .sample(&original, 0., None, &CacheEvaluator(cached))
        .unwrap();
    assert!((scene.node_position(1).unwrap()[0] - 80.).abs() < 1e-5);
    assert_eq!(scene.sampled_project(&original), cached);
    assert_eq!(original.layers[0].transform.position.value[0], 20.);
}

struct RecordingEvaluator(RefCell<Vec<(String, f64)>>);
impl FrameEvaluator for RecordingEvaluator {
    fn evaluate<'a>(&self, project: &'a Project, frame: f64) -> Result<Cow<'a, Project>> {
        self.0
            .borrow_mut()
            .push((project.composition_id.clone(), frame));
        ExpressionEvaluator.evaluate(project, frame)
    }
}

fn reference(id: u64, composition: &str, source: i32, offset: i32, frames: u32) -> Layer {
    let mut layer = Layer::solid(id, "Reference", [100.; 2], [50., 50., 0.], [1.; 4]);
    layer.content = Content::Composition {
        clip: CompositionClip {
            composition: composition.into(),
            source_start_frame: source,
            volume: 1.,
            muted: false,
        },
    };
    layer.timeline = Some(LayerTimeline {
        in_frame: offset,
        out_frame: frames as i32,
        offset_frame: offset,
    });
    layer
}

#[test]
fn nested_compositions_use_the_real_evaluator_at_fractional_source_frames() {
    let mut project = Project::new(100, 100, 30, 120).unwrap();
    project.layers.push(reference(1, "comp-child", 12, 5, 120));
    project.compositions.push(Composition {
        id: "comp-child".into(),
        name: "Child".into(),
        width: 100,
        height: 100,
        fps: 60,
        frames: 240,
        background: [0.; 4],
        camera: Camera::new(100, 100),
        layers: vec![reference(2, "comp-leaf", 3, 7, 240)],
        expressions: vec![],
    });
    project.compositions.push(Composition {
        id: "comp-leaf".into(),
        name: "Leaf".into(),
        width: 100,
        height: 100,
        fps: 24,
        frames: 96,
        background: [0.; 4],
        camera: Camera::new(100, 100),
        layers: vec![Layer::solid(
            3,
            "Animated",
            [10.; 2],
            [50., 50., 0.],
            [1.; 4],
        )],
        expressions: vec![PropertyExpression {
            target: ExpressionTarget::Property {
                object: 3,
                property: Property::Position,
                axis: Some(motion_model::Axis::X),
            },
            source: "time * 24".into(),
            enabled: true,
            seed: 42,
            profile: EXPRESSION_PROFILE.into(),
        }],
    });
    project.validate().unwrap();
    // Set the expression through the production authoring path before sampling.
    // It compiles the source before starting evaluation's existing frame budget;
    // constructing PropertyExpression directly also times cold JS compilation.
    let expression = project.compositions[1].expressions[0].clone();
    let mut editor = motion_core::Engine::new(project).unwrap();
    editor.apply(motion_core::Command::InComposition {
        composition: "comp-leaf".into(),
        command: Box::new(motion_core::Command::SetExpression { expression, frame: 0 }),
    }).unwrap();
    let project = editor.snapshot();
    let original = project.clone();
    let evaluator = RecordingEvaluator(RefCell::new(vec![]));
    let mut scene = Scene::new(&project).unwrap();
    scene.sample(&project, 10.25, None, &evaluator).unwrap();
    let calls = evaluator.0.borrow();
    assert_eq!(calls.len(), 3);
    for ((id, frame), (expected_id, expected_frame)) in calls.iter().zip([
        ("comp-main", 10.25),
        ("comp-child", 22.5),
        ("comp-leaf", 9.2),
    ]) {
        assert_eq!(id, expected_id);
        assert!((frame - expected_frame).abs() < 1e-10);
    }
    let leaf = &scene.nested[0].scene.nested[0].scene;
    let stored_leaf = project.composition_frame_view("comp-leaf").unwrap();
    let evaluated = leaf.sampled_project(&stored_leaf);
    assert!((evaluated.layers[0].transform.position.sample(9.2).unwrap()[0] - 9.2).abs() < 1e-5);
    assert_eq!(project, original);
    assert_eq!(
        *evaluated,
        stored_leaf.evaluated_at(9.2).unwrap().into_owned()
    );
}
