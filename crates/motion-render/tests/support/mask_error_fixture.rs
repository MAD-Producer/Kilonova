use motion_core::{ExpressionEvaluator, Layer, Project};
use motion_model::masks::{MaskMode, SampledMask};
use motion_render::{effect_plan::EffectFramePlan, Scene};
use std::sync::Arc;

pub fn mask(id: u64) -> SampledMask {
    SampledMask {
        id,
        nodes: [[8., 8.], [24., 8.], [24., 24.], [8., 24.]]
            .into_iter()
            .map(|p| [p[0], p[1], 0., 0., 0., 0.])
            .collect(),
        mode: MaskMode::Add,
        inverted: false,
        opacity: 1.,
        feather: [0.; 2],
        expansion: 0.,
    }
}

pub fn scene() -> (Project, Scene) {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project
        .layers
        .push(Layer::solid(7, "Masked", [32.; 2], [32., 32., 0.], [1.; 4]));
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 5.5, None, &ExpressionEvaluator)
        .unwrap();
    scene.layers[0].masks = Arc::new(vec![mask(19)]);
    (project, scene)
}

pub fn malformed() -> Vec<(&'static str, SampledMask)> {
    let mut nan = mask(19);
    nan.nodes[0][0] = f32::NAN;
    let mut infinite = mask(19);
    infinite.nodes[2][1] = f32::INFINITY;
    let mut overflow = mask(19);
    overflow.nodes[0][0] = f32::MAX;
    overflow.nodes[0][4] = f32::MAX;
    vec![("NaN", nan), ("Infinity", infinite), ("overflow", overflow)]
}

pub fn bits(plan: &EffectFramePlan) -> Vec<u32> {
    assert_eq!(plan.masks.len(), 1);
    assert!(!plan.masks[0].vertices.is_empty());
    plan.masks[0]
        .vertices
        .iter()
        .flat_map(|v| v.position.into_iter().chain(v.color))
        .map(f32::to_bits)
        .collect()
}
