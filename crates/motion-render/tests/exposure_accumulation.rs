use motion_core::{
    render_sampling::{ExposureClock, Shutter},
    Layer, Project,
};
use motion_render::{
    ExposureTarget, Renderer, ResourceBudgetScope, ResourceRejection, Scene, TextureCategory,
};
use std::time::Instant;

fn background(color: [f32; 4]) -> Scene {
    let mut project = Project::new(16, 16, 30, 60).unwrap();
    project.background = color;
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn pixel(renderer: &Renderer, target: &ExposureTarget) -> [u8; 4] {
    renderer.capture_exposure(target).unwrap()[4 * (8 * 16 + 8)..4 * (8 * 16 + 8) + 4]
        .try_into()
        .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn linear_weights_transparency_and_uniform_submission_order_reach_real_pixels() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut target = renderer.exposure_target(16, 16, &[0.5, 0.5]).unwrap();
    assert!(renderer
        .capture_exposure(&target)
        .unwrap_err()
        .to_string()
        .contains("incomplete"));
    assert_eq!(target.phase_index(), 0);
    renderer
        .accumulate_exposure(&mut target, Some(&background([0., 0., 0., 1.])), false)
        .unwrap();
    renderer
        .accumulate_exposure(&mut target, Some(&background([1.; 4])), false)
        .unwrap();
    assert!(target.complete());
    let mid = pixel(&renderer, &target);
    assert!(
        (187..=189).contains(&mid[0]) && mid[..3].iter().all(|c| *c == mid[0]),
        "{mid:?}"
    );
    assert_eq!(mid[3], 255);
    assert!(renderer
        .accumulate_exposure(&mut target, None, false)
        .unwrap_err()
        .to_string()
        .contains("already complete"));
    let wrong_extent = renderer.render_target(4, 4).unwrap();
    assert!(renderer
        .resolve_exposure(&target, &wrong_extent)
        .unwrap_err()
        .to_string()
        .contains("extent or format"));
    let output = renderer.render_target(16, 16).unwrap();
    renderer.resolve_exposure(&target, &output).unwrap();
    let mut target = renderer
        .exposure_target(16, 16, &[0.25, 0.5, 0.25])
        .unwrap();
    for color in [[1., 0., 0., 1.], [0., 1., 0., 1.], [0., 0., 1., 1.]] {
        renderer
            .accumulate_exposure(&mut target, Some(&background(color)), true)
            .unwrap();
    }
    let actual = pixel(&renderer, &target);
    assert!(
        actual
            .iter()
            .zip([137u8, 188, 137, 255])
            .all(|(a, b)| a.abs_diff(b) <= 1),
        "{actual:?}"
    );
    for transparent_first in [true, false] {
        let mut target = renderer.exposure_target(16, 16, &[0.5, 0.5]).unwrap();
        let red = background([1., 0., 0., 1.]);
        for phase in 0..2 {
            renderer
                .accumulate_exposure(
                    &mut target,
                    if (phase == 0) == transparent_first {
                        None
                    } else {
                        Some(&red)
                    },
                    false,
                )
                .unwrap();
        }
        let actual = pixel(&renderer, &target);
        assert_eq!(&actual[..3], &[255, 0, 0]);
        assert!(
            (127..=128).contains(&actual[3]),
            "transparent phase must retain half the shutter: {actual:?}"
        );
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn eighty_nonuniform_phases_preserve_weighted_colors_and_transparent_gaps() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let weights: Vec<_> = (1..=80).map(|index| f64::from(index) / 3240.).collect();
    let scenes = [background([1.,0.,0.,1.]),background([0.,1.,0.,1.]),background([0.,0.,1.,1.])];
    for reverse in [false,true] {
        let mut target = renderer.exposure_target(16,16,&weights).unwrap();
        let mut linear = [0.0f64;3];
        for (phase,weight) in weights.iter().enumerate() {
            let channel = if reverse { (79-phase)%3 } else { phase%3 };
            renderer.accumulate_exposure(&mut target,Some(&scenes[channel]),false).unwrap();
            linear[channel] += weight;
        }
        let actual = pixel(&renderer,&target);
        for (channel,value) in linear.into_iter().enumerate() {
            let expected = ((1.055*value.powf(1./2.4)-0.055)*255.).round() as u8;
            assert!(actual[channel].abs_diff(expected)<=1,"nonuniform phase color={actual:?} expected={expected} channel={channel} reverse={reverse}");
        }
        assert_eq!(actual[3],255);
    }
    // The reusable sample contains the previous color when a phase is absent.
    // Missing phases must contribute zero, including after the final real one.
    for reverse in [false,true] {
        let mut target = renderer.exposure_target(16,16,&[0.0125;80]).unwrap();
        for phase in 0..80 {
            let present = if reverse { phase>=48 } else { phase<32 };
            renderer.accumulate_exposure(&mut target,present.then_some(&scenes[0]),false).unwrap();
        }
        let actual = pixel(&renderer,&target);
        assert!(actual[0]>=254 && actual[1]==0 && actual[2]==0 && actual[3].abs_diff(102)<=1,
            "transparent gaps={actual:?} reverse={reverse}");
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn maximal_clock_preserves_constant_layers_and_drop_cancels_without_leaking_the_next_result() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_submission_tracking(true);
    let shutters: Vec<_> = (2..=16)
        .map(|samples| {
            (
                u64::from(samples),
                Shutter {
                    frames: 1.,
                    samples,
                },
            )
        })
        .collect();
    let clock = ExposureClock::new(5., &shutters, false).unwrap();
    let weights: Vec<_> = clock.phases().iter().map(|p| p.weight).collect();
    assert_eq!(weights.len(), 80);
    let white = background([1.; 4]);
    let mut target = renderer.exposure_target(16, 16, &weights).unwrap();
    let bytes = 16 * 16 * 12;
    assert_eq!(target.texture_bytes(), bytes);
    assert_eq!(renderer.exposure_texture_bytes(), bytes);
    let ledger = target.texture_resource_ledger();
    assert_eq!(ledger.total_known_bytes(), bytes);
    assert_eq!(ledger.resources().count(), 2);
    assert!(ledger.resources().all(|r| r
        .references
        .iter()
        .all(|r| r.category == TextureCategory::ExposureTargets)));
    for _ in 0..80 {
        renderer
            .accumulate_exposure(&mut target, Some(&white), false)
            .unwrap();
    }
    let actual = pixel(&renderer, &target);
    assert!(
        actual.iter().all(|c| c.abs_diff(255) <= 1),
        "constant image over 80 phases: {actual:?}"
    );
    assert_eq!(target.phase_count(), target.phase_index());
    drop(target);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    let mut canceled = renderer.exposure_target(16, 16, &[0.5, 0.5]).unwrap();
    renderer
        .accumulate_exposure(&mut canceled, Some(&background([1., 0., 0., 1.])), false)
        .unwrap();
    assert!(!canceled.complete());
    drop(canceled);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    let mut next = renderer.exposure_target(16, 16, &[0.5, 0.5]).unwrap();
    renderer
        .accumulate_exposure(&mut next, None, false)
        .unwrap();
    renderer
        .accumulate_exposure(&mut next, None, false)
        .unwrap();
    assert_eq!(pixel(&renderer, &next), [0; 4]);
    drop(next);
    let info = renderer.submission_memory_info(&renderer.texture_resource_ledger());
    assert_eq!(info.pending_submissions, 0);
    assert_eq!(info.known_retired_bytes, 0);
    assert!(info.peak_known_pending_bytes >= bytes);
    assert_eq!(info.submitted, info.completed);
    assert!(info.submitted >= 85);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn allocation_refusal_counts_all_live_targets_and_current_policy_without_partial_advancement() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    for weights in [
        vec![],
        vec![0.],
        vec![f64::NAN],
        vec![0.4, 0.4],
        vec![1. / 81.; 81],
    ] {
        assert!(renderer.exposure_target(16, 16, &weights).is_err());
    }
    assert!(renderer.exposure_target(0, 16, &[1.]).is_err());
    let limit = renderer.device.limits().max_texture_dimension_2d;
    let error = renderer.exposure_target(limit + 1, 1, &[1.]).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::TextureDimensions {
            width: limit + 1,
            height: 1,
            max_dimension: limit
        })
    );
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    let mut policy = renderer.memory_policy();
    policy.scratch_budget_bytes = 128 * 1024 * 1024;
    renderer.set_memory_policy(policy).unwrap();
    let mut targets: Vec<_> = (0..3)
        .map(|_| renderer.exposure_target(2048, 1024, &[1.]).unwrap())
        .collect();
    assert_eq!(renderer.exposure_texture_bytes(), 72 * 1024 * 1024);
    policy.scratch_budget_bytes = 64 * 1024 * 1024;
    renderer.set_memory_policy(policy).unwrap();
    let error = renderer
        .accumulate_exposure(&mut targets[0], None, false)
        .unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::ExposureTargets,
            required_bytes: 72 * 1024 * 1024,
            budget_bytes: 64 * 1024 * 1024
        })
    );
    assert!(error.to_string().contains("sample=0/1"));
    assert_eq!(targets[0].phase_index(), 0);
    drop(targets.pop());
    assert_eq!(renderer.exposure_texture_bytes(), 48 * 1024 * 1024);
    let error = renderer.exposure_target(2048, 1024, &[1.]).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::ExposureTargets,
            required_bytes: 72 * 1024 * 1024,
            budget_bytes: 64 * 1024 * 1024
        })
    );
    assert!(error
        .to_string()
        .contains("phase=allocate live=50331648 additional=25165824"));
    assert_eq!(renderer.exposure_texture_bytes(), 48 * 1024 * 1024);
    renderer
        .accumulate_exposure(&mut targets[0], None, false)
        .unwrap();
    assert!(targets[0].complete());
    let mut other = pollster::block_on(Renderer::headless()).unwrap();
    assert!(other
        .accumulate_exposure(&mut targets[1], None, false)
        .unwrap_err()
        .to_string()
        .contains("another renderer"));
    assert_eq!(targets[1].phase_index(), 0);
    drop(targets);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn desktop_bgra_and_rejected_unsubmitted_yuv_do_not_publish_partial_exposures() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ))
    .unwrap();
    let mut target = renderer.exposure_target(16, 16, &[0.5, 0.5]).unwrap();
    for color in [[1., 0., 0., 1.], [0., 0., 1., 1.]] {
        renderer
            .accumulate_exposure(&mut target, Some(&background(color)), false)
            .unwrap();
    }
    let actual = pixel(&renderer, &target);
    assert!(
        actual
            .iter()
            .zip([188u8, 0, 188, 255])
            .all(|(a, b)| a.abs_diff(b) <= 1),
        "{actual:?}"
    );
    let frame = motion_render::Yuv420Frame {
        width: 16,
        height: 16,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: motion_render::ChromaLayout::Uv,
        y: vec![235; 256],
        uv: vec![128; 128],
    };
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let token = renderer
        .encode_video_yuv(42, 1, 100, &frame, &mut encoder)
        .unwrap()
        .unwrap();
    let mut target = renderer.exposure_target(16, 16, &[1.]).unwrap();
    let mut scene = background([0.; 4]);
    // A pending source is checked by encode_internal before the phase is submitted.
    let mut p = Project::new(16, 16, 30, 60).unwrap();
    p.layers.push(Layer::solid(
        42,
        "pending video",
        [16.; 2],
        [8., 8., 0.],
        [1.; 4],
    ));
    let mut layer = Scene::new(&p).unwrap();
    layer
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene.layers = layer.layers;
    scene.layers[0].video = Some(motion_core::VideoSample {
        asset: 1,
        source_time_us: 100,
    });
    let error = renderer
        .accumulate_exposure(&mut target, Some(&scene), false)
        .unwrap_err();
    assert!(error.to_string().contains("unsubmitted"), "{error}");
    assert_eq!(target.phase_index(), 0);
    assert!(renderer.capture_exposure(&target).is_err());
    drop(encoder);
    drop(token);
    let canceled_error = renderer
        .accumulate_exposure(&mut target, Some(&scene), false)
        .unwrap_err();
    assert!(canceled_error
        .to_string()
        .contains("object=42 source=1 time_us=100 phase=video_ready"));
    assert!(canceled_error.to_string().contains("canceled"));
    assert_eq!(target.phase_index(), 0);
    renderer.upload_video_yuv(42, 1, 100, &frame).unwrap();
    renderer
        .accumulate_exposure(&mut target, Some(&scene), false)
        .unwrap();
    assert!(pixel(&renderer, &target).iter().all(|c| *c >= 254));
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cold_nested_mask_and_effect_rendering_preserve_transparent_pixels_through_exposure() {
    use motion_core::{
        masks::LayerMask,
        vector::{PathNode, VectorPath},
        Track,
    };
    let mut p = Project::new(32, 32, 30, 60).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.background = [0.; 4];
    let mut layer = Layer::solid(
        1,
        "masked spatial source",
        [24.; 2],
        [16., 16., 0.],
        [0.25, 0.65, 0.9, 0.7],
    );
    layer.transform.rotation.value[2] = 12.;
    layer.masks.push(LayerMask::new(
        1,
        VectorPath {
            tangents: Default::default(), id: 1,
            closed: true,
            nodes: [[4., 4.], [20., 4.], [20., 20.], [4., 20.]]
                .into_iter()
                .enumerate()
                .map(|(i, xy)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([xy[0], xy[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    ));
    let package = motion_effects::builtin::package().unwrap();
    let def = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "gaussian_blur")
        .unwrap();
    let mut effect = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        def,
        [24.; 2],
    );
    effect.params.get_mut("p0001").unwrap().track.value[0] = 2.;
    layer.effects.push(effect);
    p.layers.push(layer);
    p.rebuild_plugin_dependencies();
    let mut engine = motion_core::Engine::new(p).unwrap();
    engine
        .apply_batch(
            motion_core::parse_commands(
                &serde_json::json!({
                    "op":"composition","composition":motion_core::MAIN_COMPOSITION,
                    "action":{"kind":"precompose","objects":[1],"name":"exposure child"}
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    // Precompose supports flat source layers; the resulting reference is a
    // spatial plane exercising the supported nested/spatial render boundary.
    let mut project = engine.snapshot();
    project.layers[0].three_d = true;
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 5., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert!(scene.layers[0].three_d);
    assert_eq!(scene.nested.len(), 1);
    assert!(!scene.nested[0].scene.layers[0].masks.is_empty());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut target = renderer.exposure_target(32, 32, &[0.125; 8]).unwrap();
    for _ in 0..8 {
        renderer
            .accumulate_exposure(&mut target, Some(&scene), false)
            .unwrap();
    }
    let accumulated = renderer.capture_exposure(&target).unwrap();
    let reference = renderer.capture_target(32, 32).unwrap();
    let original = renderer.capture(&scene, &reference).unwrap().0;
    assert!(original
        .iter()
        .zip(&accumulated)
        .all(|(a, b)| a.abs_diff(*b) <= 1));
    assert_eq!(&accumulated[..4], &[0; 4]);
    assert!(accumulated.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
    assert!(renderer.texture_resource_ledger().resources().any(|r| r
        .references
        .iter()
        .any(|r| r.category == TextureCategory::NestedCompositions)));
    let bytes = renderer.texture_bytes();
    renderer.capture_exposure(&target).unwrap();
    assert_eq!(renderer.texture_bytes(), bytes);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn authored_rectangle_shutter_produces_real_image_and_keeps_an_unblurred_layer_fixed() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = Project::new(192, 96, 30, 60).unwrap();
    p.background = [0.; 4];
    p.layers.push(Layer::solid(
        1,
        "shutter rectangle",
        [24., 24.],
        [96., 32., 0.],
        [0.2, 0.7, 0.95, 1.],
    ));
    p.layers.push(Layer::solid(
        2,
        "center-time reference",
        [40., 8.],
        [96., 76., 0.],
        [0.95, 0.5, 0.15, 1.],
    ));
    let mut scene = Scene::new(&p).unwrap();
    scene
        .sample(&p, 10., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let reference = renderer.capture_target(192, 96).unwrap();
    let sharp = renderer.capture(&scene, &reference).unwrap().0;
    let clock = ExposureClock::new(
        10.,
        &[(
            1,
            Shutter {
                frames: 2.,
                samples: 8,
            },
        )],
        false,
    )
    .unwrap();
    let weights: Vec<_> = clock.phases().iter().map(|p| p.weight).collect();
    let started = Instant::now();
    let mut target = renderer.exposure_target(192, 96, &weights).unwrap();
    let center_x = f64::from(scene.layers[0].model.w_axis.x);
    // This tests the accumulator and clock with explicitly authored sample
    // scenes. Production per-layer scene/camera/parent evaluation remains separate.
    for i in 0..clock.phases().len() {
        scene.layers[0].model.w_axis.x =
            (center_x + (clock.layer_frame(1, i).unwrap() - 10.) * 48.) as f32;
        renderer
            .accumulate_exposure(&mut target, Some(&scene), false)
            .unwrap();
    }
    let blurred = renderer.capture_exposure(&target).unwrap();
    let first_us = started.elapsed().as_micros();
    assert_ne!(sharp, blurred);
    for row in 68..84 {
        assert_eq!(
            &sharp[row * 192 * 4..(row + 1) * 192 * 4],
            &blurred[row * 192 * 4..(row + 1) * 192 * 4]
        );
    }
    let alpha = |pixels: &[u8]| pixels.chunks_exact(4).map(|p| u64::from(p[3])).sum::<u64>();
    assert!(alpha(&sharp).abs_diff(alpha(&blurred)) < alpha(&sharp) / 100);
    assert_eq!(&blurred[..4], &[0; 4]);
    let mut warm_us = vec![];
    for _ in 0..3 {
        let started = Instant::now();
        let mut next = renderer.exposure_target(192, 96, &weights).unwrap();
        for i in 0..clock.phases().len() {
            scene.layers[0].model.w_axis.x =
                (center_x + (clock.layer_frame(1, i).unwrap() - 10.) * 48.) as f32;
            renderer
                .accumulate_exposure(&mut next, Some(&scene), false)
                .unwrap();
        }
        assert_eq!(blurred, renderer.capture_exposure(&next).unwrap());
        warm_us.push(started.elapsed().as_micros());
    }
    if let Some(folder) = std::env::var_os("MOTION_SAMPLING_ARTIFACT_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (name, pixels) in [
            ("exposure-sharp.png", sharp),
            ("exposure-accumulation.png", blurred),
        ] {
            image::RgbaImage::from_raw(192, 96, pixels)
                .unwrap()
                .save(folder.join(name))
                .unwrap();
        }
        std::fs::write(folder.join("exposure-cost.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "width":192,"height":96,"phases":8,"target_texture_bytes":target.texture_bytes(),
            "renderer_texture_bytes":renderer.texture_bytes(),"first_capture_us":first_us,"warm_capture_us":warm_us,
            "scope":"Explicit authored sample scenes through actual GPU accumulator; allocation, rendering and readback included. Not production host playback, frozen protocol or FPS."
        })).unwrap()).unwrap();
    }
}
