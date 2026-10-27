#[path = "../../motion-effects/tests/support/scene_packages.rs"]
mod scene_fixtures;
use motion_core::{
    compositing::*, masks::LayerMask, vector::*, Asset, Content, EffectInstance,
    ExpressionEvaluator, Layer, Project, Track,
};
use motion_render::{LayerRaster, Renderer, ResourceBudgetScope, ResourceRejection, Scene};

fn project() -> Project {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.background = [0.; 4];
    project.version = motion_model::CURRENT_PROJECT_VERSION;
    project
}
fn layer(id: u64, color: [f32; 4]) -> Layer {
    Layer::solid(id, "independent receiver", [32.; 2], [32., 32., 0.], color)
}
fn effect(name: &str, scene: bool) -> EffectInstance {
    let package = if scene {
        scene_fixtures::scene_package()
    } else {
        motion_effects::builtin::package()
    }
    .unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|definition| definition.id == name)
        .unwrap();
    EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [32.; 2],
    )
}
fn sample(project: &mut Project, frame: f64) -> Scene {
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, frame, None, &ExpressionEvaluator)
        .unwrap();
    scene
}
fn retained(
    renderer: &mut Renderer,
    scene: &Scene,
    width: u32,
    height: u32,
    preview: bool,
) -> Vec<LayerRaster> {
    scene
        .layers
        .iter()
        .map(|layer| {
            renderer
                .materialize_layer(scene, layer.id, width, height, preview)
                .unwrap()
                .unwrap()
                .0
        })
        .collect()
}
fn draw(
    renderer: &mut Renderer,
    scene: &Scene,
    rasters: &[LayerRaster],
    width: u32,
    height: u32,
) -> Vec<u8> {
    let target = renderer.capture_target(width, height).unwrap();
    // Intentionally reverse materialization order: phase stacking must use
    // authored order, not receiver evaluation or caller slice order.
    let refs: Vec<_> = rasters.iter().rev().collect();
    renderer
        .draw_layer_rasters(scene, &refs, &target.view, width, height)
        .unwrap();
    renderer.read_target(&target).unwrap()
}
fn equal(actual: &[u8], expected: &[u8], context: &str) {
    assert_eq!(actual.len(), expected.len());
    let (index, difference) = actual
        .iter()
        .zip(expected)
        .enumerate()
        .map(|(index, (a, b))| (index, a.abs_diff(*b)))
        .max_by_key(|(_, difference)| *difference)
        .unwrap();
    assert!(
        difference <= 1,
        "{context}: max difference={difference} at byte {index}, actual={} expected={}",
        actual[index],
        expected[index]
    );
}
fn reference(renderer: &mut Renderer, scene: &Scene, width: u32, height: u32) -> Vec<u8> {
    let target = renderer.capture_target(width, height).unwrap();
    renderer.capture(scene, &target).unwrap().0
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn owned_images_survive_other_receivers_and_normal_renders_with_authored_opacity_and_order() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    renderer.set_submission_tracking(true);
    let mut project = project();
    project.layers = vec![layer(1, [1., 0., 0., 1.]), layer(2, [0., 0., 1., 1.])];
    project.layers[1].transform.position.value[0] += 8.;
    project.layers[1].transform.opacity.value = 0.5;
    let scene = sample(&mut project, 5.);
    let expected = reference(&mut renderer, &scene, 64, 64);
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    assert_eq!(renderer.exposure_texture_bytes(), 2 * 4);
    assert!(rasters
        .iter()
        .all(|raster| raster.size() == [1; 2] && raster.frame() == 5.));
    assert_eq!(rasters[1].layer().opacity, 0.5);
    for raster in &rasters {
        let ledger = raster.texture_resource_ledger();
        assert_eq!(ledger.resources().count(), 1);
        assert_eq!(ledger.total_known_bytes(), raster.texture_bytes());
    }
    project.layers[0].content = Content::Solid {
        color: [0., 1., 0., 1.],
    };
    project.layers[1].transform.position.value[0] -= 20.;
    let other = sample(&mut project, 20.);
    let other_rasters = retained(&mut renderer, &other, 64, 64, true);
    let _ = reference(&mut renderer, &other, 64, 64);
    equal(
        &draw(&mut renderer, &scene, &rasters, 64, 64),
        &expected,
        "retained after overwrite",
    );
    drop(other_rasters);
    drop(rasters);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    renderer.check_health().unwrap();
    // Colors and alpha that cannot be represented exactly by an intermediate
    // RGBA8 image must still use their original floating-point draw uniform.
    project.layers[0].content = Content::Solid {
        color: [0.31, 0.63, 0.87, 0.27],
    };
    project.layers[0].transform.opacity.value = 0.41;
    project.layers[1].content = Content::Solid {
        color: [0.72, 0.23, 0.46, 0.61],
    };
    let scene = sample(&mut project, 5.);
    let expected = reference(&mut renderer, &scene, 64, 64);
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    assert_eq!(draw(&mut renderer, &scene, &rasters, 64, 64), expected);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn effect_padding_masks_and_effect_stage_image_inputs_match_the_original_renderer() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    let mut project = project();
    let mut receiver = layer(1, [1., 0., 0., 1.]);
    receiver.transform.rotation.value[2] = 13.;
    receiver.masks.push(LayerMask::new(
        1,
        VectorPath {
            id: 1,
            closed: true,
            nodes: [[4., 3.], [27., 3.], [27., 28.], [4., 28.]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    ));
    let mut shadow = effect("drop_shadow", false);
    // Independently exercise asymmetric effect bounds, not centered blur only.
    shadow.params.get_mut("offset").unwrap().track.value = [7., 3., 0., 0.];
    receiver.effects.push(shadow);
    let mut displacement = effect("displacement_map", false);
    displacement.id = 2;
    displacement.image_input = Some(motion_core::EffectImageInput::Layer {
        layer: 2,
        stage: motion_core::EffectImageStage::Effects,
    });
    receiver.effects.push(displacement);
    let mut source = layer(2, [0.4, 0.7, 0.8, 1.]);
    source.visible = false;
    source.effects.push(effect("brightness_contrast", false));
    project.layers = vec![receiver, source];
    let scene = sample(&mut project, 11.25);
    let expected = reference(&mut renderer, &scene, 64, 64);
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    assert!(rasters[0].layer().size[0] > 32. || rasters[0].layer().size[1] > 32.);
    assert!(rasters[0].layer().masks.is_empty());
    equal(
        &draw(&mut renderer, &scene, &rasters, 64, 64),
        &expected,
        "expanded mask/image input",
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn retained_matte_modes_chains_and_letterboxing_survive_cache_reuse() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    for mode in [
        MatteMode::Alpha,
        MatteMode::AlphaInverted,
        MatteMode::Luma,
        MatteMode::LumaInverted,
    ] {
        let mut project = project();
        let mut first = layer(1, [0.5, 0.5, 0.5, 0.5]);
        first.visible = false;
        first.size = [20.; 2];
        let mut second = layer(2, [1., 1., 1., 0.5]);
        second.visible = false;
        second.track_matte = Some(TrackMatte {
            source: 1,
            mode: MatteMode::AlphaInverted,
            hide_source: true,
        });
        let mut receiver = layer(3, [0.9, 0.2, 0.1, 0.7]);
        receiver.track_matte = Some(TrackMatte {
            source: 2,
            mode,
            hide_source: true,
        });
        project.layers = vec![first, second, receiver];
        let scene = sample(&mut project, 10.);
        let expected = reference(&mut renderer, &scene, 96, 64);
        let rasters = retained(&mut renderer, &scene, 96, 64, false);
        assert_eq!(rasters[2].texture_bytes(), 4 + 64 * 64);
        assert_eq!(rasters[2].texture_resource_ledger().resources().count(), 2);
        let mut empty = Scene::new(&project).unwrap();
        empty.layers.clear();
        let _ = reference(&mut renderer, &empty, 16, 16);
        equal(
            &draw(&mut renderer, &scene, &rasters, 96, 64),
            &expected,
            &format!("{mode:?}"),
        );
    }
    for mode in [
        MatteMode::Alpha,
        MatteMode::AlphaInverted,
        MatteMode::Luma,
        MatteMode::LumaInverted,
    ] {
        let mut project = project();
        let mut matte = layer(1, [1.; 4]);
        matte.visible = false;
        matte.timeline = Some(motion_core::LayerTimeline {
            in_frame: 0,
            out_frame: 1,
            offset_frame: 0,
        });
        let mut receiver = layer(2, [1., 0., 0., 1.]);
        receiver.track_matte = Some(TrackMatte {
            source: 1,
            mode,
            hide_source: true,
        });
        project.layers = vec![matte, receiver];
        let scene = sample(&mut project, 2.);
        let expected = reference(&mut renderer, &scene, 64, 64);
        let rasters = retained(&mut renderer, &scene, 64, 64, false);
        assert_eq!(rasters.len(), 1);
        let inverted = matches!(mode, MatteMode::AlphaInverted | MatteMode::LumaInverted);
        assert_eq!(
            expected[(32 * 64 + 32) * 4 + 3],
            if inverted { 255 } else { 0 }
        );
        equal(
            &draw(&mut renderer, &scene, &rasters, 64, 64),
            &expected,
            "absent matte keeps zero coverage and inversion",
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn blend_modes_and_scene_generator_keep_their_compositing_contract() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
        let mut project = project();
        project.background = [0.1, 0.2, 0.3, 0.5];
        project.layers = vec![layer(1, [0.2, 0.4, 0.6, 1.]), layer(2, [0.8, 0.7, 0.3, 1.])];
        project.layers[1].transform.opacity.value = 0.5;
        project.layers[1].blend = LayerBlend {
            mode,
            space: BlendSpace::Srgb,
        };
        let scene = sample(&mut project, 10.);
        let expected = reference(&mut renderer, &scene, 64, 64);
        let rasters = retained(&mut renderer, &scene, 64, 64, false);
        equal(
            &draw(&mut renderer, &scene, &rasters, 64, 64),
            &expected,
            &format!("blend {mode:?}"),
        );
    }
    let mut project = project();
    project.layers.push(layer(1, [0.; 4]));
    project.layers[0].effects.push(effect("starfield", true));
    project.layers[0].effects[0]
        .params
        .get_mut("extent")
        .unwrap()
        .track
        .value = [40., 40., 20., 0.];
    let scene = sample(&mut project, 30.);
    let expected = reference(&mut renderer, &scene, 64, 64);
    assert!(expected.chunks_exact(4).any(|pixel| pixel[3] > 0));
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    assert!(rasters[0].layer().screen_overlay);
    equal(
        &draw(&mut renderer, &scene, &rasters, 64, 64),
        &expected,
        "screen generator",
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn image_and_continuous_vector_keep_source_pixels_and_local_raster_size() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    let mut project = project();
    project.layers.push(layer(1, [1.; 4]));
    project.layers[0].content = Content::Image { asset: 1 };
    project.assets.push(Asset {
        id: 1,
        path: "assets/local.png".into(),
        width: 32,
        height: 32,
    });
    let pixels: Vec<u8> = (0..32 * 32)
        .flat_map(|i| [((i % 32) * 8) as u8, ((i / 32) * 8) as u8, 200, 255])
        .collect();
    renderer.upload_image(1, 32, 32, &pixels).unwrap();
    let scene = sample(&mut project, 0.);
    let expected = reference(&mut renderer, &scene, 64, 64);
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    equal(
        &draw(&mut renderer, &scene, &rasters, 64, 64),
        &expected,
        "image",
    );
    project.assets.clear();
    project.layers[0].content = Content::Vector {
        vector: VectorContent::shape(ShapeKind::Ellipse),
    };
    project.layers[0].sampling.continuous_rasterization = true;
    project.layers[0].transform.scale.value = [150.; 3];
    let scene = sample(&mut project, 0.);
    let expected = reference(&mut renderer, &scene, 64, 64);
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    equal(
        &draw(&mut renderer, &scene, &rasters, 64, 64),
        &expected,
        "continuous vector",
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_temporal_budget_drop_and_refusals_preserve_existing_images() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    let mut project = project();
    project.layers.push(layer(1, [1., 0., 0., 1.]));
    project.layers[0]
        .effects
        .push(effect("brightness_contrast", false));
    let scene = sample(&mut project, 0.);
    let expected = reference(&mut renderer, &scene, 64, 64);
    let raster = renderer
        .materialize_layer(&scene, 1, 64, 64, false)
        .unwrap()
        .unwrap()
        .0;
    let mut policy = renderer.memory_policy();
    let original = policy;
    policy.scratch_budget_bytes = 64 * 1024 * 1024;
    renderer.set_memory_policy(policy).unwrap();
    let large = renderer.exposure_target(2048, 2048, &[1.]).unwrap();
    let rest = renderer.exposure_target(1365, 1024, &[1.]).unwrap();
    assert_eq!(
        renderer.exposure_texture_bytes(),
        policy.scratch_budget_bytes
    );
    let error = renderer.exposure_target(1, 1, &[1.]).err().unwrap();
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::Budget {
            scope: ResourceBudgetScope::ExposureTargets,
            ..
        })
    ));
    assert_eq!(
        renderer.exposure_texture_bytes(),
        policy.scratch_budget_bytes
    );
    drop(rest);
    drop(large);
    assert_eq!(renderer.exposure_texture_bytes(), 32 * 32 * 4);
    renderer.set_memory_policy(original).unwrap();
    let output = renderer.capture_target(64, 64).unwrap();
    assert!(renderer
        .draw_layer_rasters(&scene, &[&raster, &raster], &output.view, 64, 64)
        .unwrap_err()
        .to_string()
        .contains("duplicate"));
    let mut other = pollster::block_on(Renderer::headless()).unwrap();
    other.set_effect_registry(scene_fixtures::registry().unwrap());
    let other_output = other.capture_target(64, 64).unwrap();
    assert!(other
        .draw_layer_rasters(&scene, &[&raster], &other_output.view, 64, 64)
        .unwrap_err()
        .to_string()
        .contains("another renderer"));
    assert!(renderer
        .materialize_layer(&scene, 999, 64, 64, false)
        .unwrap()
        .is_none());
    assert!(renderer.materialize_layer(&scene, 1, 0, 64, false).is_err());
    assert!(renderer
        .draw_layer_rasters(&scene, &[&raster], &output.view, 63, 64)
        .unwrap_err()
        .to_string()
        .contains("extent"));
    renderer
        .draw_layer_rasters(&scene, &[&raster], &output.view, 64, 64)
        .unwrap();
    equal(
        &renderer.read_target(&output).unwrap(),
        &expected,
        "recovery",
    );
    policy.scratch_budget_bytes = 128 * 1024 * 1024;
    renderer.set_memory_policy(policy).unwrap();
    let first = renderer.exposure_target(2048, 2048, &[1.]).unwrap();
    let second = renderer.exposure_target(2048, 2048, &[1.]).unwrap();
    policy.scratch_budget_bytes = 64 * 1024 * 1024;
    renderer.set_memory_policy(policy).unwrap();
    let error = renderer
        .draw_layer_rasters(&scene, &[&raster], &output.view, 64, 64)
        .unwrap_err();
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::Budget {
            scope: ResourceBudgetScope::ExposureTargets,
            ..
        })
    ));
    drop(second);
    renderer
        .draw_layer_rasters(&scene, &[&raster], &output.view, 64, 64)
        .unwrap();
    equal(
        &renderer.read_target(&output).unwrap(),
        &expected,
        "recovery after live policy reduction",
    );
    drop(first);
    renderer.set_memory_policy(original).unwrap();
    drop(raster);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    project.layers[0].content = Content::Adjustment;
    let scene = sample(&mut project, 0.);
    assert!(renderer
        .materialize_layer(&scene, 1, 64, 64, false)
        .err()
        .unwrap()
        .to_string()
        .contains("adjustment"));
    assert!(renderer
        .draw_layer_rasters(&scene, &[], &output.view, 64, 64)
        .unwrap_err()
        .to_string()
        .contains("adjustment"));
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn receiver_clock_materializes_real_effects_before_phase_accumulation_and_keeps_other_layers_sharp()
{
    use motion_core::{
        render_sampling::{ExposureClock, Shutter},
        Ease,
    };
    use motion_render::exposure_scene::{ExposureDocument, ExposureSceneStep, ExposureScenes};
    use std::{sync::Arc, time::Instant};
    let mut project = project();
    let mut moving = layer(1, [1., 0., 0., 1.]);
    moving.size = [8.; 2];
    for (frame, x) in [(0, 14.), (1, 50.)] {
        moving
            .transform
            .position
            .upsert(frame, [x, 16., 0.], Ease::Linear)
            .unwrap();
    }
    let shutter = Shutter {
        frames: 1.,
        samples: 8,
    };
    moving.sampling.motion_blur = Some(shutter);
    let mut gaussian = effect("gaussian_blur", false);
    gaussian.params.get_mut("p0001").unwrap().track.value[0] = 2.;
    moving.effects.push(gaussian);
    let mut sharp = layer(2, [0., 1., 0., 1.]);
    sharp.size = [8.; 2];
    for (frame, x) in [(0, 14.), (1, 50.)] {
        sharp
            .transform
            .position
            .upsert(frame, [x, 48., 0.], Ease::Linear)
            .unwrap();
    }
    project.layers = vec![moving, sharp];
    let center = sample(&mut project, 0.5);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    let original = reference(&mut renderer, &center, 64, 64);
    let clock = ExposureClock::new(0.5, &[(1, shutter)], false).unwrap();
    let weights: Vec<_> = clock.phases().iter().map(|phase| phase.weight).collect();
    let mut target = renderer.exposure_target(64, 64, &weights).unwrap();
    let document = ExposureDocument::new(Arc::new(project)).unwrap();
    let root = document.composition_id.clone();
    let mut task =
        ExposureScenes::new(document, &root, 0.5, false, None, ExpressionEvaluator).unwrap();
    let mut phase = Vec::new();
    let mut peak = 0;
    let mut draws = 0;
    let started = Instant::now();
    loop {
        match task.current().unwrap() {
            ExposureSceneStep::Receiver(sample) => {
                if let Some(scene) = sample.scene {
                    if let Some((raster, stats)) = renderer
                        .materialize_layer(scene, sample.object, 64, 64, false)
                        .unwrap()
                    {
                        assert_eq!(raster.frame(), sample.frame);
                        draws += stats.draw_calls;
                        phase.push(raster);
                    }
                }
                peak = peak.max(renderer.exposure_texture_bytes());
                task.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { index, .. } => {
                assert_eq!(target.phase_index(), index);
                let refs: Vec<_> = phase.iter().collect();
                draws += renderer
                    .accumulate_layer_exposure(&mut target, &center, &refs)
                    .unwrap()
                    .draw_calls;
                assert_eq!(target.phase_index(), index + 1);
                phase.clear();
                task.finish_phase().unwrap();
            }
            ExposureSceneStep::Complete => break,
        }
    }
    let actual = renderer.capture_exposure(&target).unwrap();
    let capture_us = started.elapsed().as_micros();
    // The green layer is animated too, but its receiver time stays at the
    // center: a whole-scene motion average would visibly widen this strip.
    equal(
        &actual[40 * 64 * 4..],
        &original[40 * 64 * 4..],
        "unblurred animated reference",
    );
    let red_support = |pixels: &[u8]| {
        (0..64)
            .filter(|x| pixels[(16 * 64 + x) * 4 + 3] > 1)
            .count()
    };
    assert!(red_support(&actual) > red_support(&original) * 2);
    let area = |pixels: &[u8]| {
        pixels[..32 * 64 * 4]
            .chunks_exact(4)
            .map(|p| u64::from(p[3]))
            .sum::<u64>()
    };
    assert!((area(&actual) as f64 / area(&original) as f64 - 1.).abs() < 0.03);
    assert_eq!(renderer.exposure_texture_bytes(), target.texture_bytes());
    if let Ok(path) = std::env::var("MOTION_SAMPLING_ARTIFACT_DIR") {
        std::fs::create_dir_all(&path).unwrap();
        for (name, pixels) in [
            ("local-effects-sharp.png", &original),
            ("local-effects-exposure.png", &actual),
        ] {
            image::save_buffer(
                std::path::Path::new(&path).join(name),
                pixels,
                64,
                64,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        std::fs::write(std::path::Path::new(&path).join("local-effects-cost.json"),
            serde_json::to_vec_pretty(&serde_json::json!({"adapter":format!("{:?}",renderer.adapter_info),
                "phases":target.phase_count(),"capture_us":capture_us,"peak_temporal_texture_bytes":peak,
                "effect_and_plane_draws":draws,"output":[64,64]})).unwrap()).unwrap();
    }
    drop(target);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn bgra_nested_and_intersecting_receiver_projections_match_existing_pixels() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ))
    .unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    let mut project = project();
    project.layers = vec![layer(1, [1., 0., 0., 1.]), layer(2, [0., 0., 1., 1.])];
    for (layer, angle) in project.layers.iter_mut().zip([50., -40.]) {
        layer.three_d = true;
        layer.transform.rotation.value[1] = angle;
    }
    let mut scene = sample(&mut project, 0.);
    // Distinct receiver camera matrices exercise the projected BSP path.
    scene.layers[1].view_projection *= glam::Mat4::from_translation(glam::Vec3::new(4., 0., 0.));
    let expected = reference(&mut renderer, &scene, 64, 64);
    assert!(expected.chunks_exact(4).any(|pixel| pixel[0] > 0));
    assert!(expected.chunks_exact(4).any(|pixel| pixel[2] > 0));
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    equal(
        &draw(&mut renderer, &scene, &rasters, 64, 64),
        &expected,
        "BGRA intersecting projected planes",
    );
    let mut child = layer(3, [0.2, 0.8, 0.4, 1.]);
    child.effects.push(effect("gaussian_blur", false));
    project.compositions.push(motion_core::Composition {
        id: "comp-local".into(),
        name: "local child".into(),
        width: 64,
        height: 64,
        fps: 30,
        frames: 120,
        work_area: motion_core::WorkArea::full(120),
        guides: Vec::new(),
        background: [0.; 4],
        camera: motion_core::Camera::new(64, 64),
        layers: vec![child],
        expressions: vec![],
    });
    project.layers.truncate(1);
    project.layers[0].three_d = false;
    project.layers[0].transform.rotation.value = [0.; 3];
    project.layers[0].content = Content::Composition {
        clip: motion_core::CompositionClip::new("comp-local".into()),
    };
    let scene = sample(&mut project, 3.5);
    let expected = reference(&mut renderer, &scene, 64, 64);
    assert!(expected.chunks_exact(4).any(|pixel| pixel[3] > 0));
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    equal(
        &draw(&mut renderer, &scene, &rasters, 64, 64),
        &expected,
        "nested BGRA",
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unsubmitted_video_is_rejected_and_rasters_keep_completed_pixels_after_upload_reuse() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    let mut project = project();
    project.layers.push(layer(1, [1.; 4]));
    let mut scene = sample(&mut project, 0.);
    // Explicitly supplied decoded pixels, not a media-file decoding test.
    scene.layers[0].video = Some(motion_core::VideoSample {
        asset: 7,
        source_time_us: 0,
    });
    let frame = motion_render::Yuv420Frame {
        width: 32,
        height: 32,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: motion_render::ChromaLayout::Uv,
        y: vec![235; 32 * 32],
        uv: vec![128; 32 * 16],
    };
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let token = renderer
        .encode_video_yuv(1, 7, 0, &frame, &mut encoder)
        .unwrap()
        .unwrap();
    let error = renderer
        .materialize_layer(&scene, 1, 64, 64, false)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("object=1") && error.contains("source=7") && error.contains("unsubmitted"),
        "{error}"
    );
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    drop(encoder);
    drop(token);
    assert!(renderer
        .materialize_layer(&scene, 1, 64, 64, false)
        .err()
        .unwrap()
        .to_string()
        .contains("canceled"));
    renderer.upload_video_yuv(1, 7, 0, &frame).unwrap();
    let expected = reference(&mut renderer, &scene, 64, 64);
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    renderer
        .upload_video_frame(1, 7, 1000, 32, 32, &[255, 0, 0, 255].repeat(32 * 32))
        .unwrap();
    equal(
        &draw(&mut renderer, &scene, &rasters, 64, 64),
        &expected,
        "video image ownership",
    );
    let mut target = renderer.exposure_target(64, 64, &[1.]).unwrap();
    assert!(renderer
        .accumulate_layer_exposure(&mut target, &scene, &[&rasters[0], &rasters[0]])
        .is_err());
    assert_eq!(target.phase_index(), 0);
    renderer
        .accumulate_layer_exposure(&mut target, &scene, &[&rasters[0]])
        .unwrap();
    equal(
        &renderer.capture_exposure(&target).unwrap(),
        &expected,
        "complete retained video exposure",
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn failed_effect_is_not_bypassed_in_preview_or_formal_materialization_and_valid_context_recovers() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    let mut project = project();
    project.layers.push(layer(1, [1., 0., 0., 1.]));
    project.layers[0]
        .effects
        .push(effect("brightness_contrast", false));
    let scene = sample(&mut project, 5.);
    let expected = reference(&mut renderer, &scene, 64, 64);
    let mut broken = scene.clone();
    broken.effects[0].effect = "controlled_missing_effect".into();
    for preview in [true, false] {
        let error = renderer
            .materialize_layer(&broken, 1, 64, 64, preview)
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("object=1")
                && error.contains("local_materialize")
                && error.contains("controlled_missing_effect"),
            "{error}"
        );
        assert_eq!(renderer.exposure_texture_bytes(), 0);
    }
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    equal(
        &draw(&mut renderer, &scene, &rasters, 64, 64),
        &expected,
        "effect refusal recovery",
    );
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn retained_phase_submission_records_both_local_input_and_exposure_output_capacity() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    renderer.set_submission_tracking(true);
    let mut project = project();
    project.layers.push(layer(1, [0.3, 0.7, 0.2, 1.]));
    let scene = sample(&mut project, 0.);
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    let mut target = renderer.exposure_target(64, 64, &[1.]).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let mut current = renderer.texture_resource_ledger();
    current.merge(target.texture_resource_ledger());
    current.merge(rasters[0].texture_resource_ledger());
    let before = renderer.submission_memory_info(&current);
    renderer
        .accumulate_layer_exposure(&mut target, &scene, &[&rasters[0]])
        .unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let after = renderer.submission_memory_info(&current);
    assert_eq!(after.submitted, before.submitted + 2);
    assert_eq!(after.completed, after.submitted);
    assert_eq!(after.pending_submissions, 0);
    // Only the plane submission uses the local input, while both need the
    // exposure output. Counting the latter on the sum alone misses this peak.
    assert!(after.peak_known_pending_bytes >= current.total_known_bytes());
    assert_eq!(after.known_retired_bytes, 0);
    assert_eq!(after.skipped_snapshots, 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn blend_capacity_is_checked_before_allocation_and_original_rasters_recover() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(scene_fixtures::registry().unwrap());
    let mut project = project();
    project.layers = vec![layer(1, [0.2, 0.6, 0.8, 1.]), layer(2, [0.8, 0.3, 0.5, 1.])];
    project.layers[1].blend = LayerBlend {
        mode: BlendMode::Multiply,
        space: BlendSpace::Srgb,
    };
    let scene = sample(&mut project, 0.);
    let rasters = retained(&mut renderer, &scene, 64, 64, false);
    let original = renderer.memory_policy();
    let mut reduced = original;
    reduced.source_texture_budget_bytes = 64;
    renderer.set_memory_policy(reduced).unwrap();
    let before = renderer.texture_resource_ledger().total_known_bytes();
    let target = renderer.capture_target(64, 64).unwrap();
    let refs: Vec<_> = rasters.iter().collect();
    let error = renderer
        .draw_layer_rasters(&scene, &refs, &target.view, 64, 64)
        .unwrap_err();
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::Budget {
            scope: ResourceBudgetScope::LayerCompositing,
            required_bytes: 49152,
            budget_bytes: 64,
        })
    ));
    assert_eq!(
        renderer.texture_resource_ledger().total_known_bytes(),
        before
    );
    assert_eq!(renderer.exposure_texture_bytes(), 8);
    renderer.set_memory_policy(original).unwrap();
    let expected = reference(&mut renderer, &scene, 64, 64);
    equal(
        &draw(&mut renderer, &scene, &rasters, 64, 64),
        &expected,
        "composite capacity recovery",
    );
}
