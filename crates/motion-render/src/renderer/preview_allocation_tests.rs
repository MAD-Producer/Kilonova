//! Real preview-frame allocation measurements, including native backend calls.
//! Readback and independent reference rendering deliberately happen afterwards.
use super::*;
use crate::test_allocations;
use motion_core::{
    compositing::{MatteMode, TrackMatte},
    vector::{ShapeKind, VectorContent},
    Content, Layer,
};

fn project(count: u64, kind: &str) -> Project {
    let package = (kind == "effect").then(|| motion_effects::builtin::package().unwrap());
    let mut project = Project::new(32, 24, 30, 120).unwrap();
    project.background = [0.; 4];
    for id in 1..=count {
        let mut layer = Layer::solid(
            id,
            "Allocation measurement",
            [16., 12.],
            [16., 12., 0.],
            [0.2, 0.4, 0.8, 0.7],
        );
        if kind == "vector" {
            layer.content = Content::Vector {
                vector: VectorContent::shape(ShapeKind::Rectangle),
            };
        } else if kind == "matte" && (id - 1) % 8 != 0 {
            layer.track_matte = Some(TrackMatte {
                source: id - 1,
                hide_source: false,
                mode: if id % 2 == 0 {
                    MatteMode::Alpha
                } else {
                    MatteMode::Luma
                },
            });
        } else if let Some(package) = &package {
            let definition = package
                .manifest
                .effects
                .iter()
                .find(|d| d.id == "displacement_map")
                .unwrap();
            let mut effect = motion_core::EffectInstance::new(
                id,
                &package.manifest.id,
                &package.manifest.version,
                &package.hash,
                definition,
                [16., 12.],
            );
            if id > 1 {
                effect.image_input = Some(motion_core::EffectImageInput::Layer {
                    layer: id - 1,
                    stage: motion_core::EffectImageStage::Effects,
                });
            }
            layer.effects.push(effect);
        }
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    project
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn animated_preview_reports_sampling_preparation_encoding_and_submission_allocations() {
    for kind in ["solid", "vector", "matte", "effect"] {
        for count in [1, 8, 32] {
            let mut project = project(count, kind);
            let mut scene = Scene::new(&project).unwrap();
            let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
            let target = renderer.capture_target(32, 24).unwrap();
            let mut first_pixels = None;
            for frame in 0..20 {
                // Every sampled transform changes; this cannot pass by reusing
                // one static scene or only executing the resident prepare path.
                for layer in &mut project.layers {
                    layer.transform.position.value[0] = 10. + (frame % 8) as f32;
                    if kind == "effect" {
                        layer.effects[0]
                            .params
                            .get_mut("amount")
                            .unwrap()
                            .track
                            .value[0] = frame as f32 / 16.;
                    }
                }
                let sample = test_allocations::count(|| {
                    scene
                        .sample(
                            &project,
                            frame as f64,
                            None,
                            &motion_core::ExpressionEvaluator,
                        )
                        .unwrap();
                });
                let prepare = test_allocations::count(|| {
                    assert!(renderer
                        .prepare_scene_assets(
                            &scene,
                            crate::image_resources::Resolution::Preview(32),
                            false
                        )
                        .unwrap());
                });
                let mut encoder = None;
                let create = test_allocations::count(|| {
                    encoder = Some(
                        renderer
                            .allocation_command_encoder(&Default::default())
                            .unwrap(),
                    );
                });
                let mut encoder = encoder.unwrap();
                let previous_plan_builds = renderer.effect_plan_builds();
                let mut stats = None;
                let encode = test_allocations::count(|| {
                    stats = Some(
                        renderer
                            .encode_preview(&scene, &target.view, 32, 24, &mut encoder, None)
                            .unwrap(),
                    );
                });
                assert!(stats.unwrap().draw_calls > 0);
                if kind == "effect" {
                    assert!(
                        renderer.effect_plan_builds() > previous_plan_builds,
                        "changing sampled effect parameters must rebuild the dependency plan"
                    );
                }
                let submit = test_allocations::count(|| {
                    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
                });
                let poll = test_allocations::count(|| {
                    renderer
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .unwrap();
                    renderer.check_health().unwrap();
                });
                if frame >= 4 {
                    println!(
                        "{}",
                        serde_json::json!({"kind":kind,"layers":count,"frame":frame,
                        "sample":sample,"prepare":prepare,"create":create,"encode":encode,
                        "submit":submit,"poll":poll,"total":sample+prepare+create+encode+submit+poll})
                    );
                }
                if frame == 4 {
                    first_pixels = Some(renderer.read_target(&target).unwrap());
                }
            }
            let actual = renderer.read_target(&target).unwrap();
            assert_ne!(
                actual,
                first_pixels.unwrap(),
                "the sampled transform must change actual output"
            );
            let mut fresh = pollster::block_on(Renderer::headless()).unwrap();
            let reference = fresh.capture_target(32, 24).unwrap();
            assert_eq!(
                actual,
                fresh.capture(&scene, &reference).unwrap().0,
                "{kind} layers={count}"
            );
            renderer.check_health().unwrap();
        }
    }
}
