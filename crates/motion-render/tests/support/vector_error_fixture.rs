use motion_core::{ExpressionEvaluator, Layer, Project};
use motion_model::vector::{
    groups::PaintBatch, FillRule, LineCap, LineJoin, SampledDashes, SampledPath, SampledTrimPaths,
    SampledVector, TrimMode,
};
use motion_render::{effect_plan::EffectFramePlan, Scene};
use std::sync::Arc;

pub fn healthy() -> SampledVector {
    SampledVector {
        paths: vec![SampledPath {
            closed: true,
            nodes: [[-8., -8.], [8., -8.], [8., 8.], [-8., 8.]]
                .into_iter()
                .map(|p| [p[0], p[1], 0., 0., 0., 0.])
                .collect(),
        }],
        fill: Some([1.; 4]),
        fill_rule: FillRule::NonZero,
        stroke: Some(([1.; 4], 2., LineCap::Round, LineJoin::Round, 4.)),
        trim: None,
        dashes: None,
        batches: None,
        root_opacity: 1.,
        group_parameters: Default::default(),
    }
}

pub fn scene() -> (Project, Scene) {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project
        .layers
        .push(Layer::solid(7, "Vector", [32.; 2], [32., 32., 0.], [1.; 4]));
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 5.5, None, &ExpressionEvaluator)
        .unwrap();
    scene.layers[0].vector = Some(Arc::new(healthy()));
    (project, scene)
}

pub fn malformed() -> Vec<(&'static str, SampledVector)> {
    let mut trim = healthy();
    trim.trim = Some(SampledTrimPaths {
        start: -1.,
        end: 100.,
        offset: 0.,
        mode: TrimMode::Simultaneously,
    });
    let mut dash = healthy();
    dash.dashes = Some(SampledDashes {
        pattern: vec![1.],
        offset: 0.,
    });
    let mut nan = healthy();
    nan.paths[0].nodes[0][0] = f32::NAN;
    let mut infinite = healthy();
    infinite.paths[0].nodes[2][1] = f32::INFINITY;
    let mut overflow = healthy();
    overflow.paths[0].nodes[0][0] = f32::MAX;
    overflow.paths[0].nodes[0][4] = f32::MAX;
    let mut grouped = healthy();
    grouped.batches = Some(
        [healthy(), trim.clone()]
            .into_iter()
            .map(|vector| PaintBatch {
                vector: Box::new(vector),
                transform: [1., 0., 0., 0., 1., 0., 0., 0., 1.],
                scopes: vec![],
            })
            .collect(),
    );
    vec![
        ("trim", trim),
        ("dash", dash),
        ("NaN", nan),
        ("Infinity", infinite),
        ("overflow", overflow),
        ("group trim", grouped),
    ]
}

pub fn bits(plan: &EffectFramePlan) -> Vec<u32> {
    assert_eq!(plan.vectors.len(), 1);
    let mesh = &plan.vectors[0];
    assert!(!mesh.vertices.is_empty());
    mesh.vertices
        .iter()
        .flat_map(|v| v.position.into_iter().chain(v.color))
        .map(f32::to_bits)
        .collect()
}
