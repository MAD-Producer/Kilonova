use super::*;
use crate::{test_allocations, vector_mesh::VectorMesh};
use motion_core::{Layer, Project};
use std::{collections::BTreeSet, sync::Arc};

fn working_vector_draw(
    renderer: &mut Renderer,
    scene: &Scene,
    target: &CaptureTarget,
    submit: bool,
    prematurely_commit: bool,
    unwind: bool,
) {
    let guard = renderer.layer_gpu.authored_raster_guard();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode_local_colors(
            scene,
            &target.view,
            64,
            64,
            &mut encoder,
            None,
            false,
            true,
            None,
        )
        .unwrap();
    if unwind {
        std::panic::panic_any("original authored encoder unwind");
    }
    if submit {
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        guard.commit();
    } else if prematurely_commit {
        // Deliberately wrong producer used only to prove the recovery oracle.
        guard.commit();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn abandoned_authored_vector_encoders_invalidate_actual_rasters_on_drop_and_unwind() {
    use motion_core::{
        vector::{groups::VectorGroup, ShapeKind, VectorContent},
        Content, Track,
    };
    use std::panic::{catch_unwind, AssertUnwindSafe};
    for grouped in [false, true] {
        let mut project = Project::new(64, 64, 30, 30).unwrap();
        project.background = [0.; 4];
        let mut layer = Layer::solid(
            1,
            "encoder ownership control",
            [20.; 2],
            [32., 32., 0.],
            [1.; 4],
        );
        let mut vector = VectorContent::shape(ShapeKind::Rectangle);
        if grouped {
            vector = VectorGroup::wrap(vector, [20.; 2]);
        }
        layer.content = Content::Vector { vector };
        project.layers.push(layer);
        let mut scene = Scene::new(&project).unwrap();
        scene
            .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let target = renderer.capture_target(64, 64).unwrap();
        let center = |renderer: &Renderer| {
            renderer.read_target(&target).unwrap()[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4]
                .to_vec()
        };
        working_vector_draw(&mut renderer, &scene, &target, true, false, false);
        for (unwind, color) in [(false, [1., 0., 0., 1.]), (true, [0., 1., 0., 1.])] {
            let Content::Vector { vector } = &mut project.layers[0].content else {
                unreachable!()
            };
            if grouped {
                *vector = VectorGroup::wrap(
                    VectorContent {
                        fill: Some(Track::constant(color)),
                        ..VectorContent::shape(ShapeKind::Rectangle)
                    },
                    [20.; 2],
                );
            } else {
                vector.fill = Some(Track::constant(color));
            }
            scene
                .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
                .unwrap();
            let before = center(&renderer);
            let result = catch_unwind(AssertUnwindSafe(|| {
                working_vector_draw(&mut renderer, &scene, &target, false, false, unwind)
            }));
            if unwind {
                assert_eq!(
                    result.unwrap_err().downcast_ref::<&str>(),
                    Some(&"original authored encoder unwind")
                );
            } else {
                result.unwrap();
            }
            assert_eq!(
                center(&renderer),
                before,
                "unsubmitted commands cannot publish pixels"
            );
            working_vector_draw(&mut renderer, &scene, &target, true, false, false);
            assert_eq!(center(&renderer), color.map(|value| (value * 255.) as u8));
        }
        let Content::Vector { vector } = &mut project.layers[0].content else {
            unreachable!()
        };
        let blue = VectorContent {
            fill: Some(Track::constant([0., 0., 1., 1.])),
            ..VectorContent::shape(ShapeKind::Rectangle)
        };
        *vector = if grouped {
            VectorGroup::wrap(blue, [20.; 2])
        } else {
            blue
        };
        scene
            .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let old = center(&renderer);
        working_vector_draw(&mut renderer, &scene, &target, false, true, false);
        working_vector_draw(&mut renderer, &scene, &target, true, false, false);
        assert_eq!(
            center(&renderer),
            old,
            "wrong early commit must visibly retain stale pixels"
        );
        assert_ne!(center(&renderer), [0, 0, 255, 255]);
        drop(renderer.layer_gpu.authored_raster_guard());
        working_vector_draw(&mut renderer, &scene, &target, true, false, false);
        assert_eq!(center(&renderer), [0, 0, 255, 255]);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn dirty_vector_texture_preparation_reuses_storage_without_publishing_pixels() {
    use motion_core::{
        vector::{groups::VectorGroup, ShapeKind, VectorContent, VectorSource},
        Content, Track,
    };
    for grouped in [false, true] {
        for count in [1, 8, 32] {
            let mut project = Project::new(64, 64, 30, 30).unwrap();
            project.background = [0.; 4];
            for id in 1..=count {
                let mut layer =
                    Layer::solid(id, "Animated vector", [16.; 2], [32., 32., 0.], [1.; 4]);
                let mut vector = VectorContent::shape(ShapeKind::Rectangle);
                if grouped {
                    vector = VectorGroup::wrap(vector, [16.; 2]);
                    let VectorSource::Group { group } = &mut vector.source else {
                        unreachable!()
                    };
                    group.transform.opacity.value = 50.;
                }
                layer.content = Content::Vector { vector };
                project.layers.push(layer);
            }
            let mut scene = Scene::new(&project).unwrap();
            scene
                .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
                .unwrap();
            let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
            let target = renderer.capture_target(64, 64).unwrap();
            let before = renderer.capture(&scene, &target).unwrap().0;
            for layer in &mut project.layers {
                let Content::Vector { vector } = &mut layer.content else {
                    unreachable!()
                };
                if grouped {
                    let VectorSource::Group { group } = &mut vector.source else {
                        unreachable!()
                    };
                    group.transform.opacity.value = 75.;
                } else {
                    vector.fill = Some(Track::constant([0., 1., 0., 1.]));
                }
            }
            scene
                .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
                .unwrap();
            let meshes = renderer
                .effect_gpu
                .builder
                .build(&scene, &[0], 64, 64, true)
                .unwrap()
                .vectors
                .clone();
            assert_eq!(meshes.len(), count as usize);
            let ledger = renderer.texture_resource_ledger();
            let budget = renderer.allocation_budget().info().unwrap().admitted_bytes;
            let source_budget = renderer.effect_gpu.builder.source_texture_budget;
            let allocations = test_allocations::count(|| {
                for _ in 0..120 {
                    assert!(!renderer
                        .layer_gpu
                        .prepare_textures(
                            &renderer.device,
                            &renderer.image_layout,
                            &renderer.sampler,
                            &scene,
                            &meshes,
                            &mut renderer.images,
                            &mut renderer.texture_bytes,
                            0,
                            source_budget
                        )
                        .unwrap());
                }
            });
            assert_eq!(
                allocations, 0,
                "dirty texture preparation layers={count} grouped={grouped}"
            );
            assert_eq!(renderer.texture_resource_ledger(), ledger);
            assert_eq!(
                renderer.allocation_budget().info().unwrap().admitted_bytes,
                budget
            );
            assert_eq!(renderer.read_target(&target).unwrap(), before);
            let after = renderer.capture(&scene, &target).unwrap().0;
            // Many overlapping translucent layers saturate alpha; one layer
            // still proves this is an authored change, not a cache-only hit.
            if count == 1 {
                assert_ne!(after, before);
            }
            let mut fresh = pollster::block_on(Renderer::headless()).unwrap();
            let fresh_target = fresh.capture_target(64, 64).unwrap();
            assert_eq!(after, fresh.capture(&scene, &fresh_target).unwrap().0);
            eprintln!("dirty texture preparation layers={count} grouped={grouped} iterations=120 allocations={allocations}");
        }
    }
}

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
