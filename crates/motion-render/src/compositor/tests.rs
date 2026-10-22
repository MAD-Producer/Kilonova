use super::*;
use motion_model::{Layer, Project};

#[test]
fn missing_and_out_of_range_planes_return_context_without_adding_nodes() {
    let mut compositor = PlaneCompositor::new();
    for (layer, split) in [0, MAX_LAYERS, usize::MAX].into_iter()
        .flat_map(|layer| [false, true].map(|split| (layer, split))) {
        let error = compositor.node(Polygon { layer, vertices: 0..4, split }).unwrap_err();
        assert!(matches!(error, motion_model::Error::Invalid(_)));
        let reason = error.to_string();
        assert!(reason.contains("spatial compositor node missing nondegenerate plane"));
        assert!(reason.contains(&format!("layer_index={layer}")));
        assert!(compositor.nodes.is_empty());
        assert!(compositor.vertices.is_empty() && compositor.batches.is_empty());
    }
}

#[test]
fn prepare_preserves_split_layer_planes_and_replaces_degenerate_planes_on_reuse() {
    let mut project = Project::new(256, 256, 30, 180).unwrap();
    for (id, rotation) in [(1, 45.), (2, -45.), (3, 0.)] {
        let mut layer = Layer::solid(id, "spatial plane", [256.; 2], [128., 128., 0.], [1.; 4]);
        layer.three_d = true;
        layer.transform.rotation.value[1] = rotation;
        project.layers.push(layer);
    }
    let mut scene = Scene::new(&project).unwrap();
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    assert_eq!(scene.layers.len(), 3);
    let degenerate = glam::Mat4::from_scale(glam::Vec3::new(0., 1., 1.));
    scene.layers[2].model = degenerate;
    let mut compositor = PlaneCompositor::new();
    compositor.prepare(&scene).unwrap();
    assert!(compositor.planes[0].is_some() && compositor.planes[1].is_some());
    assert!(compositor.planes[2].is_none());
    // Two intersecting quads produce more triangles than two unsplit quads.
    assert!(compositor.vertices.len() > 12);
    assert!(compositor.nodes.iter().all(|node| {
        node.polygon.layer < 2 && compositor.planes[node.polygon.layer].is_some()
    }));
    assert!(compositor.batches.iter().any(|batch| batch.layer == 0));
    assert!(compositor.batches.iter().any(|batch| batch.layer == 1));
    assert!(compositor.batches.iter().all(|batch| batch.layer < 2));

    scene.layers[1].model = degenerate;
    compositor.prepare(&scene).unwrap();
    assert!(compositor.planes[1].is_none() && compositor.planes[2].is_none());
    assert_eq!(compositor.nodes.len(), 1);
    assert_eq!(compositor.vertices.len(), 6);
    assert_eq!(compositor.batches.len(), 1);
    assert_eq!(compositor.batches[0].layer, 0);
}
