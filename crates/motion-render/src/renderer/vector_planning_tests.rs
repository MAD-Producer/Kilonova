use super::*;
use crate::{test_allocations, vector_mesh::VectorMesh};
use motion_core::{Layer, Project};
use std::{collections::BTreeSet, sync::Arc};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn warmed_vector_budget_planning_is_allocation_free_for_shared_and_distinct_sizes() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    for count in [1, 8, 96] {
        let mut project = Project::new(32, 32, 30, 30).unwrap();
        project.layers = (0..count)
            .map(|i| Layer::solid(i + 1, "Planning", [8.; 2], [0.; 3], [1.; 4]))
            .collect();
        let mut scene = Scene::new(&project).unwrap();
        scene
            .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        for grouped in [false, true] {
            let meshes: Vec<_> = (0..count as usize)
                .map(|i| VectorMesh {
                    layer: i,
                    width: 8 + (i % 4) as u32,
                    height: 8 + (i % 3) as u32,
                    fingerprint: i as u64,
                    vertices: Arc::new(vec![]),
                    commands: Arc::new(vec![]),
                    root_opacity: if grouped { 0.5 } else { 1. },
                    origin: [0.; 2],
                })
                .collect();
            let targets: BTreeSet<_> = meshes.iter().map(|m| (m.width, m.height)).collect();
            let sources = meshes
                .iter()
                .map(|m| u64::from(m.width) * u64::from(m.height) * 4)
                .sum::<u64>();
            let scratch = targets
                .iter()
                .map(|(w, h)| u64::from(*w) * u64::from(*h) * 4 * 5)
                .sum::<u64>();
            let expected = sources + scratch;
            assert_eq!(
                renderer
                    .layer_gpu
                    .pending_vector_bytes(&scene, &meshes, &renderer.images),
                expected
            );
            let allocations = test_allocations::count(|| {
                for _ in 0..120 {
                    assert_eq!(
                        renderer
                            .layer_gpu
                            .pending_vector_bytes(&scene, &meshes, &renderer.images),
                        expected
                    );
                }
            });
            assert_eq!(allocations, 0, "layers={count} grouped={grouped}");
            assert_eq!(
                renderer
                    .layer_gpu
                    .pending_vector_bytes(&scene, &[], &renderer.images),
                0,
                "a new frame must not retain prior target IDs"
            );
            eprintln!("vector budget planning layers={count} grouped={grouped} iterations=120 allocations={allocations} bytes={expected}");
        }
    }
}
