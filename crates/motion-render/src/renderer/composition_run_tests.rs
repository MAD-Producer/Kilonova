use super::*;
use motion_core::{
    vector::{ShapeKind, VectorContent},
    Content, Layer,
};
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, WorkingSpace,
};

fn project() -> Project {
    let mut project = Project::new(32, 24, 30, 120).unwrap();
    project.background = [0.1, 0.2, 0.3, 0.4];
    for id in 1..=8 {
        let mut layer = Layer::solid(
            id,
            "Composition run",
            [16., 12.],
            [10. + id as f32, 8. + id as f32 / 2., 0.],
            [0.1 * id as f32, 0.4, 0.8, 0.6],
        );
        layer.content = Content::Vector {
            vector: VectorContent::shape(ShapeKind::Rectangle),
        };
        project.layers.push(layer);
    }
    project.validate().unwrap();
    project
}

fn sample(project: &Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unmodified_vector_batches_share_one_pass_and_match_legacy_complete_pixels_in_all_precisions() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            crate::AllocationBudget::default(),
        ))
        .unwrap();
        let target = renderer
            .image_capture_target(
                47,
                31,
                ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Premultiplied,
                    },
                    precision,
                ),
            )
            .unwrap();
        let mut project = project();
        let mut previous = None;
        for frame in 0..3 {
            project.layers[0].transform.position.value[0] += 3.;
            let scene = sample(&project);
            renderer.legacy_composition_passes = true;
            let expected_stats = renderer.draw(&scene, target.view(), 47, 31).unwrap();
            assert_eq!(renderer.composition_runs, 0);
            let expected = renderer.read_image_target(&target).unwrap().pixels;
            let ledger = renderer.texture_resource_ledger();
            renderer.legacy_composition_passes = false;
            let actual_stats = renderer.draw(&scene, target.view(), 47, 31).unwrap();
            let actual = renderer.read_image_target(&target).unwrap().pixels;
            assert_eq!(actual, expected, "precision={precision:?} frame={frame}");
            assert_eq!(actual_stats.draw_calls, expected_stats.draw_calls);
            assert_eq!(renderer.texture_resource_ledger(), ledger);
            println!(
                "precision={precision:?} frame={frame} batches={} passes={} output_bytes={}",
                renderer.effect_gpu.builder.frame.batches.len(),
                renderer.composition_runs,
                actual.len()
            );
            assert_eq!(
                renderer.composition_runs, 1,
                "ordinary batches must actually use the shared pass"
            );
            if let Some(previous) = previous {
                assert_ne!(actual, previous);
            }
            previous = Some(actual);
            renderer.check_health().unwrap();
        }
    }
}

fn effect(layer: &mut Layer) {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "fill")
        .unwrap();
    layer.effects.push(motion_core::EffectInstance::new(
        layer.id,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [32., 24.],
    ));
}

fn mask(layer: &mut Layer) {
    use motion_core::{
        masks::LayerMask,
        vector::{PathNode, VectorPath},
        Track,
    };
    layer.masks.push(LayerMask::new(
        1,
        VectorPath {
            id: 1,
            closed: true,
            tangents: Default::default(),
            nodes: [[2., 2.], [14., 2.], [14., 10.], [2., 10.]]
                .into_iter()
                .enumerate()
                .map(|(index, point)| PathNode {
                    id: index as u64 + 1,
                    geometry: Track::constant([point[0], point[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    ));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn composition_runs_preserve_boundaries_and_pixels_through_precision_and_scene_changes() {
    use motion_core::compositing::{BlendMode, BlendSpace, MatteMode, TrackMatte};
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            crate::AllocationBudget::default(),
        ))
        .unwrap();
        let target = renderer
            .image_capture_target(
                47,
                31,
                ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Premultiplied,
                    },
                    precision,
                ),
            )
            .unwrap();
        for case in [
            "masked",
            "matte",
            "blend",
            "effect",
            "adjustment",
            "three_d",
            "hidden",
            "plain",
        ] {
            let mut project = project();
            match case {
                "masked" => mask(&mut project.layers[1]),
                "matte" => {
                    project.layers[0].track_matte = Some(TrackMatte {
                        source: 8,
                        mode: MatteMode::Alpha,
                        hide_source: false,
                    });
                    project.layers[1].track_matte = Some(TrackMatte {
                        source: 8,
                        mode: MatteMode::LumaInverted,
                        hide_source: false,
                    });
                }
                "blend" => {
                    project.layers[3].blend.mode = BlendMode::Multiply;
                    project.layers[5].blend.mode = BlendMode::Screen;
                    project.layers[5].blend.space = BlendSpace::Srgb;
                }
                "effect" => effect(&mut project.layers[3]),
                "adjustment" => {
                    project.layers[3].content = Content::Adjustment;
                    effect(&mut project.layers[3]);
                }
                "three_d" => {
                    for (index, layer) in project.layers.iter_mut().enumerate() {
                        layer.three_d = true;
                        layer.transform.rotation.value =
                            [index as f32 * 8., 20., index as f32 * 3.];
                        layer.transform.position.value[2] = index as f32;
                    }
                }
                "hidden" => project.layers[3].visible = false,
                _ => {}
            }
            project.rebuild_plugin_dependencies();
            project.validate().unwrap();
            let scene = sample(&project);
            for preview in [false, true] {
                renderer.legacy_composition_passes = true;
                let old = renderer
                    .draw_internal(
                        &scene,
                        target.view(),
                        47,
                        31,
                        preview,
                        crate::gpu_commands::CommandStage::Composition,
                    )
                    .unwrap();
                assert_eq!(renderer.composition_runs, 0);
                let expected = renderer.read_image_target(&target).unwrap().pixels;
                let ledger = renderer.texture_resource_ledger();
                let bytes = renderer.texture_bytes();
                renderer.legacy_composition_passes = false;
                let new = renderer
                    .draw_internal(
                        &scene,
                        target.view(),
                        47,
                        31,
                        preview,
                        crate::gpu_commands::CommandStage::Composition,
                    )
                    .unwrap();
                let actual = renderer.read_image_target(&target).unwrap().pixels;
                assert_eq!(actual, expected, "{precision:?} {case} preview={preview}");
                assert_eq!(new.draw_calls, old.draw_calls);
                assert_eq!(new.texture_bytes, old.texture_bytes);
                assert_eq!(renderer.texture_bytes(), bytes);
                assert_eq!(renderer.texture_resource_ledger(), ledger);
                assert!(
                    renderer.composition_runs > 0,
                    "{case} must exercise at least one run"
                );
                println!("boundary precision={precision:?} case={case} preview={preview} runs={} bytes={}", renderer.composition_runs, actual.len());
                renderer.check_health().unwrap();
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn composition_run_returns_missing_input_context_and_recovers_without_submitting_partial_work() {
    let mut project = project();
    mask(&mut project.layers[0]);
    let scene = sample(&project);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(32, 24).unwrap();
    let expected = renderer.capture(&scene, &target).unwrap().0;
    let ledger = renderer.texture_resource_ledger();
    for (key, text) in [
        (TextureKey::Vector(1), "draw image is not uploaded"),
        (TextureKey::Mask(1), "mask image is not prepared"),
    ] {
        let image = renderer.images.remove(&key).unwrap();
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        let frame = &renderer.effect_gpu.builder.frame;
        let result = renderer.encode_composition_run(
            &mut encoder,
            composition_runs::CompositionRun {
                scene: &scene,
                frame,
                batches: &frame.batches,
                view: &target.view,
                viewport: [0., 0., 32., 24.],
                timestamps: None,
                compositing: false,
            },
        );
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("layer=1") && error.contains("frame=0") && error.contains(text),
            "{error}"
        );
        drop(encoder); // Actual unsubmitted command abandonment, not a retry.
        renderer.images.insert(key, image);
        assert_eq!(renderer.texture_resource_ledger(), ledger);
        assert_eq!(renderer.read_target(&target).unwrap(), expected);
        assert_eq!(renderer.capture(&scene, &target).unwrap().0, expected);
    }
    renderer.check_health().unwrap();
}
